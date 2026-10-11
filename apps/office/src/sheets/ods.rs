//! OpenDocument spreadsheets (.ods): cells with their values, formulas (OpenFormula,
//! `[.A1]` references and `;` between arguments), cell styles and number styles,
//! column widths, row heights, hidden rows and columns, merges and notes.

use super::formula::{self, Value};
use super::model::*;
use super::xlsx::{Opened, num_fmt};
use crate::odf::{self, NUMBER, OFFICE, STYLE, Styles, TABLE, TEXT, attr, esc, pt};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fmt::Write as _;

/// Points per pixel on the grid.
const PX: f64 = 0.75;

/// Functions OpenFormula knows by Excel's newer names only with this prefix.
const MICROSOFT: [&str; 14] = [
    "TEXTJOIN", "CONCAT", "IFS", "SWITCH", "MAXIFS", "MINIFS", "XLOOKUP", "XMATCH", "UNIQUE", "FILTER", "SORT", "SEQUENCE",
    "LET", "SORTBY",
];

// ---------- Formulas ----------

/// A cell address with optional `$`s: `A1`, `$B$2`.
fn is_cell(s: &str) -> bool {
    formula::parse_cell(s).is_some() && !s.contains(['(', ' '])
}

/// Our formula (without its `=`) in OpenFormula.
pub fn to_odf(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::from("of:=");
    let mut i = 0;
    // A reference at `i`: an optional sheet, a cell, an optional `:cell`.
    let reference = |i: usize| -> Option<(String, usize)> {
        let mut j = i;
        let mut sheet = None;
        if chars.get(j) == Some(&'\'') {
            let mut name = String::new();
            j += 1;
            while j < chars.len() {
                if chars[j] == '\'' {
                    if chars.get(j + 1) == Some(&'\'') {
                        name.push('\'');
                        j += 2;
                        continue;
                    }
                    break;
                }
                name.push(chars[j]);
                j += 1;
            }
            if chars.get(j + 1) != Some(&'!') {
                return None;
            }
            sheet = Some(name);
            j += 2;
        } else {
            let k = (j..chars.len()).find(|&k| !(chars[k].is_alphanumeric() || chars[k] == '_')).unwrap_or(chars.len());
            if k > j && chars.get(k) == Some(&'!') {
                sheet = Some(chars[j..k].iter().collect());
                j = k + 1;
            }
        }
        let cell_at = |j: usize| {
            let k = (j..chars.len()).find(|&k| !(chars[k].is_ascii_alphanumeric() || chars[k] == '$')).unwrap_or(chars.len());
            let s: String = chars[j..k].iter().collect();
            is_cell(&s).then_some((s, k))
        };
        let (first, mut k) = cell_at(j)?;
        // A function named like a cell (LOG10) isn't a reference.
        if chars.get(k) == Some(&'(') {
            return None;
        }
        let mut second = None;
        if chars.get(k) == Some(&':')
            && let Some((s, e)) = cell_at(k + 1)
        {
            second = Some(s);
            k = e;
        }
        let sheet = sheet.map(|s| {
            if s.chars().all(|c| c.is_alphanumeric() || c == '_') { format!("${s}") } else { format!("$'{}'", s.replace('\'', "''")) }
        });
        let text = match second {
            Some(b) => format!("[{}.{first}:.{b}]", sheet.unwrap_or_default()),
            None => format!("[{}.{first}]", sheet.unwrap_or_default()),
        };
        Some((text, k))
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            out.push(c);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == '"' {
                    if chars.get(i + 1) == Some(&'"') {
                        out.push('"');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        let word_before = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_' || chars[i - 1] == '.');
        if !word_before && (c == '\'' || c == '$' || c.is_alphabetic()) {
            if let Some((text, next)) = reference(i) {
                out.push_str(&text);
                i = next;
                continue;
            }
            // A name: a function (maybe renamed) or a constant.
            let k = (i..chars.len()).find(|&k| !(chars[k].is_alphanumeric() || chars[k] == '_' || chars[k] == '.')).unwrap_or(chars.len());
            let word: String = chars[i..k].iter().collect();
            if chars.get(k) == Some(&'(') && MICROSOFT.contains(&word.to_ascii_uppercase().as_str()) {
                out.push_str("COM.MICROSOFT.");
            }
            out.push_str(&word);
            i = k.max(i + 1);
            continue;
        }
        out.push(if c == ',' { ';' } else { c });
        i += 1;
    }
    out
}

/// One part of a bracketed reference (`$'Sheet 1'.$A$1`, `.B2`): its sheet and cell.
fn ref_part(s: &str) -> (Option<String>, String) {
    let s = s.trim().trim_start_matches('$');
    if let Some(cell) = s.strip_prefix('.') {
        return (None, cell.to_string());
    }
    // The sheet runs to the last dot outside quotes.
    let mut quoted = false;
    let mut dot = None;
    for (i, c) in s.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            '.' if !quoted => dot = Some(i),
            _ => {}
        }
    }
    let Some(d) = dot else { return (None, s.to_string()) };
    let sheet = s[..d].trim_start_matches('$');
    let sheet = match sheet.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')) {
        Some(q) => q.replace("''", "'"),
        None => sheet.to_string(),
    };
    (Some(sheet), s[d + 1..].to_string())
}

