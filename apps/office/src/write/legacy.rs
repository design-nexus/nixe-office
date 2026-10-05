//! Documents saved by the Qt version of Nexus Write: a ZIP of `manifest.json`,
//! `document.html` (Qt rich text) and `media/*.png`. They open as before and
//! save in the current format.

use super::model::*;
use crate::container;
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::io::Cursor;

pub fn read(bytes: &[u8]) -> Result<(Document, BTreeMap<String, Vec<u8>>)> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("not a Nexus Write file")?;
    let html = container::read_entry(&mut archive, "document.html")?.context("not a Nexus Write file")?;
    let manifest: serde_json::Value =
        container::read_entry(&mut archive, "manifest.json")?.and_then(|m| serde_json::from_slice(&m).ok()).unwrap_or_default();
    if manifest.get("format").and_then(|v| v.as_str()) != Some("nexus-write") {
        bail!("not a Nexus Write file");
    }
    let mut media = BTreeMap::new();
    let names: Vec<String> =
        archive.file_names().filter(|n| n.starts_with("media/") && !n.ends_with('/')).map(String::from).collect();
    for n in names {
        if let Some(b) = container::read_entry(&mut archive, &n)? {
            media.insert(n.trim_start_matches("media/").to_string(), b);
        }
    }
    let mut doc = from_html(&String::from_utf8_lossy(&html));
    if let Some(page) = manifest.get("page") {
        let m = |k: &str| page.get(k).and_then(|v| v.as_f64()).unwrap_or(54.0).clamp(36.0, 144.0);
        doc.page.margins = [m("top"), m("bottom"), m("left"), m("right")];
        doc.page.header = page.get("header").and_then(|v| v.as_str()).unwrap_or("").chars().take(120).collect();
        doc.page.footer = page.get("footer").and_then(|v| v.as_str()).unwrap_or("").chars().take(120).collect();
        doc.page.page_numbers = page.get("pageNumbers").and_then(|v| v.as_bool()).unwrap_or(false);
    }
    // Qt's pages were letter-sized.
    doc.page.paper = "letter".into();
    Ok((doc, media))
}

#[derive(Clone, Default)]
struct Fmt {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    font: Option<String>,
    size: Option<f64>,
    color: Option<String>,
    highlight: Option<String>,
}

enum Tok {
    Open(String, Vec<(String, String)>, bool),
    Close(String),
    Text(String),
}

fn decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..end];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ if ent.starts_with("#x") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32),
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn tokenize(html: &str) -> Vec<Tok> {
    let mut toks = Vec::new();
    let mut rest = html;
    while !rest.is_empty() {
        match rest.find('<') {
            Some(0) => {
                if rest.starts_with("<!--") {
                    rest = rest.find("-->").map(|e| &rest[e + 3..]).unwrap_or("");
                    continue;
                }
                let Some(end) = rest.find('>') else { break };
                let inner = &rest[1..end];
                rest = &rest[end + 1..];
                if inner.starts_with('!') || inner.starts_with('?') {
                    continue;
                }
                if let Some(name) = inner.strip_prefix('/') {
                    toks.push(Tok::Close(name.trim().to_lowercase()));
                    continue;
                }
                let self_closing = inner.ends_with('/');
                let inner = inner.trim_end_matches('/');
                let name_end = inner.find(char::is_whitespace).unwrap_or(inner.len());
                let name = inner[..name_end].to_lowercase();
                toks.push(Tok::Open(name, attributes(&inner[name_end..]), self_closing));
            }
            Some(i) => {
                toks.push(Tok::Text(decode(&rest[..i])));
                rest = &rest[i..];
            }
            None => {
                toks.push(Tok::Text(decode(rest)));
                rest = "";
            }
        }
    }
    toks
}

fn attributes(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = s.trim();
    while !rest.is_empty() {
        let eq = rest.find('=');
        let ws = rest.find(char::is_whitespace);
        match eq {
            Some(e) if ws.is_none_or(|w| e < w) => {
                let key = rest[..e].trim().to_lowercase();
                let after = rest[e + 1..].trim_start();
                let (val, next) = if let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') {
                    let body = &after[1..];
                    let close = body.find(q).unwrap_or(body.len());
                    (&body[..close], body.get(close + 1..).unwrap_or(""))
                } else {
                    let close = after.find(char::is_whitespace).unwrap_or(after.len());
                    (&after[..close], &after[close..])
                };
                out.push((key, decode(val)));
                rest = next.trim_start();
            }
            _ => {
                let w = ws.unwrap_or(rest.len());
                out.push((rest[..w].to_lowercase(), String::new()));
                rest = rest[w..].trim_start();
            }
        }
    }
    out
}

fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// Apply a `style="…"` attribute to the formatting.
fn apply_style(f: &mut Fmt, style: &str, in_heading: bool) {
    for decl in style.split(';') {
        let Some((k, v)) = decl.split_once(':') else { continue };
        let (k, v) = (k.trim().to_lowercase(), v.trim());
        match k.as_str() {
            "font-weight" => f.bold = !in_heading && v.parse::<u32>().map(|w| w >= 600).unwrap_or(v == "bold"),
            "font-style" => f.italic = v == "italic",
            "text-decoration" => {
                f.underline = v.contains("underline");
                f.strike = v.contains("line-through");
            }
            "font-family" => {
                let fam = v.split(',').next().unwrap_or("").trim().trim_matches(['\'', '"']).to_string();
                if !fam.is_empty() && fam != DEFAULT_FONT {
                    f.font = Some(fam);
                }
            }
            "font-size" if !in_heading => {
                if let Some(pt) = v.strip_suffix("pt").and_then(|n| n.trim().parse::<f64>().ok())
                    && (pt - DEFAULT_SIZE).abs() > 0.01
                {
                    f.size = Some(pt);
                }
            }
            "color" => f.color = hex(v).filter(|c| c != DEFAULT_COLOR && c != "#000000"),
            "background-color" => f.highlight = hex(v).filter(|c| c != "#ffffff" && c != "transparent"),
            _ => {}
        }
    }
}

fn hex(v: &str) -> Option<String> {
    let v = v.trim().to_lowercase();
    (v.len() == 7 && v.starts_with('#') && v[1..].chars().all(|c| c.is_ascii_hexdigit())).then_some(v)
}

