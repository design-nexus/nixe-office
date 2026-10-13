//! Excel workbooks. Reading parses the XML directly, so styles come along
//! (bold, italic, colours, fills, alignment, number formats, sizes); writing
//! uses rust_xlsxwriter, chart included.

use super::formula;
use super::model::*;
use super::rules::Test;
use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::io::Cursor;

const MAX_PART: u64 = 64 * 1024 * 1024;

fn part(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<String> {
    let mut f = zip.by_index(crate::container::entry_index(zip, name)?).ok()?;
    if f.size() > MAX_PART {
        return None;
    }
    String::from_utf8(crate::container::read_capped(&mut f, MAX_PART).ok()?).ok().map(crate::container::transitional)
}

/// Relationship id → target, resolved against `base` (the folder of the part).
fn relationships(xml: &str, base: &str) -> HashMap<String, String> {
    let Ok(doc) = crate::xml::parse(xml) else { return HashMap::new() };
    doc.descendants()
        .filter(|n| n.has_tag_name("Relationship"))
        .filter_map(|n| {
            let target = n.attribute("Target")?;
            let path = if let Some(abs) = target.strip_prefix('/') { abs.to_string() } else { resolve(base, target) };
            Some((n.attribute("Id")?.to_string(), path))
        })
        .collect()
}

fn resolve(base: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = base.split('/').filter(|p| !p.is_empty()).collect();
    for seg in rel.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn folder(path: &str) -> &str {
    path.rsplit_once('/').map(|(a, _)| a).unwrap_or("")
}

fn rels_of(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{path}.rels"),
    }
}

/// `FFRRGGBB` or `RRGGBB` → `#rrggbb`.
fn argb(v: &str) -> Option<String> {
    let v = v.trim();
    let hex = if v.len() == 8 { &v[2..] } else { v };
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())).then(|| format!("#{}", hex.to_lowercase()))
}

/// A cell's number format: one of ours when it matches, else Excel's code kept as is.
pub(super) fn num_fmt(id: u32, code: Option<&str>) -> (NumFmt, Option<String>) {
    // Excel's built-in formats, by number.
    let builtin = match id {
        0 | 49 => return (NumFmt::General, None),
        4 => return (NumFmt::Decimal, None),
        5..=8 => return (NumFmt::Currency, None),
        14 => return (NumFmt::Date, None),
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        9 => "0%",
        10 => "0.00%",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "m/d/yyyy h:mm",
        37 => "#,##0;(#,##0)",
        38 => "#,##0;[Red](#,##0)",
        39 => "#,##0.00;(#,##0.00)",
        40 => "#,##0.00;[Red](#,##0.00)",
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mm:ss.0",
        _ => "",
    };
    if !builtin.is_empty() {
        return (NumFmt::General, Some(builtin.to_string()));
    }
    let Some(code) = code else { return (NumFmt::General, None) };
    // The codes this app writes for its own formats.
    match code {
        "General" => return (NumFmt::General, None),
        "#,##0.00" => return (NumFmt::Decimal, None),
        "$#,##0.00" | "\"$\"#,##0.00" => return (NumFmt::Currency, None),
        "0.0%" => return (NumFmt::Percent, None),
        "yyyy-mm-dd" => return (NumFmt::Date, None),
        _ => {}
    }
    if super::numfmt::render(code, 1.0).is_some() {
        return (NumFmt::General, Some(code.to_string()));
    }
    (num_kind(code), None)
}

/// The nearest of our formats to a code we can't show.
fn num_kind(code: &str) -> NumFmt {
    let lower = code.to_lowercase();
    // Strip quoted text and colour codes before looking at the pattern.
    let bare: String = lower.split('"').step_by(2).collect::<String>().split(['[', ']']).step_by(2).collect();
    if bare.contains('%') {
        NumFmt::Percent
    } else if code.contains('$') || code.contains('€') || code.contains('£') {
        NumFmt::Currency
    } else if bare.contains('y') || bare.contains('d') {
        NumFmt::Date
    } else if bare.contains("0.0") || bare.contains("#.#") {
        NumFmt::Decimal
    } else {
        NumFmt::General
    }
}

/// What reading kept and what it had to leave out.
pub struct Opened {
    pub workbook: Workbook,
    /// Cells past the grid's size were left out.
    pub truncated: bool,
    /// Some conditional formats Sheets can't show were left out.
    pub dropped_rules: bool,
    /// Pivot tables rebuilt as live summaries.
    pub pivots: usize,
}

/// A pivot table as read: where it sits, and what it summarizes.
pub(super) struct Pivot {
    /// The sheet it's on, and its cells there.
    pub sheet: usize,
    pub at: Area,
    /// The data's sheet (by name) and cells, header row first.
    pub source: (String, Area),
    /// Columns of the data, counted from its first: grouped down, grouped across, totalled.
    pub rows: usize,
    pub across: Option<usize>,
    pub value: usize,
    pub how: super::summary::Summary,
}

/// Each pivot table rebuilt where it was, as a live summary of its data. Returns how many.
pub(super) fn rebuild_pivots(wb: &mut Workbook, pivots: &[Pivot]) -> usize {
    let mut done = 0;
    for p in pivots {
        let Some(src) = wb.sheets.iter().position(|s| s.name.eq_ignore_ascii_case(&p.source.0)) else { continue };
        let data = p.source.1;
        let width = (data.right - data.left) as usize;
        if p.rows > width || p.value > width || p.across.is_some_and(|a| a > width) || p.sheet >= wb.sheets.len() {
            continue;
        }
        // The pivot's frozen values go; the summary takes their place.
        let s = &mut wb.sheets[p.sheet];
        s.cells.retain(|(r, c), _| !p.at.contains(*r, *c));
        s.merges.retain(|m| !m.overlaps(&p.at));
        let req = super::summary::Request {
            sheet: src,
            data,
            group: data.left + p.rows as u32,
            across: p.across.map(|a| data.left + a as u32),
            value: data.left + p.value as u32,
            how: p.how,
        };
        if super::summary::summarize_at(wb, &req, p.sheet, (p.at.top, p.at.left)) {
            done += 1;
        }
    }
    done
}

/// A pivot table's definition, with its cache's source.
fn read_pivot(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, path: &str, sheet: usize) -> Option<Pivot> {
    use super::summary::Summary;
    let xml = part(zip, path)?;
    let doc = crate::xml::parse(&xml).ok()?;
    let at = doc.descendants().find(|n| n.has_tag_name("location")).and_then(|n| n.attribute("ref")).and_then(Area::parse)?;
    let fields = |list: &str| -> Vec<usize> {
        doc.descendants()
            .find(|n| n.has_tag_name(list))
            .map(|l| {
                l.children()
                    .filter(|f| f.has_tag_name("field"))
                    // -2 stands for the values themselves.
                    .filter_map(|f| f.attribute("x").and_then(|x| x.parse::<i64>().ok()).filter(|x| *x >= 0).map(|x| x as usize))
                    .collect()
            })
            .unwrap_or_default()
    };
    let rows = *fields("rowFields").first()?;
    let across = fields("colFields").first().copied();
    let data = doc.descendants().find(|n| n.has_tag_name("dataField"))?;
    let value = data.attribute("fld")?.parse().ok()?;
    let how = match data.attribute("subtotal") {
        Some("count" | "countNums") => Summary::Count,
        Some("average") => Summary::Average,
        Some("max") => Summary::Max,
        Some("min") => Summary::Min,
        _ => Summary::Sum,
    };
    // The cache says where the data is.
    let rels = relationships(&part(zip, &rels_of(path)).unwrap_or_default(), folder(path));
    let cache = rels.values().find(|t| t.contains("pivotCacheDefinition"))?;
    let cxml = part(zip, cache)?;
    let cdoc = crate::xml::parse(&cxml).ok()?;
    let ws = cdoc.descendants().find(|n| n.has_tag_name("worksheetSource"))?;
    let source = (ws.attribute("sheet")?.to_string(), Area::parse(&ws.attribute("ref")?.replace('$', ""))?);
    Some(Pivot { sheet, at, source, rows, across, value, how })
}

