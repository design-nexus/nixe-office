//! Word documents: paragraphs, headings, lists, character formatting, pictures,
//! tables and the page setup, both ways.

use super::model::*;
use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io::{Cursor, Write};

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const A_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const WP_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const PIC_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const PKG: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
/// Twentieths of a point.
const TWIP: f64 = 20.0;
const EMU: f64 = 12700.0;
const WPS_NS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
const CHART_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\t' => {}
            c => o.push(c),
        }
    }
    o
}

// ---------- Writing ----------

fn style_id(s: Style) -> Option<&'static str> {
    match s {
        Style::Normal => None,
        Style::Title => Some("Title"),
        Style::H1 => Some("Heading1"),
        Style::H2 => Some("Heading2"),
        Style::H3 => Some("Heading3"),
        Style::Quote => Some("Quote"),
    }
}

fn run_props(r: &TextRun) -> String {
    let mut p = String::new();
    if r.link.is_some() {
        p.push_str(r#"<w:rStyle w:val="Hyperlink"/>"#);
    }
    if let Some(f) = &r.font {
        let _ = write!(p, r#"<w:rFonts w:ascii="{0}" w:hAnsi="{0}" w:cs="{0}"/>"#, esc(f));
    }
    if r.bold {
        p.push_str("<w:b/>");
    }
    if r.italic {
        p.push_str("<w:i/>");
    }
    if r.strike {
        p.push_str("<w:strike/>");
    }
    if let Some(c) = &r.color {
        let _ = write!(p, r#"<w:color w:val="{}"/>"#, c.trim_start_matches('#').to_uppercase());
    }
    if let Some(s) = r.size {
        let _ = write!(p, r#"<w:sz w:val="{0}"/><w:szCs w:val="{0}"/>"#, (s * 2.0).round() as i64);
    }
    if r.underline {
        p.push_str(r#"<w:u w:val="single"/>"#);
    }
    if let Some(c) = &r.highlight {
        // Any colour, as shading (Word's own highlight has only a few names).
        let _ = write!(p, r#"<w:shd w:val="clear" w:color="auto" w:fill="{}"/>"#, c.trim_start_matches('#').to_uppercase());
    }
    if r.superscript {
        p.push_str(r#"<w:vertAlign w:val="superscript"/>"#);
    } else if r.subscript {
        p.push_str(r#"<w:vertAlign w:val="subscript"/>"#);
    }
    if p.is_empty() { p } else { format!("<w:rPr>{p}</w:rPr>") }
}

fn text_run(r: &TextRun) -> String {
    let mut out = String::new();
    let props = run_props(r);
    // Tabs and line breaks are their own elements; everything else is plain text.
    let mut text = String::new();
    let flush = |out: &mut String, text: &mut String| {
        if !text.is_empty() {
            let _ = write!(out, r#"<w:r>{props}<w:t xml:space="preserve">{}</w:t></w:r>"#, esc(text));
            text.clear();
        }
    };
    for c in r.text.chars() {
        match c {
            '\t' | LINE_BREAK => {
                flush(&mut out, &mut text);
                let _ = write!(out, "<w:r>{props}{}</w:r>", if c == '\t' { "<w:tab/>" } else { "<w:br/>" });
            }
            c => text.push(c),
        }
    }
    flush(&mut out, &mut text);
    out
}

fn picture_run(rid: &str, id: usize, name: &str, (w_pt, h_pt): (f64, f64), wrap: Option<Side>) -> String {
    let (cx, cy) = ((w_pt * EMU).round() as i64, (h_pt * EMU).round() as i64);
    // A wrapped picture floats at the side of its paragraph, text around it.
    let (open, close) = match wrap {
        None => (r#"<wp:inline distT="0" distB="0" distL="0" distR="0">"#.to_string(), "</wp:inline>"),
        Some(side) => (
            format!(
                r#"<wp:anchor distT="0" distB="114300" distL="114300" distR="114300" simplePos="0" relativeHeight="{}" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:align>{}</wp:align></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>"#,
                251_658_240 + id,
                if side == Side::Right { "right" } else { "left" }
            ),
            "</wp:anchor>",
        ),
    };
    let after_extent = if wrap.is_some() { r#"<wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapSquare wrapText="bothSides"/>"# } else { "" };
    format!(
        r#"<w:r><w:drawing>{open}<wp:extent cx="{cx}" cy="{cy}"/>{after_extent}<wp:docPr id="{id}" name="{name}"/><wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect="1"/></wp:cNvGraphicFramePr><a:graphic><a:graphicData uri="{PIC_NS}"><pic:pic><pic:nvPicPr><pic:cNvPr id="{id}" name="{name}"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic>{close}</w:drawing></w:r>"#,
        name = esc(name)
    )
}

fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || !bytes.starts_with(b"\x89PNG") {
        return None;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((w, h))
}

fn header_footer(tag: &str, text: &str, page_numbers: bool) -> String {
    let mut runs = esc(text);
    runs = if runs.is_empty() { String::new() } else { format!(r#"<w:r><w:t xml:space="preserve">{runs}</w:t></w:r>"#) };
    if page_numbers {
        if !runs.is_empty() {
            runs.push_str("<w:r><w:tab/></w:r>");
        }
        runs.push_str(r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#);
    }
    let jc = if page_numbers && text.is_empty() { r#"<w:pPr><w:jc w:val="center"/></w:pPr>"# } else { "" };
    format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:{tag} xmlns:w="{W_NS}"><w:p>{jc}{runs}</w:p></w:{tag}>"#)
}

/// The first text run's comment in a block, for keeping a comment open across paragraphs.
fn first_comment(b: Option<&Block>) -> Option<u32> {
    let Some(Block::Paragraph(p)) = b else { return None };
    p.runs.iter().find_map(|r| if let Run::Text(t) = r { Some(t.comment) } else { None }).flatten()
}

/// A run, inside `w:ins` or `w:del` when it's a tracked change.
fn tracked_run(t: &TextRun, revisions: &[Revision]) -> String {
    let (tag, id) = match (t.inserted, t.deleted) {
        (Some(id), _) => ("ins", id),
        (_, Some(id)) => ("del", id),
        _ => return text_run(t),
    };
    let rev = revisions.iter().find(|r| r.id == id);
    let author = esc(rev.map(|r| r.author.as_str()).filter(|a| !a.is_empty()).unwrap_or("Nexus Write"));
    let date = rev.map(|r| r.date.as_str()).filter(|d| !d.is_empty()).map(|d| format!(r#" w:date="{}""#, esc(d))).unwrap_or_default();
    let mut runs = text_run(t);
    if tag == "del" {
        runs = runs.replace("<w:t xml:space=\"preserve\">", "<w:delText xml:space=\"preserve\">").replace("</w:t>", "</w:delText>");
    }
    // Word wants revision ids distinct from comments': keep them apart.
    format!(r#"<w:{tag} w:id="{}" w:author="{author}"{date}>{runs}</w:{tag}>"#, id + 10_000)
}

fn paragraph_xml(text: &str, ppr: &str) -> String {
    let runs = if text.is_empty() { String::new() } else { text_run(&TextRun { text: text.into(), ..Default::default() }) };
    format!("<w:p>{ppr}{runs}</w:p>")
}

/// The document as a .docx file, and what Word can't keep.
pub fn write(doc: &Document, media: &BTreeMap<String, Vec<u8>>) -> Result<(Vec<u8>, Vec<String>)> {
    let mut body = String::new();
    let mut lost = Vec::new();
    // Footnotes, numbered from 1 in Word's file.
    let mut footnotes: Vec<String> = Vec::new();
    let mut endnotes: Vec<String> = Vec::new();
    // Charts in the text, each a part of its own.
    let mut charts: Vec<String> = Vec::new();
    // Word keeps a section's settings at its end: each break writes the
    // section before it (filled in below), the body's last sectPr the last one.
    let mut breaks: Vec<(usize, &str)> = Vec::new();
    let (mut sec_cols, mut sec_type) = (doc.page.columns(), "nextPage");
    // The comment whose range is open: Word marks where each starts and ends.
    let mut open: Option<u32> = None;
    let close = |open: &mut Option<u32>, out: &mut String| {
        if let Some(id) = open.take() {
            let _ = write!(out, r#"<w:commentRangeEnd w:id="{id}"/><w:r><w:commentReference w:id="{id}"/></w:r>"#);
        }
    };
    // Page numbers for a table of contents.
    let heading_pages = if doc.blocks.iter().any(|b| matches!(b, Block::Contents)) {
        super::layout::paginate(doc, media).heading_pages().clone()
    } else {
        HashMap::new()
    };
    let mut rels: Vec<(String, String, String)> =
        vec![("rId1".into(), "styles".into(), "styles.xml".into()), ("rId2".into(), "numbering".into(), "numbering.xml".into())];
    let mut images: Vec<(String, Vec<u8>)> = Vec::new();
    // Each numbered list gets its own numbering instance, so it starts at 1.
    let mut nums: Vec<(u32, bool)> = Vec::new();
    let mut list_num: Option<u32> = None;
    let mut pic_id = 0;
    for (bi, b) in doc.blocks.iter().enumerate() {
        match b {
            Block::Paragraph(p) => {
                let mut ppr = String::new();
                if let Some(s) = style_id(p.style) {
                    let _ = write!(ppr, r#"<w:pStyle w:val="{s}"/>"#);
                }
                // w:spacing goes after w:numPr and before w:ind.
                let mut spacing = String::new();
                if let Some(b) = p.space_before {
                    let _ = write!(spacing, r#" w:before="{}""#, (b * TWIP).round() as i64);
                }
                if let Some(a) = p.space_after {
                    let _ = write!(spacing, r#" w:after="{}""#, (a * TWIP).round() as i64);
                }
                if let Some(l) = p.line_spacing {
                    let _ = write!(spacing, r#" w:line="{}" w:lineRule="auto""#, (l * 240.0).round() as i64);
                }
                let spacing = if spacing.is_empty() { spacing } else { format!("<w:spacing{spacing}/>") };
                match p.list {
                    Some(l) => {
                        let id = match (l, list_num) {
                            (List::Number, Some(id)) => id,
                            (List::Bullet, _) => {
                                // Bullets share one instance.
                                if !nums.iter().any(|n| n.0 == 1 && !n.1) {
                                    nums.insert(0, (1, false));
                                }
                                1
                            }
                            (List::Number, _) => {
                                let id = nums.len() as u32 + 2;
                                nums.push((id, true));
                                id
                            }
                        };
                        if l == List::Number {
                            list_num = Some(id);
                        }
                        let _ = write!(
                            ppr,
                            r#"<w:numPr><w:ilvl w:val="{}"/><w:numId w:val="{id}"/></w:numPr>"#,
                            p.indent.min(MAX_INDENT)
                        );
                        ppr.push_str(&spacing);
                    }
                    None => {
                        ppr.push_str(&spacing);
                        if p.indent > 0 {
                            let _ = write!(ppr, r#"<w:ind w:left="{}"/>"#, (p.indent as f64 * INDENT_STEP * TWIP) as i64);
                        }
                    }
                }
                if p.list != Some(List::Number) && p.list.is_none() {
                    list_num = None;
                }
                let jc = match p.align {
                    Align::Left => None,
                    Align::Center => Some("center"),
                    Align::Right => Some("right"),
                    Align::Justify => Some("both"),
                };
                if let Some(j) = jc {
                    let _ = write!(ppr, r#"<w:jc w:val="{j}"/>"#);
                }
                body.push_str("<w:p>");
                if !ppr.is_empty() {
                    let _ = write!(body, "<w:pPr>{ppr}</w:pPr>");
                }
                // The link open in this paragraph, if any.
                let mut link: Option<&str> = None;
                for r in &p.runs {
                    let wanted = if let Run::Text(t) = r { t.link.as_deref() } else { None };
                    if wanted != link {
                        if link.take().is_some() {
                            body.push_str("</w:hyperlink>");
                        }
                        if let Some(uri) = wanted {
                            let rid = match rels.iter().find(|(_, k, t)| k == "hyperlink" && t == uri) {
                                Some((id, ..)) => id.clone(),
                                None => {
                                    let id = format!("rId{}", rels.len() + 1);
                                    rels.push((id.clone(), "hyperlink".into(), uri.to_string()));
                                    id
                                }
                            };
                            let _ = write!(body, r#"<w:hyperlink r:id="{rid}" w:history="1">"#);
                            link = Some(uri);
                        }
                    }
                    match r {
                        Run::Text(t) => {
                            if t.comment != open {
                                close(&mut open, &mut body);
                                if let Some(id) = t.comment {
                                    let _ = write!(body, r#"<w:commentRangeStart w:id="{id}"/>"#);
                                    open = Some(id);
                                }
                            }
                            body.push_str(&tracked_run(t, &doc.revisions));
                        }
                        Run::Note(n) if n.end => {
                            endnotes.push(n.note.clone());
                            let _ = write!(
                                body,
                                r#"<w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:endnoteReference w:id="{}"/></w:r>"#,
                                endnotes.len()
                            );
                        }
                        Run::Note(n) => {
                            footnotes.push(n.note.clone());
                            let _ = write!(
                                body,
                                r#"<w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteReference w:id="{}"/></w:r>"#,
                                footnotes.len()
                            );
                        }
                        Run::Image(img) => {
                            let Some(bytes) = media.get(&img.image) else { continue };
                            let (pw, ph) = png_size(bytes).unwrap_or((4, 3));
                            let h = img.width * ph as f64 / pw.max(1) as f64;
                            pic_id += 1;
                            let rid = format!("rId{}", rels.len() + 1);
                            let file = format!("image{pic_id}.png");
                            rels.push((rid.clone(), "image".into(), format!("media/{file}")));
                            images.push((file, bytes.clone()));
                            body.push_str(&picture_run(&rid, pic_id, &img.image, (img.width, h), img.wrap));
                        }
                        Run::Drawing(d) => {
                            pic_id += 1;
                            let (cx, cy) = ((d.width * EMU).round() as i64, (d.height * EMU).round() as i64);
                            let data = match &d.drawing {
                                Figure::Shape { shape, fill } => format!(
                                    r#"<a:graphicData uri="{WPS_NS}"><wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="{}"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="{}"/></a:solidFill><a:ln><a:noFill/></a:ln></wps:spPr><wps:bodyPr/></wps:wsp></a:graphicData>"#,
                                    shape.preset(),
                                    fill.trim_start_matches('#').to_uppercase()
                                ),
                                Figure::Chart { chart, names, labels, series } => {
                                    charts.push(crate::present::pptx_more::chart_part(*chart, names, labels, series));
                                    let rid = format!("rId{}", rels.len() + 1);
                                    rels.push((rid.clone(), "chart".into(), format!("charts/chart{}.xml", charts.len())));
                                    format!(r#"<a:graphicData uri="{CHART_NS}"><c:chart xmlns:c="{CHART_NS}" r:id="{rid}"/></a:graphicData>"#)
                                }
                            };
                            let _ = write!(
                                body,
                                r#"<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{pic_id}" name="Drawing {pic_id}"/><wp:cNvGraphicFramePr/><a:graphic>{data}</a:graphic></wp:inline></w:drawing></w:r>"#
                            );
                        }
                    }
                }
                if link.is_some() {
                    body.push_str("</w:hyperlink>");
                }
                // A comment running on into the next paragraph stays open.
                if open.is_some() && first_comment(doc.blocks.get(bi + 1)) != open {
                    close(&mut open, &mut body);
                }
                body.push_str("</w:p>");
            }
            Block::PageBreak => {
                list_num = None;
                body.push_str(r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#);
            }
            Block::Section(sec) => {
                list_num = None;
                let _ = write!(body, "<w:p><w:pPr><!--SECT{}--></w:pPr></w:p>", breaks.len());
                breaks.push((sec_cols, sec_type));
                sec_cols = sec.columns.clamp(1, 3) as usize;
                sec_type = if sec.new_page { "nextPage" } else { "continuous" };
            }
            Block::Contents => {
                list_num = None;
                // A TOC field with the entries filled in; Word refreshes it on request.
                body.push_str(&paragraph_xml("Contents", r#"<w:pPr><w:pStyle w:val="TOCHeading"/></w:pPr>"#));
                let width = (doc.page.column_width() * TWIP) as i64;
                let headings = doc.headings();
                body.push_str(r#"<w:p><w:r><w:fldChar w:fldCharType="begin" w:dirty="true"/></w:r><w:r><w:instrText xml:space="preserve"> TOC \o "1-3" \h \z \u </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r></w:p>"#);
                for (level, text, hb) in headings {
                    let page = heading_pages.get(&hb).map(|p| p.to_string()).unwrap_or_default();
                    let _ = write!(
                        body,
                        r#"<w:p><w:pPr><w:pStyle w:val="TOC{}"/><w:tabs><w:tab w:val="right" w:leader="dot" w:pos="{width}"/></w:tabs></w:pPr>{}<w:r><w:tab/></w:r>{}</w:p>"#,
                        level.max(1),
                        text_run(&TextRun { text, ..Default::default() }),
                        text_run(&TextRun { text: page, ..Default::default() }),
                    );
                }
                body.push_str(r#"<w:p><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#);
            }
            Block::Table(t) => {
                list_num = None;
                let cols = t.cols();
                let width = doc.page.content_width() * TWIP;
                let col_w = (width / cols as f64) as i64;
                let _ = write!(
                    body,
                    r#"<w:tbl><w:tblPr><w:tblW w:w="{}" w:type="dxa"/><w:tblBorders><w:top w:val="single" w:sz="4" w:color="9AA0AA"/><w:left w:val="single" w:sz="4" w:color="9AA0AA"/><w:bottom w:val="single" w:sz="4" w:color="9AA0AA"/><w:right w:val="single" w:sz="4" w:color="9AA0AA"/><w:insideH w:val="single" w:sz="4" w:color="C4C8CE"/><w:insideV w:val="single" w:sz="4" w:color="C4C8CE"/></w:tblBorders></w:tblPr><w:tblGrid>"#,
                    width as i64
                );
                for _ in 0..cols {
                    let _ = write!(body, r#"<w:gridCol w:w="{col_w}"/>"#);
                }
                body.push_str("</w:tblGrid>");
                for (ri, row) in t.rows.iter().enumerate() {
                    let head = ri == 0 && t.header;
                    body.push_str("<w:tr>");
                    if head {
                        body.push_str("<w:trPr><w:tblHeader/></w:trPr>");
                    }
                    for c in 0..cols {
                        let text = row.get(c).map(String::as_str).unwrap_or("");
                        let shade = if head { r#"<w:shd w:val="clear" w:color="auto" w:fill="EEF0F4"/>"# } else { "" };
                        let run = if text.is_empty() {
                            String::new()
                        } else {
                            text_run(&TextRun { text: text.into(), bold: head, ..Default::default() })
                        };
                        let _ = write!(
                            body,
                            r#"<w:tc><w:tcPr><w:tcW w:w="{col_w}" w:type="dxa"/>{shade}</w:tcPr><w:p>{run}</w:p></w:tc>"#
                        );
                    }
                    body.push_str("</w:tr>");
                }
                body.push_str("</w:tbl>");
            }
        }
    }
    // The page.
    let paper = doc.page.paper();
    let [t, b, l, r] = doc.page.margins.map(|m| (m * TWIP) as i64);
    let mut sect = String::new();
    let mut extra_parts: Vec<(String, String)> = Vec::new();
    if !doc.page.header.is_empty() {
        let rid = format!("rId{}", rels.len() + 1);
        rels.push((rid.clone(), "header".into(), "header1.xml".into()));
        extra_parts.push(("word/header1.xml".into(), header_footer("hdr", &doc.page.header, false)));
        let _ = write!(sect, r#"<w:headerReference w:type="default" r:id="{rid}"/>"#);
    }
    if !doc.page.footer.is_empty() || doc.page.page_numbers {
        let rid = format!("rId{}", rels.len() + 1);
        rels.push((rid.clone(), "footer".into(), "footer1.xml".into()));
        extra_parts.push(("word/footer1.xml".into(), header_footer("ftr", &doc.page.footer, doc.page.page_numbers)));
        let _ = write!(sect, r#"<w:footerReference w:type="default" r:id="{rid}"/>"#);
    }
    // A plain first page: its own empty header and footer.
    let has_hf = !doc.page.header.is_empty() || !doc.page.footer.is_empty() || doc.page.page_numbers;
    if doc.page.plain_first_page && has_hf {
        for (tag, kind, file) in [("hdr", "header", "header2.xml"), ("ftr", "footer", "footer2.xml")] {
            let rid = format!("rId{}", rels.len() + 1);
            rels.push((rid.clone(), kind.into(), file.into()));
            extra_parts.push((format!("word/{file}"), header_footer(tag, "", false)));
            let _ = write!(sect, r#"<w:{kind}Reference w:type="first" r:id="{rid}"/>"#);
        }
    }
    let title_pg = if doc.page.plain_first_page && has_hf { "<w:titlePg/>" } else { "" };
    let refs = std::mem::take(&mut sect);
    let page = format!(
        r#"<w:pgSz w:w="{}" w:h="{}"{}/><w:pgMar w:top="{t}" w:right="{r}" w:bottom="{b}" w:left="{l}" w:header="{}" w:footer="{}" w:gutter="0"/>"#,
        (paper.width * TWIP).round() as i64,
        (paper.height * TWIP).round() as i64,
        if doc.page.landscape { r#" w:orient="landscape""# } else { "" },
        t / 2,
        b / 2
    );
    let gap = (doc.page.column_gap * TWIP).round() as i64;
    let section = |cols: usize, kind: &str| {
        let cols = if cols > 1 { format!(r#"<w:cols w:num="{cols}" w:space="{gap}"/>"#) } else { String::new() };
        format!(r#"{refs}<w:type w:val="{kind}"/>{page}{cols}{title_pg}"#)
    };
    for (k, (cols, kind)) in breaks.iter().enumerate() {
        body = body.replace(&format!("<!--SECT{k}-->"), &format!("<w:sectPr>{}</w:sectPr>", section(*cols, kind)));
    }
    sect = section(sec_cols, sec_type);
    if doc.tracking {
        let rid = format!("rId{}", rels.len() + 1);
        rels.push((rid, "settings".into(), "settings.xml".into()));
        extra_parts.push((
            "word/settings.xml".into(),
            format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="{W_NS}"><w:trackRevisions/></w:settings>"#),
        ));
    }
    // Footnotes and comments get parts of their own.
    if !footnotes.is_empty() {
        let rid = format!("rId{}", rels.len() + 1);
        rels.push((rid, "footnotes".into(), "footnotes.xml".into()));
        let mut x = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:footnotes xmlns:w="{W_NS}"><w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>"#
        );
        for (i, text) in footnotes.iter().enumerate() {
            let _ = write!(
                x,
                r#"<w:footnote w:id="{}"><w:p><w:pPr><w:pStyle w:val="FootnoteText"/></w:pPr><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteRef/></w:r><w:r><w:t xml:space="preserve"> {}</w:t></w:r></w:p></w:footnote>"#,
                i + 1,
                esc(text)
            );
        }
        x.push_str("</w:footnotes>");
        extra_parts.push(("word/footnotes.xml".into(), x));
    }
    if !endnotes.is_empty() {
        let rid = format!("rId{}", rels.len() + 1);
        rels.push((rid, "endnotes".into(), "endnotes.xml".into()));
        let mut x = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:endnotes xmlns:w="{W_NS}"><w:endnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:endnote><w:endnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:endnote>"#
        );
        for (i, text) in endnotes.iter().enumerate() {
            let _ = write!(
                x,
                r#"<w:endnote w:id="{}"><w:p><w:pPr><w:pStyle w:val="FootnoteText"/></w:pPr><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:endnoteRef/></w:r><w:r><w:t xml:space="preserve"> {}</w:t></w:r></w:p></w:endnote>"#,
                i + 1,
                esc(text)
            );
        }
        x.push_str("</w:endnotes>");
        extra_parts.push(("word/endnotes.xml".into(), x));
    }
    let used: Vec<u32> = doc
        .blocks
        .iter()
        .filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None })
        .flat_map(|p| p.runs.iter())
        .filter_map(|r| if let Run::Text(t) = r { t.comment } else { None })
        .collect();
    let comments: Vec<&Comment> = doc.comments.iter().filter(|c| used.contains(&c.id)).collect();
    for (k, chart) in charts.iter().enumerate() {
        extra_parts.push((format!("word/charts/chart{}.xml", k + 1), chart.clone()));
    }
    if !comments.is_empty() {
        let rid = format!("rId{}", rels.len() + 1);
        rels.push((rid, "comments".into(), "comments.xml".into()));
        let mut x = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:comments xmlns:w="{W_NS}">"#);
        for c in &comments {
            let initials: String = c.author.split_whitespace().filter_map(|w| w.chars().next()).collect();
            let date = if c.date.is_empty() { String::new() } else { format!(r#" w:date="{}""#, esc(&c.date)) };
            let _ = write!(x, r#"<w:comment w:id="{}" w:author="{}" w:initials="{}"{date}>"#, c.id, esc(&c.author), esc(&initials));
            for line in c.text.split('\n') {
                let _ = write!(x, r#"<w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#, esc(line));
            }
            x.push_str("</w:comment>");
        }
        x.push_str("</w:comments>");
        extra_parts.push(("word/comments.xml".into(), x));
        if comments.iter().any(|c| c.resolved) {
            lost.push("which comments are resolved".to_string());
        }
    }
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{W_NS}" xmlns:r="{R_NS}" xmlns:wp="{WP_NS}" xmlns:a="{A_NS}" xmlns:pic="{PIC_NS}" xmlns:wps="{WPS_NS}"><w:body>{body}<w:sectPr>{sect}</w:sectPr></w:body></w:document>"#
    );

    // Styles: the defaults, and each paragraph style's look.
    let mut styles = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{W_NS}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="{f}" w:hAnsi="{f}" w:cs="{f}"/><w:color w:val="{c}"/><w:sz w:val="{sz}"/><w:szCs w:val="{sz}"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="120" w:line="264" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#,
        f = DEFAULT_FONT,
        c = DEFAULT_COLOR.trim_start_matches('#').to_uppercase(),
        sz = (DEFAULT_SIZE * 2.0) as i64
    );
    for s in [Style::Title, Style::H1, Style::H2, Style::H3, Style::Quote] {
        let (size, above, below, bold, italic) = s.look();
        let id = style_id(s).unwrap_or("Normal");
        let outline = match s {
            Style::H1 => "<w:outlineLvl w:val=\"0\"/>",
            Style::H2 => "<w:outlineLvl w:val=\"1\"/>",
            Style::H3 => "<w:outlineLvl w:val=\"2\"/>",
            _ => "",
        };
        let ind = if s == Style::Quote { format!(r#"<w:ind w:left="{}"/>"#, (INDENT_STEP * TWIP) as i64) } else { String::new() };
        let _ = write!(
            styles,
            r#"<w:style w:type="paragraph" w:styleId="{id}"><w:name w:val="{name}"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="{}" w:after="{}"/>{ind}{outline}</w:pPr><w:rPr>{}{}<w:sz w:val="{}"/><w:szCs w:val="{}"/>{}</w:rPr></w:style>"#,
            (above * TWIP) as i64,
            (below * TWIP) as i64,
            if bold { "<w:b/>" } else { "" },
            if italic { "<w:i/>" } else { "" },
            (size * 2.0) as i64,
            (size * 2.0) as i64,
            if s == Style::Quote { r#"<w:color w:val="4A5060"/>"# } else { "" },
            name = match s {
                Style::Title => "Title",
                Style::H1 => "heading 1",
                Style::H2 => "heading 2",
                Style::H3 => "heading 3",
                _ => "Quote",
            }
        );
    }
    for (id, name, extra) in [
        ("TOCHeading", "TOC Heading", r#"<w:rPr><w:b/><w:sz w:val="31"/></w:rPr>"#),
        ("TOC1", "toc 1", r#"<w:pPr><w:spacing w:after="60"/></w:pPr><w:rPr><w:b/></w:rPr>"#),
        ("TOC2", "toc 2", r#"<w:pPr><w:spacing w:after="60"/><w:ind w:left="280"/></w:pPr>"#),
        ("TOC3", "toc 3", r#"<w:pPr><w:spacing w:after="60"/><w:ind w:left="560"/></w:pPr>"#),
        ("FootnoteText", "footnote text", r#"<w:pPr><w:spacing w:after="0"/></w:pPr><w:rPr><w:sz w:val="18"/></w:rPr>"#),
    ] {
        let _ = write!(
            styles,
            r#"<w:style w:type="paragraph" w:styleId="{id}"><w:name w:val="{name}"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/>{extra}</w:style>"#
        );
    }
    let _ = write!(
        styles,
        r#"<w:style w:type="character" w:styleId="Hyperlink"><w:name w:val="Hyperlink"/><w:rPr><w:color w:val="{}"/><w:u w:val="single"/></w:rPr></w:style>"#,
        super::buffer::LINK_INK.trim_start_matches('#').to_uppercase()
    );
    styles.push_str("</w:styles>");

    // Numbering: one bullet list and one numbered list definition, nine levels each.
    let mut numbering = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering xmlns:w="{W_NS}">"#);
    for (abs, bullet) in [(1, true), (2, false)] {
        let _ = write!(numbering, r#"<w:abstractNum w:abstractNumId="{abs}"><w:multiLevelType w:val="hybridMultilevel"/>"#);
        for lvl in 0..9u32 {
            let (fmt, text) = if bullet {
                ("bullet", ["•", "◦", "▪"][lvl as usize % 3].to_string())
            } else {
                (["decimal", "lowerLetter", "lowerRoman"][lvl as usize % 3], format!("%{}.", lvl + 1))
            };
            let left = ((lvl as f64 + 1.0) * INDENT_STEP * TWIP) as i64;
            let _ = write!(
                numbering,
                r#"<w:lvl w:ilvl="{lvl}"><w:start w:val="1"/><w:numFmt w:val="{fmt}"/><w:lvlText w:val="{text}"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="{left}" w:hanging="{h}"/></w:pPr></w:lvl>"#,
                h = (INDENT_STEP * 0.75 * TWIP) as i64
            );
        }
        numbering.push_str("</w:abstractNum>");
    }
    if !nums.iter().any(|n| n.0 == 1) {
        nums.insert(0, (1, false));
    }
    for (id, numbered) in &nums {
        let _ = write!(numbering, r#"<w:num w:numId="{id}"><w:abstractNumId w:val="{}"/>"#, if *numbered { 2 } else { 1 });
        if *numbered {
            numbering.push_str(r#"<w:lvlOverride w:ilvl="0"><w:startOverride w:val="1"/></w:lvlOverride>"#);
        }
        numbering.push_str("</w:num>");
    }
    numbering.push_str("</w:numbering>");

    let mut doc_rels = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="{PKG}">"#);
    for (id, kind, target) in &rels {
        if kind == "hyperlink" {
            let _ = write!(doc_rels, r#"<Relationship Id="{id}" Type="{R_NS}/{kind}" Target="{}" TargetMode="External"/>"#, esc(target));
        } else {
            let _ = write!(doc_rels, r#"<Relationship Id="{id}" Type="{R_NS}/{kind}" Target="{target}"/>"#);
        }
    }
    doc_rels.push_str("</Relationships>");
    let mut types = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>"#,
    );
    for (name, _) in &extra_parts {
        let kind = ["header", "footer", "footnotes", "endnotes", "comments", "settings"].into_iter().find(|k| name.contains(k)).unwrap_or("footer");
        let content = if name.contains("charts/") {
            crate::present::pptx_more::CHART_TYPE.to_string()
        } else {
            format!("application/vnd.openxmlformats-officedocument.wordprocessingml.{kind}+xml")
        };
        let _ = write!(types, r#"<Override PartName="/{name}" ContentType="{content}"/>"#);
    }
    types.push_str("</Types>");

    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut put = |name: &str, bytes: &[u8]| -> Result<()> {
        zip.start_file(name, opts)?;
        zip.write_all(bytes)?;
        Ok(())
    };
    put("[Content_Types].xml", types.as_bytes())?;
    put("_rels/.rels", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="{PKG}"><Relationship Id="rId1" Type="{R_NS}/officeDocument" Target="word/document.xml"/></Relationships>"#).as_bytes())?;
    put("word/document.xml", document.as_bytes())?;
    put("word/styles.xml", styles.as_bytes())?;
    put("word/numbering.xml", numbering.as_bytes())?;
    put("word/_rels/document.xml.rels", doc_rels.as_bytes())?;
    for (name, text) in &extra_parts {
        put(name, text.as_bytes())?;
    }
    for (name, bytes) in &images {
        put(&format!("word/media/{name}"), bytes)?;
    }
    Ok((zip.finish()?.into_inner(), lost))
}

// ---------- Reading ----------

fn part(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<Vec<u8>> {
    let mut f = zip.by_name(name.trim_start_matches('/')).ok()?;
    if f.size() > 64 * 1024 * 1024 {
        return None;
    }
    crate::container::read_capped(&mut f, 64 * 1024 * 1024).ok()
}

fn text_part(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<String> {
    part(zip, name).map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn resolve(base: &str, rel: &str) -> String {
    if let Some(abs) = rel.strip_prefix('/') {
        return abs.to_string();
    }
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

fn wattr<'a>(n: &roxmltree::Node<'a, '_>, local: &str) -> Option<&'a str> {
    n.attributes().find(|a| a.name() == local && a.namespace() == Some(W_NS)).map(|a| a.value())
}

fn child<'a, 'i>(n: &roxmltree::Node<'a, 'i>, local: &str) -> Option<roxmltree::Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name((W_NS, local)))
}

/// An on/off property (`<w:b/>`, `<w:b w:val="0"/>`).
fn on(rpr: &roxmltree::Node, local: &str) -> bool {
    child(rpr, local).is_some_and(|b| wattr(&b, "val").is_none_or(|v| v != "0" && v != "false" && v != "none"))
}

/// Word's highlight names.
fn highlight(name: &str) -> Option<&'static str> {
    Some(match name {
        "yellow" => "#fff59d",
        "green" | "brightGreen" => "#c8e6c9",
        "cyan" | "turquoise" => "#b2ebf2",
        "magenta" | "pink" => "#f8bbd0",
        "blue" => "#bbdefb",
        "red" => "#ffcdd2",
        "lightGray" | "darkGray" => "#e0e0e0",
        _ => return None,
    })
}

fn run_format(rpr: Option<roxmltree::Node>) -> TextRun {
    let mut r = TextRun::default();
    let Some(rpr) = rpr else { return r };
    r.bold = on(&rpr, "b");
    r.italic = on(&rpr, "i");
    r.strike = on(&rpr, "strike") || on(&rpr, "dstrike");
    match child(&rpr, "vertAlign").and_then(|v| wattr(&v, "val")) {
        Some("superscript") => r.superscript = true,
        Some("subscript") => r.subscript = true,
        _ => {}
    }
    r.underline = child(&rpr, "u").is_some_and(|u| wattr(&u, "val").is_some_and(|v| v != "none"));
    r.font = child(&rpr, "rFonts")
        .and_then(|f| wattr(&f, "ascii").or(wattr(&f, "hAnsi")))
        .filter(|f| *f != DEFAULT_FONT)
        .map(String::from);
    r.size = child(&rpr, "sz")
        .and_then(|s| wattr(&s, "val"))
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| v / 2.0)
        .filter(|s| (*s - DEFAULT_SIZE).abs() > 0.01);
    r.color = child(&rpr, "color")
        .and_then(|c| wattr(&c, "val"))
        .filter(|v| v.len() == 6 && *v != "auto")
        .map(|v| format!("#{}", v.to_lowercase()))
        .filter(|c| c != DEFAULT_COLOR && c != "#000000");
    r.highlight = child(&rpr, "highlight").and_then(|h| wattr(&h, "val")).and_then(highlight).map(String::from).or_else(|| {
        child(&rpr, "shd")
            .and_then(|s| wattr(&s, "fill"))
            .filter(|v| v.len() == 6 && !v.eq_ignore_ascii_case("ffffff"))
            .map(|v| format!("#{}", v.to_lowercase()))
    });
    r
}

struct Reader<'z, 'b> {
    zip: &'z mut zip::ZipArchive<Cursor<&'b [u8]>>,
    /// Footnote and endnote words by Word's id.
    footnotes: HashMap<String, String>,
    endnotes: HashMap<String, String>,
    /// Section ends met so far: the break's block, and the section it ends.
    sections: Vec<(usize, SectInfo)>,
    /// Word's comment id → ours, for those seen so far.
    comment_ids: HashMap<String, u32>,
    /// Comments from comments.xml by Word's id: (author, date, text).
    comment_text: HashMap<String, (String, String, String)>,
    comments: Vec<Comment>,
    revision_ids: HashMap<String, u32>,
    revisions: Vec<Revision>,
    /// The comment whose range is open (ranges can run across paragraphs).
    open_comment: Option<u32>,
    /// A page break in the paragraph just read: before its text, or after.
    break_before: bool,
    break_after: bool,
    rels: HashMap<String, String>,
    /// Links' relationship ids → their addresses.
    links: HashMap<String, String>,
    /// numId → levels' formats (bullet or not).
    numbering: HashMap<String, Vec<bool>>,
    /// Paragraph style id → our style.
    styles: HashMap<String, Style>,
    media: BTreeMap<String, Vec<u8>>,
    max_width: f64,
    /// Inside a field's result (a table of contents) that spans paragraphs.
    in_field: bool,
}

/// All the words in a footnote or comment, its paragraphs joined.
fn block_text(n: &roxmltree::Node) -> String {
    n.children()
        .filter(|p| p.has_tag_name((W_NS, "p")))
        .map(|p| p.descendants().filter(|t| t.has_tag_name((W_NS, "t"))).filter_map(|t| t.text()).collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

/// Does this paragraph begin a field of this kind (`TOC`)?
fn field_starts(p: &roxmltree::Node, kind: &str) -> bool {
    p.descendants().any(|c| c.has_tag_name((W_NS, "instrText")) && c.text().is_some_and(|t| t.trim_start().starts_with(kind)))
}

fn field_ends(p: &roxmltree::Node) -> bool {
    p.descendants().any(|c| c.has_tag_name((W_NS, "fldChar")) && wattr(&c, "fldCharType") == Some("end"))
}

/// A table of contents: one block; its paragraphs (to the field's end) are skipped.
fn skip_field(reader: &mut Reader, p: &roxmltree::Node, doc: &mut Document) {
    // Our own "Contents" heading paragraph goes with it.
    if let Some(Block::Paragraph(last)) = doc.blocks.last()
        && last.plain_text_any() == "Contents"
    {
        doc.blocks.pop();
    }
    doc.blocks.push(Block::Contents);
    reader.in_field = !field_ends(p);
}

/// A section's columns, and whether it carries on from the one before.
#[derive(Clone, Copy)]
struct SectInfo {
    cols: u8,
    continuous: bool,
}

impl Default for SectInfo {
    fn default() -> Self {
        SectInfo { cols: 1, continuous: false }
    }
}

fn sect_info(sect: &roxmltree::Node) -> SectInfo {
    SectInfo {
        cols: child(sect, "cols").and_then(|c| wattr(&c, "num")).and_then(|v| v.parse::<u8>().ok()).unwrap_or(1).clamp(1, 3),
        continuous: child(sect, "type").and_then(|t| wattr(&t, "val")) == Some("continuous"),
    }
}

/// A paragraph, with page breaks before or after it as Word had them, and a
/// section break after it if it ends a section.
fn push_paragraph(reader: &mut Reader, n: &roxmltree::Node, doc: &mut Document) {
    let para = reader.paragraph(n);
    if reader.break_before && !matches!(doc.blocks.last(), Some(Block::PageBreak) | None) {
        doc.blocks.push(Block::PageBreak);
    }
    let ends_section = child(n, "pPr").and_then(|p| child(&p, "sectPr"));
    // A paragraph that only held a break isn't kept.
    let empty = para.runs.is_empty() && para.list.is_none();
    if !(empty && (reader.break_before || reader.break_after || ends_section.is_some())) {
        doc.blocks.push(Block::Paragraph(para));
    }
    if reader.break_after {
        doc.blocks.push(Block::PageBreak);
    }
    if let Some(sect) = ends_section {
        // Filled in once the next section's settings are known.
        doc.blocks.push(Block::Section(Section { columns: 1, new_page: true }));
        reader.sections.push((doc.blocks.len() - 1, sect_info(&sect)));
    }
}

impl Reader<'_, '_> {
    fn paragraph(&mut self, p: &roxmltree::Node) -> Paragraph {
        let ppr = child(p, "pPr");
        let style = ppr
            .and_then(|pp| child(&pp, "pStyle"))
            .and_then(|s| wattr(&s, "val"))
            .map(|id| self.styles.get(id).copied().unwrap_or_else(|| style_from_id(id)))
            .unwrap_or_default();
        let align = match ppr.and_then(|pp| child(&pp, "jc")).and_then(|j| wattr(&j, "val")) {
            Some("center") => Align::Center,
            Some("right") | Some("end") => Align::Right,
            Some("both") | Some("distribute") => Align::Justify,
            _ => Align::Left,
        };
        let mut list = None;
        let mut indent = 0u8;
        if let Some(np) = ppr.and_then(|pp| child(&pp, "numPr")) {
            let lvl = child(&np, "ilvl").and_then(|l| wattr(&l, "val")).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            if let Some(levels) = child(&np, "numId").and_then(|n| wattr(&n, "val")).and_then(|id| self.numbering.get(id)) {
                list = Some(if levels.get(lvl).copied().unwrap_or(true) { List::Bullet } else { List::Number });
                indent = lvl.min(MAX_INDENT as usize) as u8;
            }
        } else if let Some(left) = ppr
            .and_then(|pp| child(&pp, "ind"))
            .and_then(|i| wattr(&i, "left").or(wattr(&i, "start")))
            .and_then(|v| v.parse::<f64>().ok())
        {
            indent = ((left / TWIP) / INDENT_STEP).round().clamp(0.0, MAX_INDENT as f64) as u8;
        }
        // Spacing set on the paragraph itself (a style's own spacing is the style's).
        let spacing = ppr.and_then(|pp| child(&pp, "spacing"));
        let twips = |name: &str| spacing.and_then(|s| wattr(&s, name)).and_then(|v| v.parse::<f64>().ok()).map(|v| v / TWIP);
        let (above, below) = style.spacing();
        let line_spacing = spacing
            .filter(|s| wattr(s, "lineRule").is_none_or(|r| r == "auto"))
            .and_then(|s| wattr(&s, "line"))
            .and_then(|v| v.parse::<f64>().ok())
            .map(|v| (v / 240.0 * 100.0).round() / 100.0)
            .filter(|l| *l > 0.0 && (*l - 1.0).abs() > 0.001);
        let mut para = Paragraph {
            style,
            align,
            list,
            indent: if style == Style::Quote { indent.saturating_sub(1) } else { indent },
            line_spacing,
            space_before: twips("before").filter(|v| (v - above).abs() > 0.01),
            space_after: twips("after").filter(|v| (v - below).abs() > 0.01),
            runs: Vec::new(),
        };
        self.break_before = ppr.is_some_and(|pp| on(&pp, "pageBreakBefore"));
        self.break_after = false;
        // Runs, including those inside links and tracked insertions, with the
        // comment ranges between them.
        for r in p.descendants() {
            if r.has_tag_name((W_NS, "commentRangeStart")) {
                if let Some(wid) = wattr(&r, "id") {
                    self.open_comment = Some(self.comment_id(wid));
                }
                continue;
            }
            if r.has_tag_name((W_NS, "commentRangeEnd")) {
                if wattr(&r, "id").map(|w| self.comment_id(w)) == self.open_comment {
                    self.open_comment = None;
                }
                continue;
            }
            if !r.has_tag_name((W_NS, "r")) {
                continue;
            }
            if r.ancestors().skip(1).any(|a| a.has_tag_name((W_NS, "r"))) {
                continue;
            }
            let mut fmt = run_format(child(&r, "rPr"));
            // Inside a link to a web address or file (not to a place in the document).
            fmt.link = r
                .ancestors()
                .skip(1)
                .find(|a| a.has_tag_name((W_NS, "hyperlink")))
                .and_then(|h| h.attributes().find(|a| a.name() == "id").map(|a| a.value()))
                .and_then(|rid| self.links.get(rid).cloned());
            fmt.comment = self.open_comment;
            // Tracked changes: the run sits inside w:ins or w:del. Moved text is
            // a w:moveFrom where it was and a w:moveTo where it went: a deletion
            // and an insertion, so it isn't read twice.
            let is = |a: &roxmltree::Node, names: &[&str]| names.iter().any(|n| a.has_tag_name((W_NS, *n)));
            if let Some(change) = r.ancestors().skip(1).find(|a| is(a, &["ins", "del", "moveTo", "moveFrom"])) {
                let id = self.revision_id(&change);
                if is(&change, &["ins", "moveTo"]) {
                    fmt.inserted = Some(id);
                } else {
                    fmt.deleted = Some(id);
                }
            }
            for c in r.children() {
                if c.has_tag_name((W_NS, "br")) && wattr(&c, "type") == Some("page") {
                    // Text so far stays on this page; the rest of the paragraph is lost
                    // to the break only in where it starts, so mark it after.
                    if para.runs.is_empty() {
                        self.break_before = true;
                    } else {
                        self.break_after = true;
                    }
                    continue;
                }
                if c.has_tag_name((W_NS, "footnoteReference")) {
                    if let Some(text) = wattr(&c, "id").and_then(|id| self.footnotes.get(id)) {
                        para.runs.push(Run::Note(Note { note: text.clone(), end: false }));
                    }
                    continue;
                }
                if c.has_tag_name((W_NS, "endnoteReference")) {
                    if let Some(text) = wattr(&c, "id").and_then(|id| self.endnotes.get(id)) {
                        para.runs.push(Run::Note(Note { note: text.clone(), end: true }));
                    }
                    continue;
                }
                if c.has_tag_name((W_NS, "t")) || c.has_tag_name((W_NS, "delText")) {
                    para.runs.push(Run::Text(TextRun { text: c.text().unwrap_or("").to_string(), ..fmt.clone() }));
                } else if c.has_tag_name((W_NS, "tab")) {
                    para.runs.push(Run::Text(TextRun { text: "\t".into(), ..fmt.clone() }));
                } else if c.has_tag_name((W_NS, "br")) || c.has_tag_name((W_NS, "cr")) {
                    para.runs.push(Run::Text(TextRun { text: LINE_BREAK.to_string(), ..fmt.clone() }));
                } else if c.has_tag_name((W_NS, "drawing"))
                    && let Some(d) = self.figure(&c)
                {
                    para.runs.push(Run::Drawing(d));
                } else if c.has_tag_name((W_NS, "drawing"))
                    && let Some(img) = self.picture(&c)
                {
                    para.runs.push(Run::Image(img));
                }
            }
        }
        // Headings take their weight from the style, not the runs.
        if para.style.is_heading() {
            for r in &mut para.runs {
                if let Run::Text(t) = r {
                    t.bold = false;
                    t.size = None;
                }
            }
        }
        para
    }

    /// Our id for a w:ins or w:del, adding its revision the first time.
    fn revision_id(&mut self, n: &roxmltree::Node) -> u32 {
        let wid = wattr(n, "id").unwrap_or("").to_string();
        if let Some(id) = self.revision_ids.get(&wid) {
            return *id;
        }
        let id = self.revision_ids.len() as u32 + 1;
        self.revision_ids.insert(wid, id);
        self.revisions.push(Revision {
            id,
            author: wattr(n, "author").unwrap_or("").to_string(),
            date: wattr(n, "date").unwrap_or("").to_string(),
        });
        id
    }

    /// Our id for Word's comment `wid`, adding the comment the first time.
    fn comment_id(&mut self, wid: &str) -> u32 {
        if let Some(id) = self.comment_ids.get(wid) {
            return *id;
        }
        let id = self.comment_ids.len() as u32 + 1;
        self.comment_ids.insert(wid.to_string(), id);
        let (author, date, text) = self.comment_text.get(wid).cloned().unwrap_or_default();
        self.comments.push(Comment { id, author, date, text, resolved: false });
        id
    }

    /// A shape or chart drawn in the text.
    fn figure(&mut self, drawing: &roxmltree::Node) -> Option<Drawing> {
        let data = drawing.descendants().find(|n| n.has_tag_name((A_NS, "graphicData")))?;
        let extent = drawing.descendants().find(|n| n.has_tag_name((WP_NS, "extent")))?;
        let size = |k: &str| extent.attribute(k).and_then(|v| v.parse::<f64>().ok()).map(|v| v / EMU);
        let (width, height) = (size("cx")?.min(self.max_width).max(16.0), size("cy")?.max(16.0));
        let figure = match data.attribute("uri")? {
            WPS_NS => {
                let prst = data.descendants().find(|n| n.has_tag_name((A_NS, "prstGeom"))).and_then(|g| g.attribute("prst"))?;
                let fill = data
                    .descendants()
                    .find(|n| n.has_tag_name((A_NS, "solidFill")))
                    .and_then(|f| f.children().find(|c| c.has_tag_name((A_NS, "srgbClr"))))
                    .and_then(|c| c.attribute("val"))
                    .map(|v| format!("#{}", v.to_lowercase()))
                    .unwrap_or_else(|| "#5273cf".into());
                Figure::Shape { shape: crate::present::model::Shape::from_preset(prst), fill }
            }
            CHART_NS => {
                let rid = data.children().find(|n| n.tag_name().name() == "chart")?.attributes().find(|a| a.name() == "id")?.value();
                let target = self.rels.get(rid)?.clone();
                let xml = text_part(self.zip, &target)?;
                match crate::present::pptx_more::read_chart(&xml)? {
                    crate::present::model::Kind::Chart { chart, names, labels, series } => Figure::Chart { chart, names, labels, series },
                    _ => return None,
                }
            }
            _ => return None,
        };
        Some(Drawing { drawing: figure, width, height })
    }

    fn picture(&mut self, drawing: &roxmltree::Node) -> Option<Image> {
        let blip = drawing.descendants().find(|n| n.has_tag_name((A_NS, "blip")))?;
        let rid = blip.attributes().find(|a| a.name() == "embed").map(|a| a.value())?;
        let target = self.rels.get(rid)?.clone();
        let bytes = part(self.zip, &target)?;
        let png = if bytes.starts_with(b"\x89PNG") {
            bytes
        } else {
            use gtk::prelude::*;
            gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from(&bytes)).ok()?.save_to_png_bytes().to_vec()
        };
        let extent = drawing.descendants().find(|n| n.has_tag_name((WP_NS, "extent")));
        let width = extent
            .and_then(|e| e.attribute("cx"))
            .and_then(|v| v.parse::<f64>().ok())
            .map(|v| v / EMU)
            .unwrap_or(240.0)
            .min(self.max_width)
            .max(16.0);
        let name = format!("picture-{}.png", self.media.len() + 1);
        self.media.insert(name.clone(), png);
        Some(Image { image: name, width, wrap: anchor_side(drawing, self.max_width) })
    }
}

/// A floating picture with text wrapped around it: which side it sits on.
fn anchor_side(drawing: &roxmltree::Node, column: f64) -> Option<Side> {
    let anchor = drawing.children().find(|n| n.has_tag_name((WP_NS, "anchor")))?;
    let wraps = ["wrapSquare", "wrapTight", "wrapThrough"];
    if !anchor.children().any(|n| wraps.iter().any(|w| n.has_tag_name((WP_NS, *w)))) {
        return None;
    }
    let h = anchor.children().find(|n| n.has_tag_name((WP_NS, "positionH")))?;
    if let Some(align) = h.children().find(|n| n.has_tag_name((WP_NS, "align"))).and_then(|a| a.text()) {
        return Some(if matches!(align.trim(), "right" | "outside") { Side::Right } else { Side::Left });
    }
    let offset = h
        .children()
        .find(|n| n.has_tag_name((WP_NS, "posOffset")))
        .and_then(|o| o.text())
        .and_then(|t| t.trim().parse::<f64>().ok())
        .map(|v| v / EMU)
        .unwrap_or(0.0);
    let width = anchor
        .children()
        .find(|n| n.has_tag_name((WP_NS, "extent")))
        .and_then(|e| e.attribute("cx"))
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| v / EMU)
        .unwrap_or(0.0);
    Some(if offset + width / 2.0 > column / 2.0 { Side::Right } else { Side::Left })
}

fn style_from_id(id: &str) -> Style {
    let l = id.to_lowercase().replace(' ', "");
    match l.as_str() {
        "title" => Style::Title,
        "heading1" | "1" => Style::H1,
        "heading2" | "2" => Style::H2,
        "heading3" | "heading4" | "heading5" | "heading6" | "3" => Style::H3,
        "quote" | "intensequote" => Style::Quote,
        _ => Style::Normal,
    }
}

pub fn read(bytes: &[u8]) -> Result<(Document, BTreeMap<String, Vec<u8>>)> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).context("it isn't a Word document")?;
    let root = text_part(&mut zip, "_rels/.rels").unwrap_or_default();
    let doc_path = roxmltree::Document::parse(&root)
        .ok()
        .and_then(|d| {
            d.descendants()
                .find(|n| n.has_tag_name("Relationship") && n.attribute("Type").is_some_and(|t| t.ends_with("/officeDocument")))
                .and_then(|n| n.attribute("Target").map(|t| resolve("", t)))
        })
        .unwrap_or_else(|| "word/document.xml".into());
    let base = doc_path.rsplit_once('/').map(|(a, _)| a.to_string()).unwrap_or_default();
    let xml = text_part(&mut zip, &doc_path).context("the document is missing")?;
    let rels_path = format!("{base}/_rels/{}.rels", doc_path.rsplit('/').next().unwrap_or("document.xml"));
    let rels_xml = text_part(&mut zip, &rels_path).unwrap_or_default();
    let rels_doc = roxmltree::Document::parse(&rels_xml).ok();
    let rel_nodes = || rels_doc.iter().flat_map(|d| d.descendants()).filter(|n| n.has_tag_name("Relationship"));
    let external = |n: &roxmltree::Node| n.attribute("TargetMode") == Some("External");
    let rels: HashMap<String, String> = rel_nodes()
        .filter(|n| !external(n))
        .filter_map(|n| Some((n.attribute("Id")?.to_string(), resolve(&base, n.attribute("Target")?))))
        .collect();
    let links: HashMap<String, String> = rel_nodes()
        .filter(|n| external(n) && n.attribute("Type").is_some_and(|t| t.ends_with("/hyperlink")))
        .filter_map(|n| Some((n.attribute("Id")?.to_string(), n.attribute("Target")?.to_string())))
        .collect();

    // Which numbering instances are bullets, level by level.
    let mut numbering = HashMap::new();
    if let Some(x) = text_part(&mut zip, &format!("{base}/numbering.xml"))
        && let Ok(d) = roxmltree::Document::parse(&x)
    {
        let mut abstracts: HashMap<String, Vec<bool>> = HashMap::new();
        for a in d.descendants().filter(|n| n.has_tag_name((W_NS, "abstractNum"))) {
            let Some(id) = wattr(&a, "abstractNumId") else { continue };
            let mut levels = vec![true; 9];
            for l in a.children().filter(|n| n.has_tag_name((W_NS, "lvl"))) {
                let i = wattr(&l, "ilvl").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
                let fmt = child(&l, "numFmt").and_then(|f| wattr(&f, "val")).unwrap_or("bullet");
                if i < 9 {
                    levels[i] = fmt == "bullet" || fmt == "none";
                }
            }
            abstracts.insert(id.to_string(), levels);
        }
        for n in d.descendants().filter(|n| n.has_tag_name((W_NS, "num"))) {
            if let (Some(id), Some(abs)) = (wattr(&n, "numId"), child(&n, "abstractNumId").and_then(|a| wattr(&a, "val"))) {
                numbering.insert(id.to_string(), abstracts.get(abs).cloned().unwrap_or_else(|| vec![true; 9]));
            }
        }
    }
    // Paragraph styles by id, through their names.
    let mut styles = HashMap::new();
    if let Some(x) = text_part(&mut zip, &format!("{base}/styles.xml"))
        && let Ok(d) = roxmltree::Document::parse(&x)
    {
        for s in d.descendants().filter(|n| n.has_tag_name((W_NS, "style")) && wattr(n, "type") == Some("paragraph")) {
            let (Some(id), name) = (wattr(&s, "styleId"), child(&s, "name").and_then(|n| wattr(&n, "val"))) else { continue };
            let style = name.map(style_from_id).filter(|s| *s != Style::Normal).unwrap_or_else(|| style_from_id(id));
            styles.insert(id.to_string(), style);
        }
    }

    let d = roxmltree::Document::parse(&xml).context("the document is damaged")?;
    let body = d.descendants().find(|n| n.has_tag_name((W_NS, "body"))).context("the document has no body")?;
    let mut doc = Document::default();
    // The page: size and margins from the last section.
    if let Some(sect) = body.children().rfind(|n| n.has_tag_name((W_NS, "sectPr"))) {
        if let Some(sz) = child(&sect, "pgSz") {
            let w = wattr(&sz, "w").and_then(|v| v.parse::<f64>().ok()).unwrap_or(12240.0) / TWIP;
            let h = wattr(&sz, "h").and_then(|v| v.parse::<f64>().ok()).unwrap_or(15840.0) / TWIP;
            doc.page.paper = crate::print::Paper::closest(w, h).unwrap_or("letter").into();
            doc.page.landscape = w > h || wattr(&sz, "orient") == Some("landscape");
        }
        doc.page.plain_first_page = child(&sect, "titlePg").is_some_and(|t| wattr(&t, "val").is_none_or(|v| v != "0" && v != "false"));
        if let Some(m) = child(&sect, "pgMar") {
            let g = |k: &str| wattr(&m, k).and_then(|v| v.parse::<f64>().ok()).map(|v| (v / TWIP).clamp(18.0, 216.0));
            doc.page.margins = [
                g("top").unwrap_or(72.0),
                g("bottom").unwrap_or(72.0),
                g("left").or(g("start")).unwrap_or(72.0),
                g("right").or(g("end")).unwrap_or(72.0),
            ];
        }
    }
    let max_width = doc.page.content_width();
    // Footnotes and comments, from their own parts.
    let part_of = |kind: &str| rels.values().find(|t| t.ends_with(&format!("{kind}.xml"))).cloned();
    let part_of_settings = part_of("settings");
    let mut notes = |part: &str, tag: &str| {
        let mut out = HashMap::new();
        if let Some(path) = part_of(part)
            && let Some(x) = text_part(&mut zip, &path)
            && let Ok(d) = roxmltree::Document::parse(&x)
        {
            for f in d.descendants().filter(|n| n.has_tag_name((W_NS, tag)) && wattr(n, "type").is_none()) {
                let Some(id) = wattr(&f, "id") else { continue };
                out.insert(id.to_string(), block_text(&f));
            }
        }
        out
    };
    let footnotes = notes("footnotes", "footnote");
    let endnotes = notes("endnotes", "endnote");
    let mut comment_text = HashMap::new();
    if let Some(path) = part_of("comments")
        && let Some(x) = text_part(&mut zip, &path)
        && let Ok(d) = roxmltree::Document::parse(&x)
    {
        for c in d.descendants().filter(|n| n.has_tag_name((W_NS, "comment"))) {
            let Some(id) = wattr(&c, "id") else { continue };
            let lines: Vec<String> = c
                .children()
                .filter(|p| p.has_tag_name((W_NS, "p")))
                .map(|p| p.descendants().filter(|t| t.has_tag_name((W_NS, "t"))).filter_map(|t| t.text()).collect())
                .collect();
            comment_text.insert(
                id.to_string(),
                (
                    wattr(&c, "author").unwrap_or("").to_string(),
                    wattr(&c, "date").unwrap_or("").to_string(),
                    lines.join("\n").trim().to_string(),
                ),
            );
        }
    }
    if let Some(sect) = body.children().rfind(|n| n.has_tag_name((W_NS, "sectPr")))
        && let Some(cols) = child(&sect, "cols")
    {
        doc.page.columns = wattr(&cols, "num").and_then(|v| v.parse::<u8>().ok()).unwrap_or(1).clamp(1, 3);
        if let Some(space) = wattr(&cols, "space").and_then(|v| v.parse::<f64>().ok()) {
            doc.page.column_gap = (space / TWIP).clamp(6.0, 72.0);
        }
    }
    let mut reader = Reader {
        zip: &mut zip,
        footnotes,
        endnotes,
        sections: Vec::new(),
        comment_ids: HashMap::new(),
        comment_text,
        comments: Vec::new(),
        revision_ids: HashMap::new(),
        revisions: Vec::new(),
        open_comment: None,
        break_before: false,
        break_after: false,
        rels,
        links,
        numbering,
        styles,
        media: BTreeMap::new(),
        max_width,
        in_field: false,
    };
    for n in body.children() {
        if n.has_tag_name((W_NS, "p")) {
            // A table of contents field: kept as one block, its entries left to us.
            if field_starts(&n, "TOC") {
                skip_field(&mut reader, &n, &mut doc);
                continue;
            }
            if reader.in_field {
                if field_ends(&n) {
                    reader.in_field = false;
                }
                continue;
            }
            push_paragraph(&mut reader, &n, &mut doc);
        } else if n.has_tag_name((W_NS, "tbl")) {
            let mut t = Table { rows: Vec::new(), header: false };
            for tr in n.children().filter(|c| c.has_tag_name((W_NS, "tr"))) {
                if t.rows.is_empty() && child(&tr, "trPr").is_some_and(|p| child(&p, "tblHeader").is_some()) {
                    t.header = true;
                }
                let row: Vec<String> = tr
                    .children()
                    .filter(|c| c.has_tag_name((W_NS, "tc")))
                    .map(|tc| {
                        tc.descendants()
                            .filter(|p| p.has_tag_name((W_NS, "p")))
                            .map(|p| {
                                p.descendants()
                                    .filter(|t| t.has_tag_name((W_NS, "t")))
                                    .filter_map(|t| t.text())
                                    .collect::<String>()
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .collect();
                t.rows.push(row);
            }
            let cols = t.cols();
            for r in &mut t.rows {
                r.resize(cols, String::new());
            }
            if !t.rows.is_empty() {
                doc.blocks.push(Block::Table(t));
            }
        } else if n.has_tag_name((W_NS, "sdt")) {
            // Word wraps its table of contents in a content control.
            if n.descendants().any(|c| c.has_tag_name((W_NS, "instrText")) && c.text().is_some_and(|t| t.trim_start().starts_with("TOC"))) {
                doc.blocks.push(Block::Contents);
                continue;
            }
            // Other content controls hold ordinary paragraphs.
            for p in n.descendants().filter(|c| c.has_tag_name((W_NS, "p"))) {
                push_paragraph(&mut reader, &p, &mut doc);
            }
        }
    }
    // Each break takes the settings of the section after it; the first
    // section's columns are the page's.
    if !reader.sections.is_empty() {
        let last = body.children().rfind(|n| n.has_tag_name((W_NS, "sectPr"))).map(|s| sect_info(&s)).unwrap_or_default();
        let mut infos: Vec<SectInfo> = reader.sections.iter().map(|(_, s)| *s).collect();
        infos.push(last);
        doc.page.columns = infos[0].cols;
        for (k, (at, _)) in reader.sections.iter().enumerate() {
            let next = infos[k + 1];
            if let Some(b) = doc.blocks.get_mut(*at) {
                *b = Block::Section(Section { columns: next.cols, new_page: !next.continuous });
            }
        }
    }
    doc.comments = std::mem::take(&mut reader.comments);
    doc.revisions = std::mem::take(&mut reader.revisions);
    if let Some(path) = part_of_settings.as_deref()
        && let Some(x) = text_part(reader.zip, path)
    {
        doc.tracking = x.contains("trackRevisions") && !x.contains(r#"trackRevisions w:val="0""#) && !x.contains(r#"trackRevisions w:val="false""#);
    }
    let media = std::mem::take(&mut reader.media);
    if doc.blocks.is_empty() {
        bail!("the document is empty");
    }
    doc.tidy();
    Ok((doc, media))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_word() {
        let png = crate::present::model::base64(
            "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAAEUlEQVR42mP8z8BQz0AEYBwAAP8IAXv9qyUAAAAASUVORK5CYII=",
        )
        .unwrap();
        let mut media = BTreeMap::new();
        media.insert("pic.png".to_string(), png.clone());
        let mut doc = super::super::templates::build("recipe").unwrap();
        doc.blocks.push(Block::Paragraph(Paragraph {
            align: Align::Center,
            runs: vec![
                Run::styled("big & red", |r| {
                    r.size = Some(18.0);
                    r.color = Some("#c62828".into());
                    r.underline = true;
                    r.font = Some("Noto Serif".into());
                }),
                Run::styled(" marked", |r| r.highlight = Some("#fff59d".into())),
                Run::Image(Image { image: "pic.png".into(), width: 120.0, wrap: Some(Side::Right) }),
            ],
            ..Default::default()
        }));
        doc.blocks.push(Block::Paragraph(Paragraph { indent: 2, runs: vec![Run::text("indented")], ..Default::default() }));
        doc.blocks.push(Block::Paragraph(Paragraph {
            list: Some(List::Number),
            indent: 1,
            runs: vec![Run::text("sub step")],
            ..Default::default()
        }));
        doc.blocks
            .push(Block::Table(Table { rows: vec![vec!["a".into(), "b".into()], vec!["1".into(), "2".into()]], header: true }));
        doc.page = PageSetup {
            paper: "a4".into(),
            landscape: false,
            margins: [54.0, 60.0, 72.0, 90.0],
            header: "Head".into(),
            footer: String::new(),
            page_numbers: true,
            plain_first_page: false,
            columns: 2,
            column_gap: 24.0,
        };
        doc.tidy();

        let (bytes, lost) = write(&doc, &media).unwrap();
        assert!(lost.is_empty());
        let (back, back_media) = read(&bytes).unwrap();
        assert_eq!(back.page.paper, "a4");
        assert!(!back.page.plain_first_page);
        let mut plain = doc.clone();
        plain.page.plain_first_page = true;
        let (bytes, _) = write(&plain, &media).unwrap();
        assert!(read(&bytes).unwrap().0.page.plain_first_page);
        for (paper, landscape) in [("a5", true), ("legal", false), ("legal", true)] {
            let mut turned = Document { blocks: vec![Block::Paragraph(Paragraph::text(Style::Normal, "Turned"))], ..Default::default() };
            turned.page.paper = paper.into();
            turned.page.landscape = landscape;
            let (bytes, _) = write(&turned, &BTreeMap::new()).unwrap();
            let (back, _) = read(&bytes).unwrap();
            assert_eq!((back.page.paper.as_str(), back.page.landscape), (paper, landscape));
        }
        assert_eq!(back.page.margins, [54.0, 60.0, 72.0, 90.0]);
        assert_eq!(back.blocks.len(), doc.blocks.len());
        for (a, b) in doc.blocks.iter().zip(&back.blocks) {
            match (a, b) {
                (Block::Paragraph(a), Block::Paragraph(b)) => {
                    assert_eq!(a.style, b.style, "{a:?}");
                    assert_eq!(a.list, b.list, "{a:?}");
                    assert_eq!(a.indent, b.indent, "{a:?}");
                    assert_eq!(a.align, b.align);
                    assert_eq!(a.plain_text(), b.plain_text());
                }
                (Block::Table(a), Block::Table(b)) => assert_eq!(a, b),
                _ => panic!("blocks differ: {a:?} / {b:?}"),
            }
        }
        let Block::Paragraph(p) = &back.blocks[back.blocks.len() - 4] else { panic!() };
        let Run::Text(t) = &p.runs[0] else { panic!() };
        assert_eq!(
            (t.size, t.color.as_deref(), t.underline, t.font.as_deref()),
            (Some(18.0), Some("#c62828"), true, Some("Noto Serif"))
        );
        assert!(matches!(&p.runs[1], Run::Text(t) if t.highlight.as_deref() == Some("#fff59d")));
        assert!(matches!(&p.runs[2], Run::Image(i) if (i.width - 120.0).abs() < 0.01 && i.wrap == Some(Side::Right)));
        assert_eq!(back_media.values().next(), Some(&png));
        // The recipe's bold labels survive.
        let Block::Paragraph(p) = &back.blocks[1] else { panic!() };
        assert!(matches!(&p.runs[0], Run::Text(t) if t.bold && t.text == "Serves: "));
        assert_eq!((back.page.columns, back.page.column_gap), (2, 24.0));
    }

    #[test]
    fn notes_comments_breaks_and_contents() {
        let mut doc = Document::default();
        let para = |runs: Vec<Run>| Block::Paragraph(Paragraph { runs, ..Default::default() });
        doc.blocks = vec![
            Block::Contents,
            Block::Paragraph(Paragraph::text(Style::H1, "First")),
            para(vec![
                Run::text("Plain, then "),
                Run::styled("commented", |r| r.comment = Some(7)),
                Run::Note(Note { note: "See the source.".into(), end: false }),
                Run::text(" after."),
            ]),
            para(vec![Run::styled("still commented", |r| r.comment = Some(7))]),
            Block::PageBreak,
            Block::Paragraph(Paragraph::text(Style::H2, "Second")),
            para(vec![Run::text("x"), Run::Note(Note { note: "Two.".into(), end: false })]),
        ];
        doc.comments =
            vec![Comment { id: 7, author: "Ken Doe".into(), date: "2026-10-05T10:00:00Z".into(), text: "Check this".into(), resolved: true }];
        let (bytes, lost) = write(&doc, &BTreeMap::new()).unwrap();
        assert_eq!(lost, ["which comments are resolved"]);
        let (back, _) = read(&bytes).unwrap();
        let kinds: Vec<&str> = back
            .blocks
            .iter()
            .map(|b| match b {
                Block::Contents => "toc",
                Block::PageBreak => "break",
                Block::Section(_) => "section",
                Block::Table(_) => "table",
                Block::Paragraph(_) => "p",
            })
            .collect();
        assert_eq!(kinds, ["toc", "p", "p", "p", "break", "p", "p"]);
        let Block::Paragraph(p) = &back.blocks[2] else { panic!() };
        let notes: Vec<&str> = p.runs.iter().filter_map(|r| if let Run::Note(n) = r { Some(n.note.as_str()) } else { None }).collect();
        assert_eq!(notes, ["See the source."]);
        let commented: String = back
            .blocks
            .iter()
            .filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None })
            .flat_map(|p| p.runs.iter())
            .filter_map(|r| if let Run::Text(t) = r { t.comment.map(|_| t.text.clone()) } else { None })
            .collect();
        assert_eq!(commented, "commentedstill commented");
        assert_eq!(back.comments.len(), 1);
        let c = &back.comments[0];
        assert_eq!((c.author.as_str(), c.text.as_str(), c.date.as_str()), ("Ken Doe", "Check this", "2026-10-05T10:00:00Z"));
        // Tracked changes keep who made them, and the switch.
        let tracked = Document {
            tracking: true,
            revisions: vec![Revision { id: 3, author: "Ann".into(), date: "2026-10-05T09:00:00Z".into() }],
            blocks: vec![para(vec![
                Run::text("Keep "),
                Run::styled("new", |r| r.inserted = Some(3)),
                Run::styled("old", |r| r.deleted = Some(3)),
            ])],
            ..Default::default()
        };
        let (bytes, _) = write(&tracked, &BTreeMap::new()).unwrap();
        let (back, _) = read(&bytes).unwrap();
        assert!(back.tracking);
        let Block::Paragraph(p) = &back.blocks[0] else { panic!() };
        let marks: Vec<(String, bool, bool)> = p
            .runs
            .iter()
            .filter_map(|r| if let Run::Text(t) = r { Some((t.text.clone(), t.inserted.is_some(), t.deleted.is_some())) } else { None })
            .collect();
        assert_eq!(marks, [("Keep ".into(), false, false), ("new".into(), true, false), ("old".into(), false, true)]);
        assert_eq!(back.revisions[0].author, "Ann");

        // Moved text: deleted where it was, added where it went.
        let moved = format!(
            r#"<w:document xmlns:w="{W_NS}"><w:body><w:p><w:moveFrom w:id="1" w:author="Ann"><w:r><w:t>Moved</w:t></w:r></w:moveFrom><w:r><w:t> stays </w:t></w:r><w:moveTo w:id="2" w:author="Ann"><w:r><w:t>Moved</w:t></w:r></w:moveTo></w:p></w:body></w:document>"#
        );
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        z.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(moved.as_bytes()).unwrap();
        let bytes = z.finish().unwrap().into_inner();
        let (back, _) = read(&bytes).unwrap();
        let Block::Paragraph(p) = &back.blocks[0] else { panic!() };
        let marks: Vec<(String, bool, bool)> = p
            .runs
            .iter()
            .filter_map(|r| if let Run::Text(t) = r { Some((t.text.clone(), t.inserted.is_some(), t.deleted.is_some())) } else { None })
            .collect();
        assert_eq!(marks, [("Moved".into(), false, true), (" stays ".into(), false, false), ("Moved".into(), true, false)]);

        // A line break inside a paragraph, and a tab.
        let broken = Document { blocks: vec![Block::Paragraph(Paragraph::text(Style::Normal, "Flat 2\u{2028}High Street\tLeeds"))], ..Default::default() };
        let (bytes, _) = write(&broken, &BTreeMap::new()).unwrap();
        let (back, _) = read(&bytes).unwrap();
        let Block::Paragraph(p) = &back.blocks[0] else { panic!() };
        assert_eq!(p.plain_text(), "Flat 2\u{2028}High Street\tLeeds");

        // Line and paragraph spacing.
        let spaced = Document {
            blocks: vec![
                Block::Paragraph(Paragraph {
                    line_spacing: Some(1.5),
                    space_before: Some(12.0),
                    space_after: Some(0.0),
                    runs: vec![Run::text("Spaced")],
                    ..Default::default()
                }),
                Block::Paragraph(Paragraph {
                    list: Some(List::Bullet),
                    line_spacing: Some(2.0),
                    runs: vec![Run::text("Listed")],
                    ..Default::default()
                }),
                Block::Paragraph(Paragraph::text(Style::H1, "Heading keeps its own")),
            ],
            ..Default::default()
        };
        let (bytes, _) = write(&spaced, &BTreeMap::new()).unwrap();
        let (back, _) = read(&bytes).unwrap();
        let spacing: Vec<(Option<f64>, Option<f64>, Option<f64>)> = back
            .blocks
            .iter()
            .filter_map(|b| if let Block::Paragraph(p) = b { Some((p.line_spacing, p.space_before, p.space_after)) } else { None })
            .collect();
        assert_eq!(spacing, [(Some(1.5), Some(12.0), Some(0.0)), (Some(2.0), None, None), (None, None, None)]);

        // Links, superscript and subscript.
        let url = "https://example.com/a?b=1&c='2'";
        let linked = Document {
            blocks: vec![para(vec![
                Run::text("H"),
                Run::styled("2", |r| r.subscript = true),
                Run::text("O and E = mc"),
                Run::styled("2", |r| r.superscript = true),
                Run::text(", see "),
                Run::styled("the site", |r| r.link = Some(url.into())),
                Run::styled(" again", |r| {
                    r.link = Some(url.into());
                    r.bold = true;
                }),
                Run::text("."),
            ])],
            ..Default::default()
        };
        let (bytes, _) = write(&linked, &BTreeMap::new()).unwrap();
        let (back, _) = read(&bytes).unwrap();
        let Block::Paragraph(p) = &back.blocks[0] else { panic!() };
        let runs: Vec<&TextRun> = p.runs.iter().filter_map(|r| if let Run::Text(t) = r { Some(t) } else { None }).collect();
        assert!(runs[1].subscript && runs[3].superscript);
        assert_eq!(runs[5].link.as_deref(), Some(url));
        assert_eq!(runs[6].link.as_deref(), Some(url));
        assert!(runs[7].link.is_none());

        // Shapes and charts in the text.
        let drawn = Document {
            blocks: vec![para(vec![
                Run::text("A star "),
                Run::Drawing(Drawing {
                    drawing: Figure::Shape { shape: crate::present::model::Shape::Star, fill: "#e0a43a".into() },
                    width: 80.0,
                    height: 80.0,
                }),
                Run::Drawing(Drawing { drawing: Figure::sample_chart(), width: 300.0, height: 180.0 }),
            ])],
            ..Default::default()
        };
        let (bytes, _) = write(&drawn, &BTreeMap::new()).unwrap();
        let (back, _) = read(&bytes).unwrap();
        assert_eq!(back.blocks, drawn.blocks);

        // Endnotes, and sections with their own columns.
        let sectioned = Document {
            blocks: vec![
                para(vec![Run::text("One"), Run::Note(Note { note: "Later.".into(), end: true })]),
                Block::Section(Section { columns: 2, new_page: false }),
                para(vec![Run::text("Two")]),
                Block::Section(Section { columns: 3, new_page: true }),
                para(vec![Run::text("Three")]),
            ],
            ..Default::default()
        };
        let (bytes, _) = write(&sectioned, &BTreeMap::new()).unwrap();
        let (back, _) = read(&bytes).unwrap();
        assert_eq!(back.blocks, sectioned.blocks);
        assert_eq!(back.page.columns, 1);

        // The cached contents had page numbers: the second heading is on page 2.
        let pages = super::super::layout::paginate(&doc, &BTreeMap::new());
        assert_eq!(pages.heading_pages().get(&5), Some(&2));
    }
}