pub fn from_html(html: &str) -> Document {
    let body_start = html.find("<body").unwrap_or(0);
    let toks = tokenize(&html[body_start..]);
    let mut blocks: Vec<Block> = Vec::new();
    let mut para: Option<Paragraph> = None;
    let mut stack: Vec<(String, Fmt)> = vec![("body".into(), Fmt::default())];
    let mut lists: Vec<List> = Vec::new();
    let mut table: Option<(Table, Vec<String>, Option<String>)> = None;

    let finish = |para: &mut Option<Paragraph>, blocks: &mut Vec<Block>| {
        if let Some(p) = para.take() {
            blocks.push(Block::Paragraph(p));
        }
    };

    for tok in toks {
        match tok {
            Tok::Open(name, attrs, self_closing) => {
                let mut f = stack.last().map(|s| s.1.clone()).unwrap_or_default();
                let heading = matches!(name.as_str(), "h1" | "h2" | "h3");
                if let Some(st) = attr(&attrs, "style") {
                    apply_style(&mut f, st, heading || para.as_ref().is_some_and(|p| p.style.is_heading()));
                }
                match name.as_str() {
                    "b" | "strong" => f.bold = true,
                    "i" | "em" => f.italic = true,
                    "u" => f.underline = true,
                    "s" | "del" => f.strike = true,
                    _ => {}
                }
                match name.as_str() {
                    "table" => {
                        finish(&mut para, &mut blocks);
                        table = Some((Table { rows: Vec::new(), header: false }, Vec::new(), None));
                    }
                    "tr" => {
                        if let Some(t) = table.as_mut() {
                            t.1.clear();
                        }
                    }
                    "td" | "th" => {
                        if let Some(t) = table.as_mut() {
                            t.2 = Some(String::new());
                            if name == "th" && t.0.rows.is_empty() {
                                t.0.header = true;
                            }
                        }
                    }
                    "ul" | "ol" => lists.push(if name == "ol" { List::Number } else { List::Bullet }),
                    "p" | "h1" | "h2" | "h3" | "li" | "div" if table.is_none() => {
                        finish(&mut para, &mut blocks);
                        let style = match name.as_str() {
                            "h1" => Style::H1,
                            "h2" => Style::H2,
                            "h3" => Style::H3,
                            _ => Style::Normal,
                        };
                        let align = Align::from_id(attr(&attrs, "align").unwrap_or("left"));
                        let (list, indent) = if name == "li" {
                            (lists.last().copied(), (lists.len().saturating_sub(1)).min(MAX_INDENT as usize) as u8)
                        } else {
                            (None, 0)
                        };
                        para = Some(Paragraph { style, align, list, indent, runs: Vec::new() });
                    }
                    "br" => {
                        if let Some(t) = table.as_mut().and_then(|t| t.2.as_mut()) {
                            t.push(' ');
                        } else if para.as_ref().is_some_and(|p| !p.runs.is_empty()) {
                            // A line break inside a paragraph starts a new one with the same look.
                            let next = para.as_ref().map(|p| Paragraph { runs: Vec::new(), ..p.clone() });
                            finish(&mut para, &mut blocks);
                            para = next;
                        }
                    }
                    "img" => {
                        if let Some(src) = attr(&attrs, "src") {
                            let name = src.trim_start_matches("media/").to_string();
                            let width = attr(&attrs, "width").and_then(|w| w.parse::<f64>().ok()).unwrap_or(300.0);
                            para.get_or_insert_with(Paragraph::default).runs.push(Run::Image(Image { image: name, width, wrap: None }));
                        }
                    }
                    _ => {}
                }
                if !self_closing && !matches!(name.as_str(), "br" | "img" | "meta") {
                    stack.push((name, f));
                }
            }
            Tok::Close(name) => {
                if let Some(pos) = stack.iter().rposition(|(n, _)| *n == name) {
                    stack.truncate(pos.max(1));
                }
                match name.as_str() {
                    "ul" | "ol" => {
                        lists.pop();
                    }
                    "td" | "th" => {
                        if let Some(t) = table.as_mut()
                            && let Some(cell) = t.2.take()
                        {
                            t.1.push(cell.trim().to_string());
                        }
                    }
                    "tr" => {
                        if let Some(t) = table.as_mut() {
                            let row = std::mem::take(&mut t.1);
                            t.0.rows.push(row);
                        }
                    }
                    "table" => {
                        if let Some((mut t, _, _)) = table.take() {
                            let cols = t.cols();
                            for r in &mut t.rows {
                                r.resize(cols, String::new());
                            }
                            if !t.rows.is_empty() {
                                blocks.push(Block::Table(t));
                            }
                        }
                    }
                    "p" | "h1" | "h2" | "h3" | "li" | "div" => finish(&mut para, &mut blocks),
                    _ => {}
                }
            }
            Tok::Text(text) => {
                if let Some(cell) = table.as_mut().and_then(|t| t.2.as_mut()) {
                    cell.push_str(&text);
                    continue;
                }
                // Whitespace between block tags isn't text.
                if para.is_none() && text.trim().is_empty() {
                    continue;
                }
                let f = stack.last().map(|s| s.1.clone()).unwrap_or_default();
                let run = TextRun {
                    text: text.replace(['\n', '\r'], " "),
                    bold: f.bold,
                    italic: f.italic,
                    underline: f.underline,
                    strike: f.strike,
                    font: f.font,
                    size: f.size,
                    color: f.color,
                    highlight: f.highlight,
                    comment: None,
                    inserted: None,
                    deleted: None,
                };
                para.get_or_insert_with(Paragraph::default).runs.push(Run::Text(run));
            }
        }
    }
    finish(&mut para, &mut blocks);
    let mut doc = Document { blocks, ..Default::default() };
    doc.tidy();
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    const QT: &str = r#"<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.0//EN" "http://www.w3.org/TR/REC-html40/strict.dtd">
<html><head><meta name="qrichtext" content="1" /><meta charset="utf-8" /><style type="text/css">
p, li { white-space: pre-wrap; }
</style></head><body style=" font-family:'Noto Sans'; font-size:11pt; font-weight:400; font-style:normal;">
<p align="right" style=" margin-top:0px;">[Date]</p>
<h1 style=" margin-top:18px;"><span style=" font-size:xx-large; font-weight:700;">Soup &amp; bread</span></h1>
<p style=" margin-top:0px;"><span style=" font-weight:700;">Serves:</span> 4&nbsp;people <span style=" color:#ff0000;">hot</span></p>
<ul style="margin-top: 0px;"><li style=" margin-top:12px;">one</li>
<li>two</li></ul>
<ol><li>first</li></ol>
<p><img src="media/a.png" width="200" /></p>
<table border="1"><tr><td><p>a</p></td><td><p>b</p></td></tr><tr><td><p>c</p></td><td><p>d</p></td></tr></table>
</body></html>"#;

    #[test]
    fn reads_qt_html() {
        let d = from_html(QT);
        let paras: Vec<&Paragraph> =
            d.blocks.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).collect();
        assert_eq!(paras[0].align, Align::Right);
        assert_eq!(paras[1].style, Style::H1);
        assert_eq!(paras[1].plain_text(), "Soup & bread");
        // A heading's bold is its style's, not the run's.
        assert!(matches!(&paras[1].runs[0], Run::Text(t) if !t.bold));
        assert!(matches!(&paras[2].runs[0], Run::Text(t) if t.bold && t.text == "Serves:"));
        assert!(paras[2].runs.iter().any(|r| matches!(r, Run::Text(t) if t.color.as_deref() == Some("#ff0000"))));
        assert_eq!(paras[3].list, Some(List::Bullet));
        assert_eq!(paras[5].list, Some(List::Number));
        assert!(matches!(&paras[6].runs[0], Run::Image(i) if i.image == "a.png" && i.width == 200.0));
        let table = d.blocks.iter().find_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).unwrap();
        assert_eq!(table.rows, vec![vec!["a", "b"], vec!["c", "d"]]);
    }
}