pub fn read(bytes: &[u8]) -> Result<Opened> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| anyhow::anyhow!(crate::container::not_a_package(bytes, "Excel workbook")))?;
    let root = relationships(&part(&mut zip, "_rels/.rels").unwrap_or_default(), "");
    let wb_path = root
        .values()
        .find(|p| p.ends_with(".xml") && p.contains("workbook"))
        .cloned()
        .unwrap_or_else(|| "xl/workbook.xml".into());
    let wb_xml = part(&mut zip, &wb_path).context("the workbook is missing")?;
    let wb_doc = crate::xml::parse(&wb_xml).context("the workbook is damaged")?;
    let rels = relationships(&part(&mut zip, &rels_of(&wb_path)).unwrap_or_default(), folder(&wb_path));
    let dir = folder(&wb_path).to_string();

    let active = wb_doc
        .descendants()
        .find(|n| n.has_tag_name("workbookView"))
        .and_then(|n| n.attribute("activeTab"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0usize);
    let sheets: Vec<(String, String)> = wb_doc
        .descendants()
        .filter(|n| n.has_tag_name("sheet"))
        .filter_map(|n| {
            let id = n.attributes().find(|a| a.name() == "id").map(|a| a.value())?;
            Some((n.attribute("name").unwrap_or("Sheet").to_string(), rels.get(id)?.clone()))
        })
        .collect();
    if sheets.is_empty() {
        bail!("the workbook has no sheets");
    }

    // Shared strings.
    let shared: Vec<String> = part(&mut zip, &format!("{dir}/sharedStrings.xml"))
        .and_then(|x| {
            let doc = crate::xml::parse(&x).ok()?;
            Some(
                doc.descendants()
                    .filter(|n| n.has_tag_name("si"))
                    .map(|si| {
                        cell_text(
                            si.descendants()
                                .filter(|t| t.has_tag_name("t") && !t.ancestors().any(|a| a.has_tag_name("rPh")))
                                .filter_map(|t| t.text())
                                .collect(),
                        )
                    })
                    .collect(),
            )
        })
        .unwrap_or_default();

    // Styles: one Format per cellXfs entry, and the looks conditional formats use.
    let mut xfs: Vec<Format> = Vec::new();
    let mut dxfs: Vec<Look> = Vec::new();
    if let Some(x) = part(&mut zip, &format!("{dir}/styles.xml"))
        && let Ok(doc) = crate::xml::parse(&x)
    {
        let codes: HashMap<u32, String> = doc
            .descendants()
            .filter(|n| n.has_tag_name("numFmt"))
            .filter_map(|n| Some((n.attribute("numFmtId")?.parse().ok()?, n.attribute("formatCode")?.to_string())))
            .collect();
        let fonts: Vec<(bool, bool, Option<String>)> = doc
            .descendants()
            .find(|n| n.has_tag_name("fonts"))
            .map(|fs| {
                fs.children()
                    .filter(|n| n.has_tag_name("font"))
                    .map(|f| {
                        let on = |tag: &str| {
                            f.children()
                                .find(|c| c.has_tag_name(tag))
                                .is_some_and(|c| c.attribute("val").is_none_or(|v| v != "0" && v != "false"))
                        };
                        let color =
                            f.children().find(|c| c.has_tag_name("color")).and_then(|c| c.attribute("rgb")).and_then(argb);
                        (on("b"), on("i"), color.filter(|c| c != "#000000"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let fills: Vec<Option<String>> = doc
            .descendants()
            .find(|n| n.has_tag_name("fills"))
            .map(|fs| {
                fs.children()
                    .filter(|n| n.has_tag_name("fill"))
                    .map(|f| {
                        let p = f.descendants().find(|c| c.has_tag_name("patternFill"))?;
                        if p.attribute("patternType") != Some("solid") {
                            return None;
                        }
                        p.children().find(|c| c.has_tag_name("fgColor")).and_then(|c| c.attribute("rgb")).and_then(argb)
                    })
                    .collect()
            })
            .unwrap_or_default();
        // Which edges each border style draws.
        let borders: Vec<u8> = doc
            .descendants()
            .find(|n| n.has_tag_name("borders"))
            .map(|bs| {
                bs.children()
                    .filter(|n| n.has_tag_name("border"))
                    .map(|b| {
                        let has = |tag: &str| {
                            b.children().any(|c| {
                                (c.has_tag_name(tag) || (tag == "left" && c.has_tag_name("start")) || (tag == "right" && c.has_tag_name("end")))
                                    && c.attribute("style").is_some_and(|s| s != "none")
                            })
                        };
                        (if has("top") { TOP } else { 0 })
                            | (if has("bottom") { BOTTOM } else { 0 })
                            | (if has("left") { LEFT } else { 0 })
                            | (if has("right") { RIGHT } else { 0 })
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Some(list) = doc.descendants().find(|n| n.has_tag_name("dxfs")) {
            for d in list.children().filter(|n| n.has_tag_name("dxf")) {
                let font = d.children().find(|c| c.has_tag_name("font"));
                let color = font
                    .and_then(|f| f.children().find(|c| c.has_tag_name("color")))
                    .and_then(|c| c.attribute("rgb"))
                    .and_then(argb);
                let bold = font.is_some_and(|f| f.children().any(|c| c.has_tag_name("b")));
                // Conditional fills keep their colour in bgColor.
                let fill = d
                    .descendants()
                    .find(|c| c.has_tag_name("bgColor") || c.has_tag_name("fgColor"))
                    .and_then(|c| c.attribute("rgb"))
                    .and_then(argb);
                dxfs.push(Look { fill, color, bold });
            }
        }
        if let Some(cell_xfs) = doc.descendants().find(|n| n.has_tag_name("cellXfs")) {
            for xf in cell_xfs.children().filter(|n| n.has_tag_name("xf")) {
                let get = |a: &str| xf.attribute(a).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
                let (bold, italic, color) = fonts.get(get("fontId")).cloned().unwrap_or((false, false, None));
                let id = get("numFmtId") as u32;
                let alignment = xf.children().find(|c| c.has_tag_name("alignment"));
                let wrap = alignment.and_then(|a| a.attribute("wrapText")).is_some_and(|v| v == "1" || v == "true");
                let align =
                    alignment.and_then(|a| a.attribute("horizontal")).and_then(|h| {
                        match h {
                            "left" => Some(HAlign::Left),
                            "center" | "centerContinuous" => Some(HAlign::Center),
                            "right" => Some(HAlign::Right),
                            _ => None,
                        }
                    });
                let (number, pattern) = num_fmt(id, codes.get(&id).map(String::as_str));
                xfs.push(Format {
                    number,
                    pattern,
                    bold,
                    italic,
                    align,
                    fill: fills.get(get("fillId")).cloned().flatten(),
                    color,
                    borders: borders.get(get("borderId")).copied().unwrap_or(0),
                    wrap,
                    checkbox: false,
                });
            }
        }
    }

    let mut out = Workbook { sheets: Vec::new(), active: 0, names: Vec::new() };
    let mut pivots: Vec<Pivot> = Vec::new();
    let mut text_budget: usize = READ_BUDGET;
    let mut truncated = false;
    let mut dropped_rules = false;
    // Defined names: Excel's own (print areas and the like) start with _xlnm.
    let sheet_names: Vec<String> = sheets.iter().map(|(n, _)| n.clone()).collect();
    let names: Vec<DefinedName> = wb_doc
        .descendants()
        .filter(|n| n.has_tag_name("definedName"))
        .filter_map(|n| {
            let name = n.attribute("name")?;
            if name.starts_with("_xlnm.") || name.starts_with('_') && n.attribute("hidden") == Some("1") {
                return None;
            }
            let sheet = n.attribute("localSheetId").and_then(|i| i.parse::<usize>().ok()).and_then(|i| sheet_names.get(i).cloned());
            let formula = n.text()?.trim().replace("_xlfn._xlws.", "").replace("_xlfn.", "");
            (!formula.is_empty()).then(|| DefinedName { name: name.to_string(), sheet, formula })
        })
        .take(10_000)
        .collect();
    // Print areas are names in the workbook, one per sheet by its position.
    let print_areas: HashMap<usize, Area> = wb_doc
        .descendants()
        .filter(|n| n.has_tag_name("definedName") && n.attribute("name") == Some("_xlnm.Print_Area"))
        .filter_map(|n| {
            let i = n.attribute("localSheetId")?.parse().ok()?;
            // 'Sheet 1'!$A$1:$C$9 (the first range, when there are several).
            let text = n.text()?.split(',').next()?;
            let range = text.rsplit('!').next()?.replace('$', "");
            Some((i, Area::parse(&range)?))
        })
        .collect();
    for (index, (name, path)) in sheets.into_iter().take(MAX_SHEETS).enumerate() {
        let mut sheet = Sheet::new(&name);
        sheet.print_area = print_areas.get(&index).copied();
        sheet.header = false;
        let Some(xml) = part(&mut zip, &path) else {
            out.sheets.push(sheet);
            continue;
        };
        let doc = crate::xml::parse(&xml).with_context(|| format!("the sheet “{name}” is damaged"))?;
        for col in doc.descendants().filter(|n| n.has_tag_name("col")) {
            let hidden = col.attribute("hidden").is_some_and(|v| v == "1" || v == "true");
            if hidden
                && let (Some(min), Some(max)) = (
                    col.attribute("min").and_then(|v| v.parse::<u32>().ok()),
                    col.attribute("max").and_then(|v| v.parse::<u32>().ok()),
                )
            {
                sheet.hidden_cols.extend(min.saturating_sub(1)..max.min(COLS));
            }
            let (Some(min), Some(max), Some(width)) = (
                col.attribute("min").and_then(|v| v.parse::<u32>().ok()),
                col.attribute("max").and_then(|v| v.parse::<u32>().ok()),
                col.attribute("width").and_then(|v| v.parse::<f64>().ok()),
            ) else {
                continue;
            };
            // The stored width already counts Excel's padding: 7 pixels a character.
            if (col.attribute("customWidth").is_none_or(|v| v == "0") && (width - 9.14).abs() < 0.5) || width < 0.5 {
                continue;
            }
            for c in min.saturating_sub(1)..max.min(COLS) {
                let px = (width * 7.0).round() as u32;
                if px != COL_W {
                    sheet.col_widths.insert(c, px.clamp(36, 800));
                }
            }
        }
        // Shrunk to fit: so many pages across and down (0 is as many as it takes).
        if doc.descendants().any(|n| n.has_tag_name("pageSetUpPr") && matches!(n.attribute("fitToPage"), Some("1" | "true"))) {
            let setup = doc.descendants().find(|n| n.has_tag_name("pageSetup"));
            let pages = |k: &str| setup.and_then(|n| n.attribute(k)).and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
            sheet.print_fit = match (pages("fitToWidth"), pages("fitToHeight")) {
                (1, 1) => Fit::Page,
                (1, 0) => Fit::Width,
                _ => Fit::Actual,
            };
        }
        if let Some(pane) = doc
            .descendants()
            .find(|n| n.has_tag_name("pane") && n.attribute("state").is_some_and(|s| s.starts_with("frozen")))
        {
            let split = |k: &str| pane.attribute(k).and_then(|v| v.parse::<f64>().ok()).map_or(0, |v| v as u32);
            sheet.freeze_rows = split("ySplit").min(MAX_FROZEN);
            sheet.freeze_cols = split("xSplit").min(MAX_FROZEN);
            if sheet.freeze_rows == 1 {
                sheet.header = true;
            }
        }
        // Shared formulas: the first cell holds the text, the others say which one.
        let mut shared_f: HashMap<String, (String, u32, u32)> = HashMap::new();
        let mut spill_areas: Vec<Area> = Vec::new();
        for row in doc.descendants().filter(|n| n.has_tag_name("row")) {
            let ri = row.attribute("r").and_then(|v| v.parse::<u32>().ok()).map(|r| r.saturating_sub(1));
            if let Some(r) = ri
                && r < ROWS
                && row.attribute("hidden").is_some_and(|v| v == "1" || v == "true")
            {
                sheet.hidden_rows.insert(r);
            }
            if let (Some(r), Some(ht)) = (ri, row.attribute("ht").and_then(|v| v.parse::<f64>().ok()))
                && row.attribute("customHeight").is_some_and(|v| v == "1" || v == "true")
                && r < ROWS
                && ht >= 0.5
            {
                let px = (ht / 0.75).round() as u32;
                if px != ROW_H {
                    sheet.row_heights.insert(r, px.clamp(20, 400));
                }
            }
            for c in row.children().filter(|n| n.has_tag_name("c")) {
                let Some((r, col)) = c.attribute("r").and_then(formula::parse_cell) else { continue };
                if r >= ROWS || col >= COLS {
                    truncated = true;
                    continue;
                }
                let format =
                    c.attribute("s").and_then(|s| s.parse::<usize>().ok()).and_then(|i| xfs.get(i)).cloned().unwrap_or_default();
                let value = c.children().find(|n| n.has_tag_name("v")).and_then(|v| v.text()).unwrap_or("");
                let mut raw = String::new();
                let f = c.children().find(|n| n.has_tag_name("f"));
                // A spilled block's values are kept beside its formula; we work them out again.
                if f.is_none() && spill_areas.iter().any(|a| a.contains(r, col) && (a.top, a.left) != (r, col)) {
                    if format != Format::default() {
                        sheet.cells.insert((r, col), Cell { raw, format, comment: None });
                    }
                    continue;
                }
                if let Some(f) = f {
                    let text = f.text().unwrap_or("").trim();
                    if f.attribute("t") == Some("array")
                        && let Some(a) = f.attribute("ref").and_then(Area::parse)
                    {
                        spill_areas.push(a);
                    }
                    if f.attribute("t") == Some("shared")
                        && let Some(si) = f.attribute("si")
                    {
                        if !text.is_empty() {
                            shared_f.insert(si.to_string(), (text.to_string(), r, col));
                            raw = format!("={text}");
                        } else if let Some((base, br, bc)) = shared_f.get(si) {
                            raw = formula::shift(&format!("={base}"), r as i64 - *br as i64, col as i64 - *bc as i64, ROWS, COLS);
                        }
                    } else if !text.is_empty() {
                        raw = format!("={text}");
                    }
                    // Excel marks functions newer than 2007 (`_xlfn.TEXTJOIN`).
                    raw = raw.replace("_xlfn._xlws.", "").replace("_xlfn.", "").replace("_xlws.", "");
                }
                if raw.is_empty() {
                    raw = match c.attribute("t") {
                        Some("s") => value.parse::<usize>().ok().and_then(|i| shared.get(i)).cloned().unwrap_or_default(),
                        Some("inlineStr") => c.descendants().filter(|n| n.has_tag_name("t")).filter_map(|t| t.text()).collect(),
                        Some("b") => if value == "1" { "TRUE" } else { "FALSE" }.into(),
                        Some("str") | Some("e") => value.to_string(),
                        _ => value.parse::<f64>().map(formula::general).unwrap_or_else(|_| value.to_string()),
                    };
                }
                if raw.is_empty() && format == Format::default() {
                    continue;
                }
                // A file can repeat one long string in every cell; past a sensible
                // amount of text in all, the rest is left out.
                let raw = cell_text(raw);
                text_budget = text_budget.saturating_sub(raw.len() + CELL_COST);
                if text_budget == 0 {
                    truncated = true;
                    continue;
                }
                sheet.cells.insert((r, col), Cell { raw, format, comment: None });
            }
        }
        for m in doc.descendants().filter(|n| n.has_tag_name("mergeCell")) {
            if let Some(a) = m.attribute("ref").and_then(Area::parse)
                && !a.is_cell()
                && !sheet.merges.iter().any(|x| x.overlaps(&a))
            {
                sheet.merges.push(a);
            }
        }
        // Dropdown lists: choices typed in quotes ("Paid,Unpaid"), or cells (=$D$1:$D$5).
        for dv in doc.descendants().filter(|n| n.has_tag_name("dataValidation") && n.attribute("type") == Some("list")) {
            let Some(f) = dv.children().find(|n| n.has_tag_name("formula1")).and_then(|n| n.text()) else { continue };
            let f = f.trim();
            let (items, source) = match f.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
                Some(list) => (list.split(',').map(|x| x.replace("\"\"", "\"").trim().to_string()).collect(), None),
                None => (Vec::new(), Some(f.trim_start_matches('=').replace('$', ""))),
            };
            for a in dv.attribute("sqref").unwrap_or("").split_whitespace().filter_map(Area::parse) {
                sheet.lists.push(super::lists::ListRule { range: a.name(), items: items.clone(), source: source.clone() });
            }
        }
        // Rules come grouped by range; `priority` gives their order across the sheet.
        let mut rules: Vec<(i64, Rule)> = Vec::new();
        for cf in doc.descendants().filter(|n| n.has_tag_name("conditionalFormatting")) {
            // "A1:A9 C1:C9": one rule per block.
            let ranges: Vec<Area> = cf.attribute("sqref").unwrap_or("").split_whitespace().filter_map(Area::parse).collect();
            for rule in cf.children().filter(|n| n.has_tag_name("cfRule")) {
                let Some(test) = cf_test(rule) else {
                    dropped_rules = true;
                    continue;
                };
                let look = rule
                    .attribute("dxfId")
                    .and_then(|v| v.parse::<usize>().ok())
                    .and_then(|i| dxfs.get(i))
                    .cloned()
                    .unwrap_or_default();
                let priority = rule.attribute("priority").and_then(|v| v.parse().ok()).unwrap_or(0);
                for a in &ranges {
                    rules.push((
                        priority,
                        Rule { range: a.name(), test: test.clone(), fill: look.fill.clone(), color: look.color.clone(), bold: look.bold },
                    ));
                }
            }
        }
        rules.sort_by_key(|(p, _)| *p);
        sheet.rules = rules.into_iter().map(|(_, r)| r).collect();
        // Comments live in their own part, linked from the sheet.
        let sheet_rels = relationships(&part(&mut zip, &rels_of(&path)).unwrap_or_default(), folder(&path));
        // Pivot tables: rebuilt once every sheet is in.
        for target in sheet_rels.values().filter(|t| t.contains("/pivotTables/")) {
            if let Some(p) = read_pivot(&mut zip, target, out.sheets.len()) {
                pivots.push(p);
            }
        }
        // Tables: their range, name and look.
        for target in sheet_rels.values().filter(|t| t.contains("/tables/")) {
            let Some(x) = part(&mut zip, target) else { continue };
            let Ok(tdoc) = crate::xml::parse(&x) else { continue };
            let t = tdoc.root_element();
            let Some(a) = t.attribute("ref").and_then(Area::parse) else { continue };
            let info = t.children().find(|n| n.has_tag_name("tableStyleInfo"));
            let style = info.and_then(|i| i.attribute("name")).unwrap_or("");
            sheet.tables.push(TableRegion {
                name: t.attribute("displayName").or(t.attribute("name")).unwrap_or("Table").to_string(),
                range: a.name(),
                look: TABLE_LOOKS.iter().position(|l| l.3 == style).unwrap_or(0),
                header: t.attribute("headerRowCount").is_none_or(|v| v != "0"),
                banded: info.and_then(|i| i.attribute("showRowStripes")).is_none_or(|v| v == "1" || v == "true"),
                total: t.attribute("totalsRowCount").is_some_and(|v| v != "0"),
            });
        }
        // Charts drawn on the sheet: where they sit, and the cells they chart.
        for target in sheet_rels.values().filter(|t| t.contains("/drawings/") && t.ends_with(".xml")) {
            let Some(x) = part(&mut zip, target) else { continue };
            let Ok(ddoc) = crate::xml::parse(&x) else { continue };
            let drels = relationships(&part(&mut zip, &rels_of(target)).unwrap_or_default(), folder(target));
            for anchor in ddoc.root_element().children().filter(|n| n.tag_name().name().ends_with("Anchor")) {
                let Some(rid) = anchor
                    .descendants()
                    .find(|n| n.tag_name().name() == "chart")
                    .and_then(|c| c.attributes().find(|a| a.name() == "id").map(|a| a.value()))
                else {
                    continue;
                };
                let Some(chart_xml) = drels.get(rid).and_then(|t| part(&mut zip, t)) else { continue };
                let Some((kind, range)) = chart_source(&chart_xml) else { continue };
                // Pixels from the cells' top left: whole cells, then the offset (EMU) into one.
                let corner = |tag: &str| -> Option<(f64, f64)> {
                    let n = anchor.children().find(|c| c.tag_name().name() == tag)?;
                    let get = |k: &str| n.children().find(|c| c.tag_name().name() == k).and_then(|c| c.text()).and_then(|t| t.trim().parse::<f64>().ok());
                    let (col, row) = (get("col")? as u32, get("row")? as u32);
                    let x: f64 = (0..col.min(COLS)).map(|c| sheet.col_w(c) as f64).sum::<f64>() + get("colOff").unwrap_or(0.0) / 9525.0;
                    let y: f64 = (0..row.min(ROWS)).map(|r| sheet.row_h(r) as f64).sum::<f64>() + get("rowOff").unwrap_or(0.0) / 9525.0;
                    Some((x, y))
                };
                let Some((x, y)) = corner("from") else { continue };
                let size = anchor.children().find(|c| c.tag_name().name() == "ext").and_then(|e| {
                    Some((e.attribute("cx")?.parse::<f64>().ok()? / 9525.0, e.attribute("cy")?.parse::<f64>().ok()? / 9525.0))
                });
                let (w, h) = match (corner("to"), size) {
                    (Some((x2, y2)), _) => (x2 - x, y2 - y),
                    (None, Some(s)) => s,
                    _ => (420.0, 260.0),
                };
                let text = chart_titles(&chart_xml);
                sheet.charts.push(PlacedChart { range, kind, text, x, y, w: w.max(160.0), h: h.max(120.0) });
            }
        }
        for target in sheet_rels.values().filter(|t| t.contains("comments")) {
            let Some(x) = part(&mut zip, target) else { continue };
            let Ok(cdoc) = crate::xml::parse(&x) else { continue };
            let authors: Vec<&str> =
                cdoc.descendants().filter(|n| n.has_tag_name("author")).map(|a| a.text().unwrap_or("")).collect();
            for c in cdoc.descendants().filter(|n| n.has_tag_name("comment")) {
                let Some((r, col)) = c.attribute("ref").and_then(formula::parse_cell) else { continue };
                if r >= ROWS || col >= COLS {
                    continue;
                }
                let text: String = c
                    .descendants()
                    .filter(|t| t.has_tag_name("t"))
                    .filter_map(|t| t.text())
                    .collect::<String>()
                    .trim()
                    .to_string();
                // Excel starts a note with "Author:" on a line of its own.
                let author = c.attribute("authorId").and_then(|i| i.parse::<usize>().ok()).and_then(|i| authors.get(i));
                let text = match author.and_then(|a| text.strip_prefix(&format!("{a}:"))) {
                    Some(rest) if !author.is_some_and(|a| a.is_empty()) => rest.trim().to_string(),
                    _ => text,
                };
                if !text.is_empty() {
                    sheet.cells.entry((r, col)).or_default().comment = Some(text);
                }
            }
        }
        out.sheets.push(sheet);
    }
    out.active = active.min(out.sheets.len().saturating_sub(1));
    out.names = names;
    let pivots = rebuild_pivots(&mut out, &pivots);
    Ok(Opened { workbook: out, truncated, dropped_rules, pivots })
}

/// An Excel chart's kind and the block of cells it charts (on its own sheet).
fn chart_source(xml: &str) -> Option<(ChartKind, String)> {
    let doc = crate::xml::parse(xml).ok()?;
    let plot = doc.descendants().find(|n| n.tag_name().name() == "plotArea")?;
    let node = plot.children().find(|n| n.tag_name().name().ends_with("Chart"))?;
    let stacked = node
        .children()
        .find(|c| c.tag_name().name() == "grouping")
        .and_then(|g| g.attribute("val"))
        .is_some_and(|v| v.contains("tacked"));
    let kind = match node.tag_name().name() {
        "barChart" | "bar3DChart" if stacked => ChartKind::StackedBar,
        "barChart" | "bar3DChart" if crate::present::pptx_more::bar_dir(&node) => ChartKind::HBar,
        "barChart" | "bar3DChart" => ChartKind::Bar,
        "lineChart" | "line3DChart" => ChartKind::Line,
        "areaChart" | "area3DChart" if stacked => ChartKind::StackedArea,
        "areaChart" | "area3DChart" => ChartKind::Area,
        "doughnutChart" => ChartKind::Doughnut,
        "pieChart" | "pie3DChart" => ChartKind::Pie,
        "scatterChart" => ChartKind::Scatter,
        _ => return None,
    };
    // Every cell reference the series use (`'Sheet 1'!$B$2:$B$9`), joined into one block.
    let mut area: Option<Area> = None;
    for f in node.descendants().filter(|n| n.tag_name().name() == "f") {
        let text = f.text().unwrap_or("");
        let cells = text.rsplit('!').next().unwrap_or(text).replace('$', "");
        let Some(a) = Area::parse(&cells) else { continue };
        area = Some(match area {
            None => a,
            Some(b) => Area::new(a.top.min(b.top), a.left.min(b.left), a.bottom.max(b.bottom), a.right.max(b.right)),
        });
    }
    Some((kind, area?.name()))
}

/// A conditional format's look, from styles.xml's `dxfs`.
#[derive(Clone, Default)]
struct Look {
    fill: Option<String>,
    color: Option<String>,
    bold: bool,
}

/// The highlight test a `cfRule` stands for, when it's one Sheets has.
fn cf_test(rule: roxmltree::Node) -> Option<Test> {
    let formulas: Vec<f64> = rule
        .children()
        .filter(|n| n.has_tag_name("formula"))
        .filter_map(|f| f.text().and_then(|t| t.trim().parse().ok()))
        .collect();
    let first = formulas.first().copied();
    Some(match rule.attribute("type")? {
        "cellIs" => match rule.attribute("operator")? {
            "greaterThan" => Test::Greater { value: first? },
            "lessThan" => Test::Less { value: first? },
            "equal" => Test::Equal { value: first? },
            "between" => Test::Between { low: first?, high: *formulas.get(1)? },
            _ => return None,
        },
        "containsText" => Test::Contains { text: rule.attribute("text")?.to_string() },
        "containsBlanks" => Test::Blank,
        "containsErrors" => Test::Error,
        "top10" if rule.attribute("bottom").is_none_or(|b| b == "0") && rule.attribute("percent").is_none_or(|p| p == "0") => {
            Test::Top { count: rule.attribute("rank")?.parse().ok()? }
        }
        "aboveAverage" if rule.attribute("aboveAverage").is_none_or(|a| a == "1") && rule.attribute("equalAverage").is_none() => {
            Test::AboveAverage
        }
        _ => return None,
    })
}

// ---------- Writing ----------

fn rgb_u32(hex: &str) -> u32 {
    u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0)
}

fn xlsx_format(f: &Format) -> rust_xlsxwriter::Format {
    use rust_xlsxwriter::{Color, Format as X, FormatAlign, FormatPattern};
    let mut x = X::new().set_font_name("Noto Sans").set_font_size(11);
    if f.bold {
        x = x.set_bold();
    }
    if f.italic {
        x = x.set_italic();
    }
    if let Some(c) = &f.color {
        x = x.set_font_color(Color::RGB(rgb_u32(c)));
    }
    if let Some(c) = &f.fill {
        x = x.set_pattern(FormatPattern::Solid).set_background_color(Color::RGB(rgb_u32(c)));
    }
    {
        use rust_xlsxwriter::FormatBorder::Thin;
        if f.borders & TOP != 0 {
            x = x.set_border_top(Thin).set_border_top_color(Color::RGB(0x5a606b));
        }
        if f.borders & BOTTOM != 0 {
            x = x.set_border_bottom(Thin).set_border_bottom_color(Color::RGB(0x5a606b));
        }
        if f.borders & LEFT != 0 {
            x = x.set_border_left(Thin).set_border_left_color(Color::RGB(0x5a606b));
        }
        if f.borders & RIGHT != 0 {
            x = x.set_border_right(Thin).set_border_right_color(Color::RGB(0x5a606b));
        }
    }
    if f.wrap {
        x = x.set_text_wrap();
    }
    x = match f.align {
        Some(HAlign::Left) => x.set_align(FormatAlign::Left),
        Some(HAlign::Center) => x.set_align(FormatAlign::Center),
        Some(HAlign::Right) => x.set_align(FormatAlign::Right),
        None => x,
    };
    if let Some(p) = &f.pattern {
        return x.set_num_format(p);
    }
    match f.number {
        NumFmt::General => x,
        NumFmt::Decimal => x.set_num_format("#,##0.00"),
        NumFmt::Currency => x.set_num_format("$#,##0.00"),
        NumFmt::Percent => x.set_num_format("0.0%"),
        NumFmt::Date => x.set_num_format("yyyy-mm-dd"),
    }
}

/// The cell a position falls in, and how far into it: (index, offset).
fn cell_at(pos: f64, size: impl Fn(u32) -> u32, max: u32) -> (u32, u32) {
    let mut left = pos.max(0.0);
    for i in 0..max {
        let w = size(i) as f64;
        if left < w {
            return (i, left as u32);
        }
        left -= w;
    }
    (max - 1, 0)
}

/// A chart of `range` for Excel, its series named by a first row of words.
/// A chart part's title and axis titles.
fn chart_titles(xml: &str) -> ChartText {
    let Ok(doc) = crate::xml::parse(xml) else { return ChartText::default() };
    // A title's words: its runs, put together.
    let words = |t: roxmltree::Node| -> String {
        t.descendants().filter(|n| n.tag_name().name() == "t").filter_map(|n| n.text()).collect::<String>().trim().to_string()
    };
    let title_of = |n: roxmltree::Node| n.children().find(|c| c.tag_name().name() == "title").map(words).unwrap_or_default();
    let mut text = ChartText::default();
    if let Some(chart) = doc.descendants().find(|n| n.tag_name().name() == "chart") {
        text.title = title_of(chart);
    }
    for ax in doc.descendants().filter(|n| matches!(n.tag_name().name(), "catAx" | "valAx" | "dateAx")) {
        let pos = ax.children().find(|c| c.tag_name().name() == "axPos").and_then(|p| p.attribute("val")).unwrap_or("");
        let t = title_of(ax);
        if t.is_empty() {
            continue;
        }
        if matches!(pos, "b" | "t") {
            text.x_title = t;
        } else {
            text.y_title = t;
        }
    }
    text
}

fn excel_chart(wb: &Workbook, calc: &Calc, si: usize, range: &str, kind: ChartKind, text: &ChartText) -> Option<rust_xlsxwriter::Chart> {
    use rust_xlsxwriter::{Chart, ChartType};
    let ((r1, c1), (r2, c2)) = formula::parse_range(range)?;
    let s = &wb.sheets[si];
    let kind = match kind {
        ChartKind::Bar => ChartType::Column,
        ChartKind::Line => ChartType::Line,
        ChartKind::Area => ChartType::Area,
        ChartKind::Pie => ChartType::Pie,
        ChartKind::StackedBar => ChartType::ColumnStacked,
        ChartKind::StackedArea => ChartType::AreaStacked,
        ChartKind::Scatter => ChartType::Scatter,
        ChartKind::HBar => ChartType::Bar,
        ChartKind::Doughnut => ChartType::Doughnut,
    };
    let mut chart = Chart::new(kind);
    let label_col = (c2 > c1).then_some(c1);
    let first_value = if label_col.is_some() { c1 + 1 } else { c1 };
    let named = (first_value..=c2).all(|c| matches!(calc.value(wb, si, r1, c), formula::Value::Text(_))) && r2 > r1;
    let start = if named { r1 + 1 } else { r1 };
    for c in (first_value..=c2).take(CHART_SERIES) {
        let series = chart.add_series();
        series.set_values((s.name.as_str(), start, c as u16, r2.min(r1 + CHART_ROWS), c as u16));
        if let Some(lc) = label_col {
            series.set_categories((s.name.as_str(), start, lc as u16, r2.min(r1 + CHART_ROWS), lc as u16));
        }
        if named {
            series.set_name((s.name.as_str(), r1, c as u16));
        }
    }
    if !text.title.is_empty() {
        chart.title().set_name(&text.title);
    }
    if kind != ChartType::Pie {
        if !text.x_title.is_empty() {
            chart.x_axis().set_name(&text.x_title);
        }
        if !text.y_title.is_empty() {
            chart.y_axis().set_name(&text.y_title);
        }
    }
    Some(chart)
}

/// A table region as an Excel table, its columns named by its heading row.
fn add_table(ws: &mut rust_xlsxwriter::Worksheet, s: &Sheet, calc: &Calc, wb: &Workbook, si: usize, t: &TableRegion, a: Area) -> Result<()> {
    use rust_xlsxwriter::{Table, TableColumn, TableStyle};
    let mut seen: Vec<String> = Vec::new();
    let columns: Vec<TableColumn> = (a.left..=a.right)
        .map(|c| {
            // Excel wants each column's name once, and as it shows in the heading.
            let shown = display(&calc.value(wb, si, a.top, c), s.raw(a.top, c), &s.format(a.top, c)).0;
            let mut name = if t.header && !shown.trim().is_empty() { shown } else { format!("Column{}", c - a.left + 1) };
            let mut k = 2;
            while seen.iter().any(|x| x.eq_ignore_ascii_case(&name)) {
                name = format!("{name}{k}");
                k += 1;
            }
            seen.push(name.clone());
            TableColumn::new().set_header(name)
        })
        .collect();
    let style = match t.look {
        1 => TableStyle::Medium3,
        2 => TableStyle::Medium4,
        3 => TableStyle::Medium5,
        4 => TableStyle::Medium7,
        _ => TableStyle::Medium2,
    };
    let table = Table::new()
        .set_name(&t.name)
        .set_style(style)
        .set_header_row(t.header)
        .set_banded_rows(t.banded)
        .set_autofilter(false)
        .set_columns(&columns);
    ws.add_table(a.top, a.left as u16, a.bottom, a.right as u16, &table)?;
    Ok(())
}

/// A highlight rule as an Excel conditional format.
fn add_rule(ws: &mut rust_xlsxwriter::Worksheet, a: Area, rule: &Rule) -> Result<()> {
    use rust_xlsxwriter::{
        Color, ConditionalFormatAverage, ConditionalFormatBlank, ConditionalFormatCell, ConditionalFormatCellRule as C,
        ConditionalFormatError, ConditionalFormatText, ConditionalFormatTextRule, ConditionalFormatTop,
        ConditionalFormatTopRule, Format as X, FormatPattern,
    };
    let mut f = X::new();
    if let Some(c) = &rule.fill {
        f = f.set_pattern(FormatPattern::Solid).set_background_color(Color::RGB(rgb_u32(c)));
    }
    if let Some(c) = &rule.color {
        f = f.set_font_color(Color::RGB(rgb_u32(c)));
    }
    if rule.bold {
        f = f.set_bold();
    }
    let (r1, c1, r2, c2) = (a.top, a.left as u16, a.bottom, a.right as u16);
    let cell = |r: C<f64>| ConditionalFormatCell::new().set_rule(r).set_format(f.clone());
    match &rule.test {
        Test::Greater { value } => ws.add_conditional_format(r1, c1, r2, c2, &cell(C::GreaterThan(*value)))?,
        Test::Less { value } => ws.add_conditional_format(r1, c1, r2, c2, &cell(C::LessThan(*value)))?,
        Test::Equal { value } => ws.add_conditional_format(r1, c1, r2, c2, &cell(C::EqualTo(*value)))?,
        Test::Between { low, high } => ws.add_conditional_format(r1, c1, r2, c2, &cell(C::Between(*low, *high)))?,
        Test::Contains { text } => ws.add_conditional_format(
            r1,
            c1,
            r2,
            c2,
            &ConditionalFormatText::new().set_rule(ConditionalFormatTextRule::Contains(text.clone())).set_format(f),
        )?,
        Test::Blank => ws.add_conditional_format(r1, c1, r2, c2, &ConditionalFormatBlank::new().set_format(f))?,
        Test::Error => ws.add_conditional_format(r1, c1, r2, c2, &ConditionalFormatError::new().set_format(f))?,
        Test::Top { count } => ws.add_conditional_format(
            r1,
            c1,
            r2,
            c2,
            &ConditionalFormatTop::new().set_rule(ConditionalFormatTopRule::Top((*count).min(1000) as u16)).set_format(f),
        )?,
        Test::AboveAverage => ws.add_conditional_format(r1, c1, r2, c2, &ConditionalFormatAverage::new().set_format(f))?,
    };
    Ok(())
}

/// The workbook as an .xlsx file, and what Excel can't hold (shown once to the user).
pub fn write(wb: &Workbook, calc: &Calc) -> Result<(Vec<u8>, Vec<String>)> {
    use rust_xlsxwriter::Workbook as X;
    let mut book = X::new();
    let mut lost = Vec::new();
    // Names (a sheet's own ones as `Sheet!Name`).
    for n in &wb.names {
        let name = match &n.sheet {
            Some(sheet) => format!("{}!{}", super::summary::sheet_ref(sheet), n.name),
            None => n.name.clone(),
        };
        if book.define_name(name, &format!("={}", n.formula)).is_err() && !lost.iter().any(|l: &String| l.starts_with("names")) {
            lost.push("names Excel doesn't accept".into());
        }
    }
    for (si, s) in wb.sheets.iter().enumerate() {
        let ws = book.add_worksheet();
        ws.set_name(&s.name)?;
        if si == wb.active {
            ws.set_active(true);
        }
        for (c, w) in &s.col_widths {
            ws.set_column_width_pixels(*c as u16, *w)?;
        }
        for (r, h) in &s.row_heights {
            ws.set_row_height_pixels(*r, *h)?;
        }
        if let Some(a) = s.print_area {
            ws.set_print_area(a.top, a.left as u16, a.bottom, a.right as u16)?;
        }
        match s.print_fit {
            Fit::Actual => {}
            Fit::Width => {
                ws.set_print_fit_to_pages(1, 0);
            }
            Fit::Page => {
                ws.set_print_fit_to_pages(1, 1);
            }
        }
        if s.freeze_rows > 0 || s.freeze_cols > 0 {
            ws.set_freeze_panes(s.freeze_rows, s.freeze_cols as u16)?;
        }
        for l in &s.lists {
            let Some(a) = Area::parse(&l.range) else { continue };
            let dv = rust_xlsxwriter::DataValidation::new();
            let dv = match &l.source {
                Some(src) => Ok(dv.allow_list_formula(rust_xlsxwriter::Formula::new(format!("={src}")))),
                None => dv.allow_list_strings(&l.items),
            };
            match dv {
                Ok(dv) => {
                    ws.add_data_validation(a.top, a.left as u16, a.bottom, a.right as u16, &dv)?;
                }
                // Excel keeps at most 255 characters of typed choices.
                Err(_) => lost.push("dropdown lists with that many choices (put the choices in cells instead)".into()),
            }
        }
        // Tables first: they write their heading row, which the cells below then fill in.
        for t in &s.tables {
            let Some(a) = t.area() else { continue };
            add_table(ws, s, calc, wb, si, t, a)?;
            if t.total && !lost.iter().any(|l: &String| l.contains("total row")) {
                lost.push("tables' total-row look (the row stays, as an ordinary row)".into());
            }
        }
        // Merges first: merging writes the top-left cell, which the cells below then fill in.
        for m in &s.merges {
            let fmt = xlsx_format(&s.format(m.top, m.left));
            ws.merge_range(m.top, m.left as u16, m.bottom, m.right as u16, "", &fmt)?;
        }
        for ((r, c), cell) in &s.cells {
            if let Some(text) = &cell.comment {
                ws.insert_note(*r, *c as u16, &rust_xlsxwriter::Note::new(text).set_author("Sheets").add_author_prefix(false))?;
            }
        }
        for rule in &s.rules {
            if let Some(a) = Area::parse(&rule.range) {
                add_rule(ws, a, rule)?;
            }
        }
        for ((r, c), cell) in &s.cells {
            let (r, c16) = (*r, *c as u16);
            let fmt = xlsx_format(&cell.format);
            let raw = cell.raw.as_str();
            if let Some(src) = raw.strip_prefix('=') {
                // Keep the value we worked out, so viewers that don't recalculate show it.
                let shown = calc.value(wb, si, r, *c).text();
                // A block that spills is a dynamic array formula in Excel.
                if let Some(a) = calc.spill_area(si, r, *c) {
                    ws.write_dynamic_array_formula_with_format(a.top, a.left as u16, a.bottom, a.right as u16, rust_xlsxwriter::Formula::new(src).set_result(shown), &fmt)?;
                    continue;
                }
                if super::arrays::may_spill(&formula::parse(src)) {
                    ws.write_dynamic_formula_with_format(r, c16, rust_xlsxwriter::Formula::new(src).set_result(shown), &fmt)?;
                    continue;
                }
                ws.write_formula_with_format(r, c16, rust_xlsxwriter::Formula::new(src).set_result(shown), &fmt)?;
                continue;
            }
            match literal(raw) {
                formula::Value::Empty => ws.write_blank(r, c16, &fmt)?,
                formula::Value::Num(n) => {
                    // A typed date becomes a real date.
                    let fmt = if cell.format.number == NumFmt::General && formula::parse_date(raw).is_some() {
                        xlsx_format(&Format { number: NumFmt::Date, ..cell.format.clone() })
                    } else {
                        fmt
                    };
                    ws.write_number_with_format(r, c16, n, &fmt)?
                }
                formula::Value::Bool(b) => ws.write_boolean_with_format(r, c16, b, &fmt)?,
                _ => ws.write_string_with_format(r, c16, raw, &fmt)?,
            };
        }
        if s.cells.values().any(|c| c.format.checkbox) && !lost.iter().any(|l: &String| l.contains("checkbox")) {
            lost.push("checkboxes (they show as TRUE and FALSE)".into());
        }
        if s.filter.is_some() && !lost.iter().any(|l: &String| l.contains("filter")) {
            lost.push("the filter (every row is shown)".into());
        }
        if let Some(ch) = &s.chart
            && let Some(chart) = excel_chart(wb, calc, si, &ch.range, ch.kind, &ch.text)
        {
            let c2 = formula::parse_range(&ch.range).map(|r| r.1.1).unwrap_or(0);
            // Beside the data.
            let right = s.used().map(|(_, c)| c + 2).unwrap_or(c2 + 2);
            ws.insert_chart(1, right.min(COLS) as u16, &chart)?;
        }
        // Charts placed on the sheet: in the cell under their corner, offset within it.
        // Excel's default sizes differ from ours, so the cells they span get theirs.
        if let Some((right, bottom)) = s.charts.iter().fold(None, |acc: Option<(f64, f64)>, c| {
            let (r, b) = (c.x + c.w, c.y + c.h);
            Some(acc.map_or((r, b), |(ar, ab)| (ar.max(r), ab.max(b))))
        }) {
            let (last_col, _) = cell_at(right, |c| s.col_w(c), COLS);
            let (last_row, _) = cell_at(bottom, |r| s.row_h(r), ROWS);
            for c in 0..=last_col {
                ws.set_column_width_pixels(c as u16, s.size_of_col(c))?;
            }
            for r in 0..=last_row {
                ws.set_row_height_pixels(r, s.size_of_row(r))?;
            }
        }
        for c in &s.hidden_cols {
            ws.set_column_hidden(*c as u16)?;
        }
        for r in &s.hidden_rows {
            ws.set_row_hidden(*r)?;
        }
        for pc in &s.charts {
            let Some(mut chart) = excel_chart(wb, calc, si, &pc.range, pc.kind, &pc.text) else { continue };
            chart.set_width(pc.w.round().max(1.0) as u32).set_height(pc.h.round().max(1.0) as u32);
            let (col, xo) = cell_at(pc.x, |c| s.col_w(c), COLS);
            let (row, yo) = cell_at(pc.y, |r| s.row_h(r), ROWS);
            ws.insert_chart_with_offset(row, col as u16, &chart, xo, yo)?;
        }
    }
    Ok((book.save_to_buffer()?, lost))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_excel() {
        let mut wb = Workbook::default();
        wb.set(0, 0, "Item");
        wb.set(0, 1, "Cost");
        wb.set(1, 0, "Rent");
        wb.set(1, 1, "1200");
        wb.set(2, 0, "Food");
        wb.set(2, 1, "300.5");
        wb.set(3, 1, "=SUM(B2:B3)");
        wb.set(4, 0, "2026-10-05");
        wb.set(5, 0, "TRUE");
        wb.set(6, 0, "=TEXTJOIN(\",\", TRUE, A2:A3)");
        wb.format(Area::new(0, 0, 0, 1), |f| {
            f.bold = true;
            f.fill = Some("#eef1f5".into());
        });
        wb.format(Area::new(1, 1, 3, 1), |f| f.number = NumFmt::Currency);
        wb.format(Area::new(1, 0, 1, 0), |f| {
            f.italic = true;
            f.color = Some("#c62828".into());
            f.align = Some(HAlign::Center);
        });
        wb.set_col_w(0, 180);
        wb.set_row_h(0, 40);
        wb.sheet_mut().freeze_rows = 1;
        wb.sheet_mut().freeze_cols = 2;
        wb.sheet_mut().chart = Some(Chart { range: "A1:B3".into(), kind: ChartKind::Bar, text: ChartText::default() });
        wb.set(8, 0, "Merged");
        wb.merge(Area::new(8, 0, 9, 2));
        wb.set_comment(1, 1, "Due on the 1st");
        wb.set_borders(Area::new(2, 0, 3, 1), Borders::Outside);
        wb.sheet_mut().tables.push(TableRegion {
            name: "Costs".into(),
            range: "A1:B3".into(),
            look: 4,
            header: true,
            banded: true,
            total: false,
        });
        wb.sheet_mut().charts.push(PlacedChart {
            range: "A1:B3".into(),
            kind: ChartKind::StackedBar,
            text: ChartText { title: "Costs".into(), x_title: "Item".into(), y_title: "Dollars".into() },
            x: 500.0, y: 40.0, w: 400.0, h: 240.0 });
        wb.format(Area::cell(4, 0), |f| f.wrap = true);
        wb.sheet_mut().rules = vec![
            Rule {
                range: "B2:B3".into(),
                test: crate::sheets::rules::Test::Greater { value: 500.0 },
                fill: Some("#fde2e1".into()),
                color: Some("#9c1c1c".into()),
                bold: true,
            },
            Rule {
                range: "A2:A9".into(),
                test: crate::sheets::rules::Test::Contains { text: "oo".into() },
                fill: Some("#dcf3e0".into()),
                color: None,
                bold: false,
            },
            Rule { range: "B2:B3".into(), test: crate::sheets::rules::Test::Top { count: 1 }, fill: None, color: Some("#1d4a9c".into()), bold: false },
        ];
        wb.add_sheet();
        wb.rename_sheet(1, "Notes").unwrap();
        wb.set(0, 0, "='Sheet 1'!B4");
        wb.active = 0;
        wb.hide_rows(6, 7, true);
        let list = |items: &[&str], source: Option<&str>| super::super::lists::ListRule {
            range: String::new(),
            items: items.iter().map(|x| x.to_string()).collect(),
            source: source.map(String::from),
        };
        wb.set_list(Area::new(1, 2, 5, 2), Some(list(&["Paid", "Not \"yet\""], None)));
        wb.set_list(Area::cell(6, 2), Some(list(&[], Some("A2:A3"))));
        wb.format(Area::cell(2, 1), |f| f.pattern = Some("#,##0.0 \"kg\"".into()));
        wb.hide_cols(3, 3, true);

        let calc = Calc::new(&wb, 0.0);
        let (bytes, lost) = write(&wb, &calc).unwrap();
        assert!(lost.is_empty());
        let back = read(&bytes).unwrap().workbook;
        let s = back.sheet();
        assert_eq!(back.sheets.len(), 2);
        assert_eq!(back.sheets[1].name, "Notes");
        assert_eq!(back.sheets[1].raw(0, 0), "='Sheet 1'!B4");
        assert_eq!(s.raw(0, 0), "Item");
        assert_eq!(s.raw(1, 1), "1200");
        assert_eq!(s.raw(3, 1), "=SUM(B2:B3)");
        assert_eq!(s.raw(5, 0), "TRUE");
        // Written as `_xlfn.TEXTJOIN`, read back plain.
        assert_eq!(s.raw(6, 0), "=TEXTJOIN(\",\", TRUE, A2:A3)");
        assert_eq!(s.format(4, 0).number, NumFmt::Date);
        assert!(s.format(0, 0).bold);
        assert_eq!(s.format(0, 0).fill.as_deref(), Some("#eef1f5"));
        assert_eq!(s.format(1, 1).number, NumFmt::Currency);
        let f = s.format(1, 0);
        assert!(f.italic);
        assert_eq!(f.color.as_deref(), Some("#c62828"));
        assert_eq!(f.align, Some(HAlign::Center));
        assert_eq!(s.col_w(0), 180);
        assert_eq!(s.row_h(0), 40);
        assert_eq!(s.hidden_rows.iter().copied().collect::<Vec<_>>(), [6, 7]);
        assert_eq!(s.hidden_cols.iter().copied().collect::<Vec<_>>(), [3]);
        assert_eq!(s.format(2, 1).pattern.as_deref(), Some("#,##0.0 \"kg\""));
        assert_eq!(s.lists.len(), 2);
        assert_eq!(back.list_at(0, 3, 2).map(|l| l.items.clone()), Some(vec!["Paid".to_string(), "Not \"yet\"".to_string()]));
        assert_eq!(back.list_at(0, 6, 2).and_then(|l| l.source.clone()).as_deref(), Some("A2:A3"));
        assert_eq!((s.row_h(6), s.size_of_row(6)), (0, ROW_H));
        assert_eq!((s.freeze_rows, s.freeze_cols), (1, 2));
        assert_eq!(s.merges, [Area::new(8, 0, 9, 2)]);
        assert_eq!(s.raw(8, 0), "Merged");
        assert_eq!(s.comment(1, 1), Some("Due on the 1st"));
        assert_eq!(s.format(2, 0).borders, TOP | LEFT);
        assert_eq!(s.format(3, 1).borders, BOTTOM | RIGHT);
        assert!(s.format(4, 0).wrap);
        assert_eq!(s.tables, wb.sheets[0].tables);
        let pc = s.charts.iter().find(|c| c.kind == ChartKind::StackedBar).expect("the placed chart");
        assert_eq!(pc.range, "A1:B3");
        assert_eq!(pc.text, ChartText { title: "Costs".into(), x_title: "Item".into(), y_title: "Dollars".into() });
        assert!((pc.x - 500.0).abs() < 2.0 && (pc.y - 40.0).abs() < 2.0, "{pc:?}");
        assert!((pc.w - 400.0).abs() < 2.0 && (pc.h - 240.0).abs() < 2.0, "{pc:?}");
        // Excel groups rules by range; within a range the order holds, and only
        // rules on the same cells compete.
        let by_range = |rules: &[Rule]| {
            let mut v = rules.to_vec();
            v.sort_by(|a, b| a.range.cmp(&b.range));
            v
        };
        assert_eq!(by_range(&s.rules), by_range(&wb.sheets[0].rules));
    }

    #[test]
    fn expands_shared_formulas() {
        // What Excel writes for a formula filled down: the text once, then references to it.
        let sheet = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>
            <row r="1"><c r="A1"><v>1</v></c><c r="B1"><f t="shared" ref="B1:B3" si="0">A1*2</f><v>2</v></c></row>
            <row r="2"><c r="A2"><v>2</v></c><c r="B2"><f t="shared" si="0"/><v>4</v></c></row>
            <row r="3"><c r="A3" t="s"><v>0</v></c><c r="B3"><f t="shared" si="0"/></c></row>
        </sheetData></worksheet>"#;
        let mut buf = Cursor::new(Vec::new());
        {
            use std::io::Write;
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            let files = [
                (
                    "_rels/.rels",
                    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
                ),
                (
                    "xl/workbook.xml",
                    r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
                ),
                (
                    "xl/_rels/workbook.xml.rels",
                    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="x" Target="worksheets/sheet1.xml"/></Relationships>"#,
                ),
                (
                    "xl/sharedStrings.xml",
                    r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><r><t>hel</t></r><r><t>lo</t></r></si></sst>"#,
                ),
                ("xl/worksheets/sheet1.xml", sheet),
            ];
            for (n, t) in files {
                z.start_file(n, o).unwrap();
                z.write_all(t.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        let bytes = buf.into_inner();
        let wb = read(&bytes).unwrap().workbook;
        let s = wb.sheet();
        assert_eq!(s.name, "Data");
        assert_eq!(s.raw(1, 1), "=A2*2");
        assert_eq!(s.raw(2, 1), "=A3*2");
        assert_eq!(s.raw(2, 0), "hello");
    }

    #[test]
    fn number_formats_map() {
        let ours = |n| (n, None);
        let code = |c: &str| (NumFmt::General, Some(c.to_string()));
        assert_eq!(num_fmt(164, Some("\"$\"#,##0.00")), ours(NumFmt::Currency));
        assert_eq!(num_fmt(166, Some("0.0%")), ours(NumFmt::Percent));
        assert_eq!(num_fmt(14, None), ours(NumFmt::Date));
        assert_eq!(num_fmt(3, None), code("#,##0"));
        assert_eq!(num_fmt(165, Some("dd/mm/yyyy")), code("dd/mm/yyyy"));
        assert_eq!(num_fmt(167, Some("[Red]0.00")), code("[Red]0.00"));
        // A code that can't be shown: the nearest of ours.
        assert_eq!(num_fmt(168, Some("0.00E+00")), ours(NumFmt::Decimal));
    }

    #[test]
    fn spilled_blocks_round_trip() {
        let mut wb = Workbook::default();
        for (r, v) in ["b", "a", "b"].iter().enumerate() {
            wb.set(r as u32, 0, v);
        }
        wb.set(0, 2, "=SORT(UNIQUE(A1:A3))");
        let calc = Calc::new(&wb, 0.0);
        let (bytes, _) = write(&wb, &calc).unwrap();
        // Excel's form: a dynamic array over the block, its values kept beside it.
        let mut zip = zip::ZipArchive::new(Cursor::new(&bytes[..])).unwrap();
        let sheet = part(&mut zip, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"t="array" ref="C1:C2""#), "{sheet}");
        let back = read(&bytes).unwrap().workbook;
        assert_eq!(back.sheets[0].raw(0, 2), "=SORT(UNIQUE(A1:A3))");
        assert_eq!(back.sheets[0].raw(1, 2), "", "the spilled value isn't a typed one");
        let calc = Calc::new(&back, 0.0);
        assert_eq!(calc.value(&back, 0, 1, 2), formula::Value::Text("b".into()));
    }

    #[test]
    fn print_settings_round_trip() {
        let mut wb = Workbook::default();
        wb.set(0, 0, "x");
        wb.sheets.push(Sheet::new("Two"));
        wb.sheets[1].cells.insert((0, 0), Cell { raw: "y".into(), format: Format::default(), comment: None });
        wb.sheets[0].print_area = Area::parse("A1:C9");
        wb.sheets[0].print_fit = Fit::Width;
        wb.sheets[1].print_fit = Fit::Page;
        let (bytes, _) = write(&wb, &Calc::new(&wb, 0.0)).unwrap();
        let back = read(&bytes).unwrap().workbook;
        assert_eq!((back.sheets[0].print_area, back.sheets[0].print_fit), (Area::parse("A1:C9"), Fit::Width));
        assert_eq!((back.sheets[1].print_area, back.sheets[1].print_fit), (None, Fit::Page));
    }

    /// A workbook with a data sheet and a pivot table of it, as Excel writes them.
    pub(crate) fn pivot_workbook() -> Vec<u8> {
        use std::io::Write as _;
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            let ns = r#"xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
            let rel = |items: &[(&str, &str)]| {
                let mut x = String::from(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
                for (i, (kind, target)) in items.iter().enumerate() {
                    x.push_str(&format!(r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}" Target="{target}"/>"#, i + 1));
                }
                x + "</Relationships>"
            };
            let row = |r: u32, cells: &[(&str, &str)]| {
                let mut x = format!(r#"<row r="{r}">"#);
                for (i, (v, t)) in cells.iter().enumerate() {
                    let at = format!("{}{r}", formula::col_name(i as u32));
                    if *t == "s" {
                        x.push_str(&format!(r#"<c r="{at}" t="inlineStr"><is><t>{v}</t></is></c>"#));
                    } else {
                        x.push_str(&format!(r#"<c r="{at}"><v>{v}</v></c>"#));
                    }
                }
                x + "</row>"
            };
            let data = [("Region", "Item", "Amount"), ("North", "Pens", "5"), ("South", "Pens", "7"), ("North", "Ink", "3"), ("North", "Pens", "2")];
            let mut rows = String::new();
            for (i, (a, b, c)) in data.iter().enumerate() {
                let t = if i == 0 { "s" } else { "n" };
                rows.push_str(&row(i as u32 + 1, &[(a, "s"), (b, "s"), (c, t)]));
            }
            // The pivot's frozen values, as Excel leaves them.
            let pivot_rows = row(3, &[("Sum of Amount", "s")]) + &row(4, &[("North", "s"), ("10", "n")]) + &row(5, &[("South", "s"), ("7", "n")]);
            let types = concat!(
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#,
                r#"<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>"#,
                r#"<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#,
                r#"<Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#,
                r#"<Override PartName="/xl/pivotTables/pivotTable1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml"/>"#,
                r#"<Override PartName="/xl/pivotCache/pivotCacheDefinition1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml"/></Types>"#
            );
            let files = [
                ("[Content_Types].xml".to_string(), types.to_string()),
                ("_rels/.rels".to_string(), rel(&[("officeDocument", "xl/workbook.xml")])),
                ("xl/workbook.xml".into(), format!(r#"<workbook {ns}><sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Pivot" sheetId="2" r:id="rId2"/></sheets><pivotCaches><pivotCache cacheId="1" r:id="rId3"/></pivotCaches></workbook>"#)),
                (
                    "xl/_rels/workbook.xml.rels".into(),
                    rel(&[("worksheet", "worksheets/sheet1.xml"), ("worksheet", "worksheets/sheet2.xml"), ("pivotCacheDefinition", "pivotCache/pivotCacheDefinition1.xml")]),
                ),
                ("xl/worksheets/sheet1.xml".into(), format!(r#"<worksheet {ns}><sheetData>{rows}</sheetData></worksheet>"#)),
                ("xl/worksheets/sheet2.xml".into(), format!(r#"<worksheet {ns}><sheetData>{pivot_rows}</sheetData></worksheet>"#)),
                ("xl/worksheets/_rels/sheet2.xml.rels".into(), rel(&[("pivotTable", "../pivotTables/pivotTable1.xml")])),
                (
                    "xl/pivotTables/pivotTable1.xml".into(),
                    format!(r#"<pivotTableDefinition {ns} name="PivotTable1" cacheId="1" dataCaption="Values"><location ref="A3:B6" firstHeaderRow="1" firstDataRow="1" firstDataCol="1"/><pivotFields count="3"><pivotField axis="axisRow"/><pivotField/><pivotField dataField="1"/></pivotFields><rowFields count="1"><field x="0"/></rowFields><dataFields count="1"><dataField name="Sum of Amount" fld="2" baseField="0" baseItem="0"/></dataFields></pivotTableDefinition>"#),
                ),
                ("xl/pivotTables/_rels/pivotTable1.xml.rels".into(), rel(&[("pivotCacheDefinition", "../pivotCache/pivotCacheDefinition1.xml")])),
                (
                    "xl/pivotCache/pivotCacheDefinition1.xml".into(),
                    format!(r#"<pivotCacheDefinition {ns}><cacheSource type="worksheet"><worksheetSource ref="A1:C5" sheet="Data"/></cacheSource><cacheFields count="3"><cacheField name="Region"/><cacheField name="Item"/><cacheField name="Amount"/></cacheFields></pivotCacheDefinition>"#),
                ),
            ];
            for (n, t) in files {
                z.start_file(n, o).unwrap();
                z.write_all(t.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn every_chart_kind_round_trips() {
        for kind in ChartKind::ALL {
            let mut wb = Workbook::default();
            for (r, (l, v)) in [("Q", "Sales"), ("Q1", "3"), ("Q2", "5")].iter().enumerate() {
                wb.set(r as u32, 0, l);
                wb.set(r as u32, 1, v);
            }
            wb.sheet_mut().chart = Some(Chart { range: "A1:B3".into(), kind, text: ChartText::default() });
            let (bytes, _) = write(&wb, &Calc::new(&wb, 0.0)).unwrap();
            let back = read(&bytes).unwrap().workbook;
            assert_eq!(back.sheets[0].charts.first().map(|c| c.kind), Some(kind), "{kind:?}");
        }
    }

    #[test]
    fn pivot_tables_become_live_summaries() {
        let opened = read(&pivot_workbook()).unwrap();
        assert_eq!(opened.pivots, 1);
        let mut wb = opened.workbook;
        let p = &wb.sheets[1];
        assert_eq!((p.raw(2, 0), p.raw(3, 0), p.raw(4, 0)), ("Region", "North", "South"));
        assert_eq!(p.raw(3, 1), "=SUMIFS(Data!$C$2:$C$5, Data!$A$2:$A$5, $A4)");
        let calc = Calc::new(&wb, 0.0);
        assert_eq!(calc.value(&wb, 1, 3, 1), formula::Value::Num(10.0));
        // It follows its data.
        wb.sheets[0].cells.insert((1, 2), Cell { raw: "50".into(), ..Default::default() });
        let calc = Calc::new(&wb, 0.0);
        assert_eq!(calc.value(&wb, 1, 3, 1), formula::Value::Num(55.0));
    }

    #[test]
    fn names_round_trip() {
        let mut wb = Workbook::default();
        wb.sheets.push(Sheet::new("Data 2"));
        wb.set(0, 0, "4");
        wb.set(1, 0, "=Total*2");
        wb.names.push(DefinedName { name: "Total".into(), sheet: None, formula: "'Sheet 1'!$A$1".into() });
        wb.names.push(DefinedName { name: "Rate".into(), sheet: None, formula: "0.25".into() });
        let (bytes, _) = write(&wb, &Calc::new(&wb, 0.0)).unwrap();
        let back = read(&bytes).unwrap().workbook;
        let find = |n: &str| back.names.iter().find(|x| x.name == n).map(|x| x.formula.clone());
        assert_eq!(find("Total").as_deref(), Some("'Sheet 1'!$A$1"));
        assert_eq!(find("Rate").as_deref(), Some("0.25"));
        let calc = Calc::new(&back, 0.0);
        assert_eq!(calc.value(&back, 0, 1, 0), formula::Value::Num(8.0));
    }
}

#[cfg(test)]
#[test]
#[ignore]
fn write_pivot_probe() {
    if let Some(p) = std::env::var_os("NXO_PROBE") {
        std::fs::write(p, tests::pivot_workbook()).unwrap();
    }
}