/// An OpenFormula formula (`of:=SUM([.A1:.B2])`) in our syntax, `=` included.
pub fn from_odf(f: &str) -> String {
    let body = match f.find(":=") {
        Some(i) if !f[..i].contains(['(', '"', '[']) => &f[i + 2..],
        _ => f.trim_start_matches('='),
    };
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::from("=");
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                out.push(c);
                i += 1;
                while i < chars.len() {
                    out.push(chars[i]);
                    if chars[i] == '"' {
                        if chars.get(i + 1) == Some(&'"') {
                            out.push('"');
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            '[' => {
                let mut j = i + 1;
                let mut quoted = false;
                while j < chars.len() && (quoted || chars[j] != ']') {
                    if chars[j] == '\'' {
                        quoted = !quoted;
                    }
                    j += 1;
                }
                let inner: String = chars[i + 1..j.min(chars.len())].iter().collect();
                i = j + 1;
                if inner.contains("#REF") || inner.contains('#') && inner.contains("://") {
                    out.push_str("#REF!");
                    continue;
                }
                // Split on the colon outside quotes.
                let mut quoted = false;
                let mut split = None;
                for (k, ch) in inner.char_indices() {
                    match ch {
                        '\'' => quoted = !quoted,
                        ':' if !quoted => {
                            split = Some(k);
                            break;
                        }
                        _ => {}
                    }
                }
                let (a, b) = match split {
                    Some(k) => (ref_part(&inner[..k]), Some(ref_part(&inner[k + 1..]))),
                    None => (ref_part(&inner), None),
                };
                if let Some(sheet) = &a.0 {
                    out.push_str(&super::summary::sheet_ref(sheet));
                    out.push('!');
                }
                out.push_str(&a.1);
                if let Some(b) = b {
                    out.push(':');
                    out.push_str(&b.1);
                }
            }
            ';' => {
                out.push(',');
                i += 1;
            }
            c if c.is_alphabetic() || c == '_' => {
                let k = (i..chars.len()).find(|&k| !(chars[k].is_alphanumeric() || chars[k] == '_' || chars[k] == '.')).unwrap_or(chars.len());
                let word: String = chars[i..k].iter().collect();
                let upper = word.to_ascii_uppercase();
                let bare = ["COM.MICROSOFT.", "ORG.OPENOFFICE.", "ORG.LIBREOFFICE.", "_XLFN._XLWS.", "_XLFN."]
                    .iter()
                    .find_map(|p| upper.starts_with(p).then(|| &word[p.len()..]))
                    .unwrap_or(&word);
                out.push_str(bare);
                i = k;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

// ---------- Number styles ----------

/// An Excel-style format code for an ODF number style element.
fn style_code(n: &roxmltree::Node) -> String {
    let mut code = String::new();
    let long = |e: &roxmltree::Node| attr(e, NUMBER, "style") == Some("long");
    let mut after_hours = false;
    for e in n.children().filter(|c| c.is_element()) {
        match e.tag_name().name() {
            "number" => {
                let decimals: usize = attr(&e, NUMBER, "decimal-places").and_then(|v| v.parse().ok()).unwrap_or(0);
                let min: usize = attr(&e, NUMBER, "min-integer-digits").and_then(|v| v.parse().ok()).unwrap_or(1);
                let grouped = attr(&e, NUMBER, "grouping") == Some("true");
                let int = if grouped { format!("#,##{}", "0".repeat(min.clamp(1, 1))) } else { "0".repeat(min.max(1)) };
                code.push_str(&int);
                if decimals > 0 {
                    code.push('.');
                    code.push_str(&"0".repeat(decimals.min(10)));
                }
            }
            "scientific-number" => code.push_str("0.00E+00"),
            "fraction" => code.push_str("# ?/?"),
            "currency-symbol" => code.push_str(e.text().unwrap_or("$")),
            "text" => {
                let t = e.text().unwrap_or("");
                if t.chars().all(|c| " %-/:,.()".contains(c)) {
                    code.push_str(t);
                } else {
                    let _ = write!(code, "\"{}\"", t.replace('"', ""));
                }
            }
            "year" => code.push_str(if long(&e) { "yyyy" } else { "yy" }),
            "month" => {
                let textual = attr(&e, NUMBER, "textual") == Some("true");
                code.push_str(match (textual, long(&e)) {
                    (true, true) => "mmmm",
                    (true, false) => "mmm",
                    (false, true) => "mm",
                    (false, false) => "m",
                });
            }
            "day" => code.push_str(if long(&e) { "dd" } else { "d" }),
            "day-of-week" => code.push_str(if long(&e) { "dddd" } else { "ddd" }),
            "hours" => {
                code.push_str(if long(&e) { "hh" } else { "h" });
                after_hours = true;
            }
            "minutes" => code.push_str(if long(&e) || after_hours { "mm" } else { "m" }),
            "seconds" => code.push_str(if long(&e) { "ss" } else { "s" }),
            "am-pm" => code.push_str("AM/PM"),
            "boolean" => return "General".into(),
            "text-content" => code.push('@'),
            _ => {}
        }
    }
    if code.is_empty() { "General".into() } else { code }
}

/// Every number style in the parts: name → format code. A style that hands
/// positive values to another (`style:map`) takes that one's code.
fn number_styles(parts: &[&roxmltree::Document]) -> HashMap<String, String> {
    let mut codes = HashMap::new();
    let mut maps = Vec::new();
    for doc in parts {
        for n in doc.descendants().filter(|n| n.tag_name().namespace() == Some(NUMBER) && n.tag_name().name().ends_with("-style")) {
            let Some(name) = attr(&n, STYLE, "name") else { continue };
            codes.insert(name.to_string(), style_code(&n));
            if let Some(m) = n.children().find(|c| {
                c.has_tag_name((STYLE, "map")) && attr(c, STYLE, "condition").is_some_and(|k| k.contains(">=0") || k.contains(">0"))
            }) && let Some(target) = attr(&m, STYLE, "apply-style-name")
            {
                maps.push((name.to_string(), target.to_string()));
            }
        }
    }
    for (name, target) in maps {
        if let Some(code) = codes.get(&target).cloned() {
            codes.insert(name, code);
        }
    }
    codes
}

/// The ODF number style for a format code (its first section).
fn code_style(name: &str, code: &str) -> String {
    let code = code.split(';').next().unwrap_or("");
    let lower = code.to_lowercase();
    let bare: String = lower.split('"').step_by(2).collect::<String>().split(['[', ']']).step_by(2).collect();
    let is_date = bare.contains('y') || bare.contains('d') || (bare.contains('m') && !bare.contains('0') && !bare.contains('#'));
    let is_time = !is_date && (bare.contains('h') || bare.contains('s') && !bare.contains('0'));
    let currency = ['$', '€', '£', '¥'].into_iter().find(|c| code.contains(*c));
    let number = |code: &str| {
        let int_part = code.split('.').next().unwrap_or("");
        let decimals = code.split('.').nth(1).map(|d| d.chars().take_while(|c| *c == '0' || *c == '#').count()).unwrap_or(0);
        let grouping = int_part.contains(',');
        format!(
            r#"<number:number number:decimal-places="{decimals}" number:min-decimal-places="{decimals}" number:min-integer-digits="1"{}/>"#,
            if grouping { r#" number:grouping="true""# } else { "" }
        )
    };
    if is_date || is_time {
        let tag = if is_date { "date-style" } else { "time-style" };
        let mut out = format!(r#"<number:{tag} style:name="{name}">"#);
        let chars: Vec<char> = code.chars().collect();
        let mut i = 0;
        let mut after_hours = false;
        while i < chars.len() {
            let c = chars[i].to_ascii_lowercase();
            let run = chars[i..].iter().take_while(|x| x.to_ascii_lowercase() == c).count();
            let long = run >= 2;
            let elem = match c {
                'y' => Some(format!(r#"<number:year{}/>"#, if run >= 3 { r#" number:style="long""# } else { "" })),
                'm' if after_hours => Some(format!(r#"<number:minutes{}/>"#, if long { r#" number:style="long""# } else { "" })),
                'm' => Some(match run {
                    1 => "<number:month/>".to_string(),
                    2 => r#"<number:month number:style="long"/>"#.to_string(),
                    3 => r#"<number:month number:textual="true"/>"#.to_string(),
                    _ => r#"<number:month number:style="long" number:textual="true"/>"#.to_string(),
                }),
                'd' if run >= 3 => Some(format!(r#"<number:day-of-week{}/>"#, if run >= 4 { r#" number:style="long""# } else { "" })),
                'd' => Some(format!(r#"<number:day{}/>"#, if long { r#" number:style="long""# } else { "" })),
                'h' => {
                    after_hours = true;
                    Some(format!(r#"<number:hours{}/>"#, if long { r#" number:style="long""# } else { "" }))
                }
                's' => Some(format!(r#"<number:seconds{}/>"#, if long { r#" number:style="long""# } else { "" })),
                _ => None,
            };
            if let Some(e) = elem {
                out.push_str(&e);
                i += run;
                continue;
            }
            if code[chars[..i].iter().map(|c| c.len_utf8()).sum::<usize>()..].to_ascii_uppercase().starts_with("AM/PM") {
                out.push_str("<number:am-pm/>");
                i += 5;
                continue;
            }
            let ch = chars[i];
            if ch != '"' && ch != '\\' {
                let _ = write!(out, "<number:text>{}</number:text>", esc(&ch.to_string()));
            }
            i += 1;
        }
        let _ = write!(out, "</number:{tag}>");
        return out;
    }
    if bare.contains('%') {
        return format!(r#"<number:percentage-style style:name="{name}">{}<number:text>%</number:text></number:percentage-style>"#, number(code));
    }
    if let Some(sym) = currency {
        return format!(
            r#"<number:currency-style style:name="{name}"><number:currency-symbol>{}</number:currency-symbol>{}</number:currency-style>"#,
            esc(&sym.to_string()),
            number(&code.replace(sym, ""))
        );
    }
    if code.eq_ignore_ascii_case("general") || code.is_empty() {
        return format!(r#"<number:number-style style:name="{name}"><number:number number:min-integer-digits="1"/></number:number-style>"#);
    }
    format!(r#"<number:number-style style:name="{name}">{}</number:number-style>"#, number(code))
}

/// The format code for one of our formats, or None for General.
fn format_code(f: &Format) -> Option<String> {
    if let Some(p) = &f.pattern {
        return Some(p.clone());
    }
    Some(
        match f.number {
            NumFmt::General => return None,
            NumFmt::Decimal => "#,##0.00",
            NumFmt::Currency => "$#,##0.00",
            NumFmt::Percent => "0.0%",
            NumFmt::Date => "yyyy-mm-dd",
        }
        .to_string(),
    )
}

// ---------- Writing ----------

#[derive(Default)]
struct Names {
    /// Format (without the number part) and data style → cell style name.
    cells: Vec<(String, String)>,
    data: Vec<(String, String)>,
    columns: Vec<(String, String)>,
    rows: Vec<(String, String)>,
    xml: String,
}

impl Names {
    fn data_style(&mut self, code: &str) -> String {
        if let Some((_, n)) = self.data.iter().find(|(c, _)| c == code) {
            return n.clone();
        }
        let name = format!("N{}", self.data.len() + 1);
        self.xml.push_str(&code_style(&name, code));
        self.data.push((code.to_string(), name.clone()));
        name
    }

    fn cell(&mut self, f: &Format) -> Option<String> {
        let mut text = String::new();
        let mut cell = String::new();
        let mut para = String::new();
        if f.bold {
            text.push_str(r#" fo:font-weight="bold""#);
        }
        if f.italic {
            text.push_str(r#" fo:font-style="italic""#);
        }
        if let Some(c) = &f.color {
            let _ = write!(text, r#" fo:color="{}""#, esc(c));
        }
        if let Some(c) = &f.fill {
            let _ = write!(cell, r#" fo:background-color="{}""#, esc(c));
        }
        if f.wrap {
            cell.push_str(r#" fo:wrap-option="wrap""#);
        }
        for (bit, side) in [(TOP, "top"), (BOTTOM, "bottom"), (LEFT, "left"), (RIGHT, "right")] {
            if f.borders & bit != 0 {
                let _ = write!(cell, r#" fo:border-{side}="0.75pt solid #000000""#);
            }
        }
        if let Some(a) = f.align {
            cell.push_str(r#" style:text-align-source="fix""#);
            let _ = write!(
                para,
                r#" fo:text-align="{}""#,
                match a {
                    HAlign::Left => "start",
                    HAlign::Center => "center",
                    HAlign::Right => "end",
                }
            );
        }
        let data = format_code(f).map(|c| self.data_style(&c));
        if text.is_empty() && cell.is_empty() && para.is_empty() && data.is_none() {
            return None;
        }
        let key = format!("{text}|{cell}|{para}|{data:?}");
        if let Some((_, n)) = self.cells.iter().find(|(k, _)| *k == key) {
            return Some(n.clone());
        }
        let name = format!("ce{}", self.cells.len() + 1);
        let _ = write!(
            self.xml,
            r#"<style:style style:name="{name}" style:family="table-cell" style:parent-style-name="Default"{}>"#,
            data.map(|d| format!(r#" style:data-style-name="{d}""#)).unwrap_or_default()
        );
        if !cell.is_empty() {
            let _ = write!(self.xml, "<style:table-cell-properties{cell}/>");
        }
        if !para.is_empty() {
            let _ = write!(self.xml, "<style:paragraph-properties{para}/>");
        }
        if !text.is_empty() {
            let _ = write!(self.xml, "<style:text-properties{text}/>");
        }
        self.xml.push_str("</style:style>");
        self.cells.push((key, name.clone()));
        Some(name)
    }

    fn sized(list: &mut Vec<(String, String)>, xml: &mut String, prefix: &str, family: &str, props: String) -> String {
        if let Some((_, n)) = list.iter().find(|(p, _)| *p == props) {
            return n.clone();
        }
        let name = format!("{prefix}{}", list.len() + 1);
        let _ = write!(xml, r#"<style:style style:name="{name}" style:family="{family}">{props}</style:style>"#);
        list.push((props, name.clone()));
        name
    }

    fn column(&mut self, px: u32) -> String {
        let props = format!(r#"<style:table-column-properties fo:break-before="auto" style:column-width="{}"/>"#, pt(px as f64 * PX));
        Names::sized(&mut self.columns, &mut self.xml, "co", "table-column", props)
    }

    fn row(&mut self, px: u32) -> String {
        let props = format!(
            r#"<style:table-row-properties style:row-height="{}" fo:break-before="auto" style:use-optimal-row-height="false"/>"#,
            pt(px as f64 * PX)
        );
        Names::sized(&mut self.rows, &mut self.xml, "ro", "table-row", props)
    }
}

/// A date serial as ODF's date value.
fn date_value(serial: f64) -> String {
    let (y, m, d) = formula::serial_date(serial);
    let secs = ((serial - serial.floor()) * 86400.0).round() as i64;
    if secs == 0 {
        format!("{y:04}-{m:02}-{d:02}")
    } else {
        format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
    }
}

fn is_dateish(f: &Format) -> bool {
    f.number == NumFmt::Date
        || f.pattern.as_deref().is_some_and(|p| {
            let bare: String = p.to_lowercase().split('"').step_by(2).collect();
            bare.contains('y') || bare.contains('d')
        })
}

pub fn write(wb: &Workbook, calc: &Calc) -> Result<(Vec<u8>, Vec<String>)> {
    let mut names = Names::default();
    let mut lost: Vec<String> = Vec::new();
    let note = |what: &str, lost: &mut Vec<String>| {
        if !lost.iter().any(|l| l == what) {
            lost.push(what.to_string());
        }
    };
    let mut body = String::new();
    for (si, s) in wb.sheets.iter().enumerate() {
        if s.chart.is_some() || !s.charts.is_empty() {
            note("charts", &mut lost);
        }
        if !s.rules.is_empty() {
            note("highlight rules", &mut lost);
        }
        if !s.lists.is_empty() {
            note("dropdown lists", &mut lost);
        }
        if s.filter.is_some() {
            note("the filter (every row is shown)", &mut lost);
        }
        if !s.tables.is_empty() {
            note("table looks (the cells stay)", &mut lost);
        }
        if s.freeze_rows > 0 || s.freeze_cols > 0 {
            note("frozen rows and columns", &mut lost);
        }
        if s.cells.values().any(|c| c.format.checkbox) {
            note("checkboxes (they show as TRUE and FALSE)", &mut lost);
        }
        let _ = write!(body, r#"<table:table table:name="{}" table:style-name="ta1">"#, esc(&s.name));
        // Columns, alike ones together.
        let mut c = 0;
        while c < COLS {
            let key = |c: u32| (s.size_of_col(c), s.hidden_cols.contains(&c));
            let k = key(c);
            let mut n = 1;
            while c + n < COLS && key(c + n) == k {
                n += 1;
            }
            let style = names.column(k.0);
            let _ = write!(
                body,
                r#"<table:table-column table:style-name="{style}"{}{} table:default-cell-style-name="Default"/>"#,
                if n > 1 { format!(r#" table:number-columns-repeated="{n}""#) } else { String::new() },
                if k.1 { r#" table:visibility="collapse""# } else { "" }
            );
            c += n;
        }
        let last_row = s
            .cells
            .keys()
            .map(|(r, _)| *r)
            .chain(s.row_heights.keys().copied())
            .chain(s.hidden_rows.iter().copied())
            .chain(s.merges.iter().map(|m| m.bottom))
            .max();
        let covered = |r: u32, c: u32| s.merges.iter().any(|m| m.contains(r, c) && (r, c) != (m.top, m.left));
        let mut r = 0;
        while let Some(last) = last_row
            && r <= last
        {
            let row_cells: Vec<u32> = s.cells.range((r, 0)..(r + 1, 0)).map(|((_, c), _)| *c).collect();
            let row_merged = s.merges.iter().any(|m| m.top <= r && r <= m.bottom);
            let height = s.size_of_row(r);
            let hidden = s.hidden_rows.contains(&r);
            // Empty rows alike in a row go as one.
            let mut repeat = 1;
            if row_cells.is_empty() && !row_merged {
                while r + repeat <= last
                    && s.cells.range((r + repeat, 0)..(r + repeat + 1, 0)).next().is_none()
                    && !s.merges.iter().any(|m| m.top <= r + repeat && r + repeat <= m.bottom)
                    && s.size_of_row(r + repeat) == height
                    && s.hidden_rows.contains(&(r + repeat)) == hidden
                {
                    repeat += 1;
                }
            }
            let style = names.row(height);
            let _ = write!(
                body,
                r#"<table:table-row table:style-name="{style}"{}{}>"#,
                if repeat > 1 { format!(r#" table:number-rows-repeated="{repeat}""#) } else { String::new() },
                if hidden { r#" table:visibility="collapse""# } else { "" }
            );
            let end = row_cells.iter().copied().chain(s.merges.iter().filter(|m| m.top <= r && r <= m.bottom).map(|m| m.right)).max();
            let mut c = 0;
            let mut blank = 0;
            let flush = |body: &mut String, blank: &mut u32| {
                if *blank > 0 {
                    let rep = if *blank > 1 { format!(r#" table:number-columns-repeated="{blank}""#) } else { String::new() };
                    let _ = write!(body, "<table:table-cell{rep}/>");
                    *blank = 0;
                }
            };
            while let Some(end) = end
                && c <= end
            {
                if covered(r, c) {
                    flush(&mut body, &mut blank);
                    body.push_str("<table:covered-table-cell/>");
                    c += 1;
                    continue;
                }
                let Some(cell) = s.cells.get(&(r, c)) else {
                    blank += 1;
                    c += 1;
                    continue;
                };
                let merge = s.merges.iter().find(|m| (m.top, m.left) == (r, c));
                let style = names.cell(&cell.format);
                if cell.raw.is_empty() && style.is_none() && cell.comment.is_none() && merge.is_none() {
                    blank += 1;
                    c += 1;
                    continue;
                }
                flush(&mut body, &mut blank);
                let mut attrs = String::new();
                if let Some(st) = &style {
                    let _ = write!(attrs, r#" table:style-name="{st}""#);
                }
                if let Some(m) = merge {
                    let _ = write!(
                        attrs,
                        r#" table:number-columns-spanned="{}" table:number-rows-spanned="{}""#,
                        m.right - m.left + 1,
                        m.bottom - m.top + 1
                    );
                }
                let raw = cell.raw.as_str();
                let value = if raw.starts_with('=') { calc.value(wb, si, r, c) } else { literal(raw) };
                if let Some(src) = raw.strip_prefix('=') {
                    let _ = write!(attrs, r#" table:formula="{}""#, esc(&to_odf(src)));
                }
                let typed_date = !raw.starts_with('=') && cell.format.number == NumFmt::General && cell.format.pattern.is_none() && formula::parse_date(raw).is_some();
                match &value {
                    Value::Num(n) if typed_date || is_dateish(&cell.format) => {
                        let _ = write!(attrs, r#" office:value-type="date" office:date-value="{}""#, date_value(*n));
                    }
                    Value::Num(n) if cell.format.number == NumFmt::Percent => {
                        let _ = write!(attrs, r#" office:value-type="percentage" office:value="{n}""#);
                    }
                    Value::Num(n) if cell.format.number == NumFmt::Currency => {
                        let _ = write!(attrs, r#" office:value-type="currency" office:currency="USD" office:value="{n}""#);
                    }
                    Value::Num(n) => {
                        let _ = write!(attrs, r#" office:value-type="float" office:value="{n}""#);
                    }
                    Value::Bool(b) => {
                        let _ = write!(attrs, r#" office:value-type="boolean" office:boolean-value="{b}""#);
                    }
                    Value::Text(t) if raw.starts_with('=') => {
                        let _ = write!(attrs, r#" office:value-type="string" office:string-value="{}""#, esc(t));
                    }
                    Value::Text(_) => attrs.push_str(r#" office:value-type="string""#),
                    Value::Err(_) | Value::Empty => {}
                }
                let shown = if typed_date { raw.to_string() } else { display(&value, raw, &cell.format).0 };
                let _ = write!(body, "<table:table-cell{attrs}>");
                if let Some(text) = &cell.comment {
                    let _ = write!(
                        body,
                        "<office:annotation><dc:creator>Nexus Sheets</dc:creator>{}</office:annotation>",
                        text.split('\n').map(|l| format!("<text:p>{}</text:p>", odf::text_xml(l))).collect::<String>()
                    );
                }
                for line in shown.split('\n').filter(|_| !shown.is_empty()) {
                    let _ = write!(body, "<text:p>{}</text:p>", odf::text_xml(line));
                }
                body.push_str("</table:table-cell>");
                c += merge.map_or(1, |m| m.right - m.left + 1);
            }
            flush(&mut body, &mut blank);
            if end.is_none() {
                body.push_str("<table:table-cell/>");
            }
            body.push_str("</table:table-row>");
            r += repeat;
        }
        if last_row.is_none() {
            let style = names.row(ROW_H);
            let _ = write!(body, r#"<table:table-row table:style-name="{style}"><table:table-cell/></table:table-row>"#);
        }
        body.push_str("</table:table>");
    }
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content {ns}><office:font-face-decls><style:font-face style:name="Noto Sans" svg:font-family="'Noto Sans'"/></office:font-face-decls><office:automatic-styles><style:style style:name="ta1" style:family="table" style:master-page-name="Default"><style:table-properties table:display="true" style:writing-mode="lr-tb"/></style:style>{styles}</office:automatic-styles><office:body><office:spreadsheet>{body}</office:spreadsheet></office:body></office:document-content>"#,
        ns = odf::NAMESPACES,
        styles = names.xml,
    );
    let styles = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-styles {ns}><office:font-face-decls><style:font-face style:name="Noto Sans" svg:font-family="'Noto Sans'"/></office:font-face-decls><office:styles><style:default-style style:family="table-cell"><style:paragraph-properties style:tab-stop-distance="36pt"/><style:text-properties style:font-name="Noto Sans" fo:font-size="10pt"/></style:default-style><style:style style:name="Default" style:family="table-cell"/></office:styles><office:automatic-styles><style:page-layout style:name="pm1"><style:page-layout-properties style:writing-mode="lr-tb"/></style:page-layout></office:automatic-styles><office:master-styles><style:master-page style:name="Default" style:page-layout-name="pm1"/></office:master-styles></office:document-styles>"#,
        ns = odf::NAMESPACES
    );
    let bytes = odf::package(odf::ODS, &content, &styles, &[])?;
    Ok((bytes, lost))
}

// ---------- Reading ----------

/// A cell style as our format.
fn cell_format(styles: &Styles, codes: &HashMap<String, String>, name: &str) -> Format {
    let mut f = Format::default();
    let g = |k: &str| styles.get("table-cell", name, k);
    f.bold = g("text/font-weight").is_some_and(|v| v == "bold" || v.parse::<u32>().is_ok_and(|w| w >= 600));
    f.italic = g("text/font-style").is_some_and(|v| v == "italic" || v == "oblique");
    f.color = g("text/color").and_then(odf::color).filter(|c| c != "#000000");
    f.fill = g("cell/background-color").and_then(odf::color);
    f.wrap = g("cell/wrap-option") == Some("wrap");
    f.align = match g("paragraph/text-align") {
        Some("center") => Some(HAlign::Center),
        Some("end" | "right") => Some(HAlign::Right),
        Some("start" | "left") if g("cell/text-align-source") != Some("value-type") => Some(HAlign::Left),
        _ => None,
    };
    let line = |v: Option<&str>| v.is_some_and(|v| v != "none" && !v.starts_with("0pt") && !v.starts_with("0cm") && !v.starts_with("0in"));
    let all = line(g("cell/border"));
    for (bit, side) in [(TOP, "top"), (BOTTOM, "bottom"), (LEFT, "left"), (RIGHT, "right")] {
        if all || line(g(&format!("cell/border-{side}"))) {
            f.borders |= bit;
        }
    }
    let data = styles.lineage("table-cell", name).into_iter().find_map(|n| styles.data_style("table-cell", &n).map(String::from));
    if let Some(code) = data.and_then(|d| codes.get(&d)) {
        (f.number, f.pattern) = num_fmt(u32::MAX, Some(code));
    }
    f
}

/// ODF's date value (`2026-01-05`, `2026-01-05T10:30:00`) as a serial.
fn date_serial(v: &str) -> Option<f64> {
    let (date, time) = v.split_once('T').unwrap_or((v, ""));
    let mut parts = date.split('-').map(|p| p.parse::<i64>());
    let (y, m, d) = (parts.next()?.ok()?, parts.next()?.ok()?, parts.next()?.ok()?);
    let mut serial = formula::date_serial(y, m, d);
    let mut t = time.split(':').map(|p| p.trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.').parse::<f64>().unwrap_or(0.0));
    let (h, mi, s) = (t.next().unwrap_or(0.0), t.next().unwrap_or(0.0), t.next().unwrap_or(0.0));
    serial += (h * 3600.0 + mi * 60.0 + s) / 86400.0;
    Some(serial)
}

/// ODF's time value (`PT10H30M00S`) as a fraction of a day.
fn time_fraction(v: &str) -> Option<f64> {
    let t = v.strip_prefix("PT").or_else(|| v.strip_prefix("-PT"))?;
    let mut total = 0.0;
    let mut num = String::new();
    for c in t.chars() {
        match c {
            'H' => total += num.parse::<f64>().ok()? * 3600.0,
            'M' => total += num.parse::<f64>().ok()? * 60.0,
            'S' => total += num.parse::<f64>().ok()?,
            _ => {
                num.push(c);
                continue;
            }
        }
        num.clear();
    }
    Some(total / 86400.0)
}

/// Rows and columns a merged cell spans.
type Span = (u32, u32);

/// The text of a cell's paragraphs (not its note's).
fn cell_text(cell: &roxmltree::Node) -> String {
    cell.children()
        .filter(|c| c.has_tag_name((TEXT, "p")) || c.has_tag_name((TEXT, "h")))
        .map(|p| odf::plain_text(&p))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn read(bytes: &[u8]) -> Result<Opened> {
    let mut pkg = odf::Package::open(bytes, odf::ODS, "spreadsheet")?;
    let content = pkg.content()?;
    let styles_text = pkg.text("styles.xml").unwrap_or_default();
    let cdoc = roxmltree::Document::parse(&content).context("the spreadsheet is damaged")?;
    let sdoc = roxmltree::Document::parse(&styles_text).ok();
    let mut parts = Vec::new();
    if let Some(s) = &sdoc {
        parts.push(s);
    }
    parts.push(&cdoc);
    let styles = Styles::read(&parts);
    let codes = number_styles(&parts);
    let mut formats: HashMap<String, Format> = HashMap::new();
    let mut format_of = |name: &str| formats.entry(name.to_string()).or_insert_with(|| cell_format(&styles, &codes, name)).clone();

    let body = cdoc.descendants().find(|n| n.has_tag_name((OFFICE, "spreadsheet"))).context("it isn't a spreadsheet")?;
    let mut sheets = Vec::new();
    let mut truncated = false;
    for table in body.children().filter(|n| n.has_tag_name((TABLE, "table"))) {
        if sheets.len() >= MAX_SHEETS {
            break;
        }
        let name = attr(&table, TABLE, "name").unwrap_or("Sheet").to_string();
        let mut sheet = Sheet::new(&name);
        let mine = |n: &roxmltree::Node| n.ancestors().find(|a| a.has_tag_name((TABLE, "table"))) == Some(table);
        // Columns: widths, hidden, and the style a column's cells default to.
        let mut col_styles: Vec<Option<String>> = Vec::new();
        let mut col = 0u32;
        for c in table.descendants().filter(|n| n.has_tag_name((TABLE, "table-column")) && mine(n)) {
            let repeat: u32 = attr(&c, TABLE, "number-columns-repeated").and_then(|v| v.parse().ok()).unwrap_or(1);
            let width = attr(&c, TABLE, "style-name")
                .and_then(|s| styles.get("table-column", s, "column/column-width"))
                .and_then(odf::length)
                .map(|w| (w / PX).round() as u32);
            let hidden = matches!(attr(&c, TABLE, "visibility"), Some("collapse" | "filter"));
            let default = attr(&c, TABLE, "default-cell-style-name").filter(|s| *s != "Default").map(String::from);
            for _ in 0..repeat.min(COLS.saturating_sub(col)) {
                if let Some(w) = width.filter(|w| (*w as i64 - COL_W as i64).abs() > 1) {
                    sheet.col_widths.insert(col, w.clamp(8, 2000));
                }
                if hidden {
                    sheet.hidden_cols.insert(col);
                }
                col_styles.push(default.clone());
                col += 1;
            }
        }
        let mut r = 0u32;
        for row in table.descendants().filter(|n| n.has_tag_name((TABLE, "table-row")) && mine(n)) {
            if r >= ROWS {
                if row.children().any(|c| !cell_text(&c).is_empty()) {
                    truncated = true;
                }
                break;
            }
            let repeat: u32 = attr(&row, TABLE, "number-rows-repeated").and_then(|v| v.parse().ok()).unwrap_or(1);
            let row_style = attr(&row, TABLE, "style-name");
            let height = row_style
                .filter(|s| styles.get("table-row", s, "row/use-optimal-row-height") != Some("true"))
                .and_then(|s| styles.get("table-row", s, "row/row-height"))
                .and_then(odf::length)
                .map(|h| (h / PX).round() as u32)
                .filter(|h| (*h as i64 - ROW_H as i64).abs() > 1);
            let hidden = matches!(attr(&row, TABLE, "visibility"), Some("collapse" | "filter"));
            let row_default = attr(&row, TABLE, "default-cell-style-name").map(String::from);
            // The row's cells once; repeated rows copy them (only while they hold something).
            let mut cells: Vec<(u32, Cell, Option<Span>)> = Vec::new();
            let mut c = 0u32;
            for cell in row.children().filter(|n| n.has_tag_name((TABLE, "table-cell")) || n.has_tag_name((TABLE, "covered-table-cell"))) {
                let k: u32 = attr(&cell, TABLE, "number-columns-repeated").and_then(|v| v.parse().ok()).unwrap_or(1).max(1);
                if cell.has_tag_name((TABLE, "covered-table-cell")) {
                    c = c.saturating_add(k);
                    continue;
                }
                let style = attr(&cell, TABLE, "style-name")
                    .map(String::from)
                    .or_else(|| row_default.clone())
                    .or_else(|| col_styles.get(c as usize).cloned().flatten());
                let format = style.as_deref().filter(|s| *s != "Default").map(&mut format_of).unwrap_or_default();
                let mut raw = match attr(&cell, OFFICE, "value-type") {
                    Some("float" | "percentage" | "currency") => {
                        attr(&cell, OFFICE, "value").and_then(|v| v.parse::<f64>().ok()).map(formula::general).unwrap_or_default()
                    }
                    Some("date") => attr(&cell, OFFICE, "date-value").and_then(date_serial).map(formula::general).unwrap_or_default(),
                    Some("time") => attr(&cell, OFFICE, "time-value").and_then(time_fraction).map(formula::general).unwrap_or_default(),
                    Some("boolean") => {
                        if attr(&cell, OFFICE, "boolean-value") == Some("true") { "TRUE" } else { "FALSE" }.into()
                    }
                    _ => cell_text(&cell),
                };
                let mut format = format;
                if attr(&cell, OFFICE, "value-type") == Some("date") && format.number == NumFmt::General && format.pattern.is_none() {
                    format.number = NumFmt::Date;
                }
                if let Some(f) = attr(&cell, TABLE, "formula") {
                    raw = from_odf(f);
                }
                let comment = cell
                    .children()
                    .find(|n| n.has_tag_name((OFFICE, "annotation")))
                    .map(|a| {
                        a.children()
                            .filter(|p| p.has_tag_name((TEXT, "p")))
                            .map(|p| odf::plain_text(&p))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .filter(|t| !t.trim().is_empty());
                let span = (
                    attr(&cell, TABLE, "number-rows-spanned").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1),
                    attr(&cell, TABLE, "number-columns-spanned").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1),
                );
                let merge = (span.0 > 1 || span.1 > 1).then_some(span);
                let blank = raw.is_empty() && format == Format::default() && comment.is_none() && merge.is_none();
                if !blank {
                    // A styled cell repeated across the whole sheet only needs the columns we have.
                    for i in 0..k {
                        if c + i >= COLS {
                            if !raw.is_empty() {
                                truncated = true;
                            }
                            break;
                        }
                        cells.push((c + i, Cell { raw: raw.clone(), format: format.clone(), comment: comment.clone() }, merge));
                    }
                }
                c = c.saturating_add(k);
            }
            let rows_here = if cells.is_empty() { repeat } else { repeat.min(ROWS - r) };
            if cells.is_empty() {
                // Plain rows (or a long run of them at the end) only carry their size.
                if repeat < 10_000 {
                    for i in 0..repeat.min(ROWS.saturating_sub(r)) {
                        if let Some(h) = height {
                            sheet.row_heights.insert(r + i, h.clamp(4, 2000));
                        }
                        if hidden {
                            sheet.hidden_rows.insert(r + i);
                        }
                    }
                }
            } else {
                for i in 0..rows_here {
                    let rr = r + i;
                    if let Some(h) = height {
                        sheet.row_heights.insert(rr, h.clamp(4, 2000));
                    }
                    if hidden {
                        sheet.hidden_rows.insert(rr);
                    }
                    for (cc, cell, merge) in &cells {
                        if let Some((rs, cs)) = merge {
                            let bottom = (rr + rs - 1).min(ROWS - 1);
                            let right = (cc + cs - 1).min(COLS - 1);
                            sheet.merges.push(Area::new(rr, *cc, bottom, right));
                        }
                        sheet.cells.insert((rr, *cc), cell.clone());
                    }
                }
            }
            r = r.saturating_add(rows_here);
        }
        sheets.push(sheet);
    }
    if sheets.is_empty() {
        sheets.push(Sheet::new("Sheet 1"));
    }
    Ok(Opened { workbook: Workbook { sheets, active: 0 }, truncated, dropped_rules: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas_translate_both_ways() {
        for (ours, odf) in [
            ("SUM(A1:B2)", "of:=SUM([.A1:.B2])"),
            ("IF(A1>0,\"a,b\",$C$3)", "of:=IF([.A1]>0;\"a,b\";[.$C$3])"),
            ("'Sheet 1'!B4*2", "of:=[$'Sheet 1'.B4]*2"),
            ("SUM(Data!A1:A9)", "of:=SUM([$Data.A1:.A9])"),
            ("TEXTJOIN(\",\",TRUE,A2:A3)", "of:=COM.MICROSOFT.TEXTJOIN(\",\";TRUE;[.A2:.A3])"),
            ("LOG10(100)+A1", "of:=LOG10(100)+[.A1]"),
        ] {
            assert_eq!(to_odf(ours), odf, "{ours}");
            assert_eq!(from_odf(odf), format!("={ours}"), "{odf}");
        }
        // LibreOffice repeats the sheet on both ends of a range.
        assert_eq!(from_odf("of:=SUM([$Sheet2.A1:$Sheet2.B2])"), "=SUM(Sheet2!A1:B2)");
        assert_eq!(from_odf("of:=[.#REF!]+1"), "=#REF!+1");
    }

    #[test]
    fn number_styles_round_trip() {
        for code in ["#,##0.00", "0.0%", "$#,##0.00", "yyyy-mm-dd", "d mmm yyyy", "0.000", "h:mm"] {
            let xml = format!(r#"<office:styles {}>{}</office:styles>"#, odf::NAMESPACES, code_style("N1", code));
            let doc = roxmltree::Document::parse(&xml).unwrap();
            let codes = number_styles(&[&doc]);
            assert_eq!(codes["N1"], code);
        }
    }

    #[test]
    fn round_trips_through_opendocument() {
        let mut wb = Workbook::default();
        wb.sheets.push(Sheet::new("Data 2"));
        let s = &mut wb.sheets[0];
        let put = |s: &mut Sheet, r, c, raw: &str, f: Format| {
            s.cells.insert((r, c), Cell { raw: raw.into(), format: f, comment: None });
        };
        put(s, 0, 0, "Item", Format { bold: true, fill: Some("#ffeeaa".into()), borders: TOP | BOTTOM, ..Default::default() });
        put(s, 1, 0, "Rent", Format::default());
        put(s, 1, 1, "1200", Format { number: NumFmt::Currency, ..Default::default() });
        put(s, 2, 1, "0.25", Format { number: NumFmt::Percent, align: Some(HAlign::Center), ..Default::default() });
        put(s, 3, 1, "=SUM(B2:B3)", Format { number: NumFmt::Decimal, ..Default::default() });
        put(s, 4, 0, "TRUE", Format::default());
        put(s, 5, 0, "46000", Format { number: NumFmt::Date, ..Default::default() });
        put(s, 6, 0, "Merged", Format { wrap: true, italic: true, color: Some("#cc0000".into()), ..Default::default() });
        put(s, 7, 3, "far", Format { pattern: Some("0.000".into()), ..Default::default() });
        s.cells.get_mut(&(1, 0)).unwrap().comment = Some("A note".into());
        s.merges.push(Area::new(6, 0, 6, 2));
        s.col_widths.insert(0, 200);
        s.row_heights.insert(2, 50);
        s.hidden_cols.insert(4);
        s.hidden_rows.insert(9);
        let s2 = &mut wb.sheets[1];
        put(s2, 0, 0, "='Sheet 1'!B4*2", Format::default());
        let calc = Calc::new(&wb, 46000.0);
        let (bytes, lost) = write(&wb, &calc).unwrap();
        assert!(lost.is_empty(), "{lost:?}");
        let back = read(&bytes).unwrap().workbook;
        assert_eq!(back.sheets.len(), 2);
        assert_eq!(back.sheets[1].name, "Data 2");
        assert_eq!(back.sheets[1].raw(0, 0), "='Sheet 1'!B4*2");
        let (a, b) = (&wb.sheets[0], &back.sheets[0]);
        assert_eq!(b.cells, a.cells);
        assert_eq!(b.merges, a.merges);
        assert_eq!(b.col_widths, a.col_widths);
        assert_eq!(b.row_heights, a.row_heights);
        assert_eq!(b.hidden_cols, a.hidden_cols);
        assert_eq!(b.hidden_rows, a.hidden_rows);
    }

    #[test]
    fn reads_a_file_collabora_saved() {
        let wb = read(include_bytes!("../../testdata/collabora/sample.ods")).unwrap().workbook;
        let s = &wb.sheets[0];
        assert_eq!(s.raw(9, 1), "=SUM(B2:B9)");
        assert_eq!(s.raw(1, 3), "=B2-C2");
        assert_eq!(s.format(1, 1).number, NumFmt::Currency);
        assert!(s.format(0, 0).bold && s.format(0, 0).fill.is_some());
        assert_eq!(s.col_widths.get(&0), Some(&180));
    }
}
