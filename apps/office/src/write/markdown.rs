//! Markdown and plain text, read and written.
//!
//! Markdown keeps headings, lists, quotes, tables, links, bold, italic,
//! strikethrough, superscript and subscript (as `<sup>`/`<sub>`), code and
//! footnotes. Pictures go in a folder beside the file ("Notes images/"). What it
//! can't keep (colours, fonts, alignment, comments…) is named on saving.

use super::model::*;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The font code is set in.
pub const CODE_FONT: &str = "monospace";

fn is_code(r: &TextRun) -> bool {
    r.font.as_deref().is_some_and(|f| f.eq_ignore_ascii_case(CODE_FONT) || f.to_lowercase().contains("mono"))
}

// ---------- Reading ----------

/// Pictures larger than this are set at this width, in points.
const MAX_PICTURE_W: f64 = 468.0;

struct Reader<'a> {
    base: Option<&'a Path>,
    blocks: Vec<Block>,
    media: BTreeMap<String, Vec<u8>>,
    para: Option<Paragraph>,
    /// Bold, italic, strikethrough, superscript, subscript: how deep in each.
    bold: u32,
    italic: u32,
    strike: u32,
    sup: u32,
    sub: u32,
    link: Option<String>,
    /// Each open list: numbered or not.
    lists: Vec<bool>,
    quote: u32,
    heading: Option<Style>,
    code_block: bool,
    /// A table being read: its rows, and the cell being filled.
    table: Option<(Vec<Vec<String>>, Option<String>)>,
    /// Inside a picture: its words are its description, not text.
    in_image: bool,
    /// Inside a footnote's definition (read in the first pass).
    in_note: bool,
    notes: HashMap<String, String>,
}

impl Reader<'_> {
    fn style(&self) -> Style {
        if let Some(h) = self.heading {
            h
        } else if self.quote > 0 {
            Style::Quote
        } else {
            Style::Normal
        }
    }

    fn para(&mut self) -> &mut Paragraph {
        if self.para.is_none() {
            let list = self.lists.last().map(|n| if *n { List::Number } else { List::Bullet });
            let indent = (self.lists.len().saturating_sub(1) as u8).min(MAX_INDENT);
            self.para = Some(Paragraph { style: self.style(), list, indent, ..Default::default() });
        }
        self.para.as_mut().expect("a paragraph")
    }

    fn finish(&mut self) {
        if let Some(p) = self.para.take() {
            self.blocks.push(Block::Paragraph(p));
        }
    }

    fn text(&mut self, t: &str, code: bool) {
        if self.in_note || self.in_image {
            return;
        }
        if let Some((_, Some(cell))) = &mut self.table {
            cell.push_str(t);
            return;
        }
        let run = TextRun {
            text: t.to_string(),
            bold: self.bold > 0,
            italic: self.italic > 0,
            strike: self.strike > 0,
            superscript: self.sup > 0,
            subscript: self.sub > 0 && self.sup == 0,
            link: self.link.clone(),
            font: (code || self.code_block).then(|| CODE_FONT.to_string()),
            ..Default::default()
        };
        let p = self.para();
        match p.runs.last_mut() {
            Some(Run::Text(last)) if last.same_format(&run) => last.text.push_str(&run.text),
            _ => p.runs.push(Run::Text(run)),
        }
    }

    /// A picture from a file beside the document (or anywhere, by its full path).
    fn picture(&mut self, dest: &str) {
        let dest = percent_decode(dest.trim());
        if dest.contains("://") && !dest.starts_with("file://") {
            return;
        }
        let rel = PathBuf::from(dest.trim_start_matches("file://"));
        let path = if rel.is_absolute() { rel } else { self.base.map(|b| b.join(&rel)).unwrap_or(rel) };
        let Ok(texture) = gtk::gdk::Texture::from_filename(&path) else { return };
        use gtk::prelude::*;
        let png = texture.save_to_png_bytes().to_vec();
        let name = (1..).map(|n| format!("picture-{n}.png")).find(|n| !self.media.contains_key(n)).unwrap_or_default();
        let width = (texture.width() as f64 * 0.75).min(MAX_PICTURE_W);
        self.media.insert(name.clone(), png);
        self.para().runs.push(Run::Image(Image { image: name, width, wrap: None }));
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_FOOTNOTES | Options::ENABLE_TASKLISTS
}

/// A document from Markdown; pictures are looked for relative to `base`.
pub fn read(text: &str, base: Option<&Path>) -> (Document, BTreeMap<String, Vec<u8>>) {
    // First pass: what each footnote says.
    let mut notes = HashMap::new();
    let mut current: Option<(String, String)> = None;
    for ev in Parser::new_ext(text, options()) {
        match ev {
            Event::Start(Tag::FootnoteDefinition(label)) => current = Some((label.to_string(), String::new())),
            Event::End(TagEnd::FootnoteDefinition) => {
                if let Some((l, t)) = current.take() {
                    notes.insert(l, t.trim().to_string());
                }
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, s)) = &mut current {
                    s.push_str(&t);
                }
            }
            Event::SoftBreak | Event::HardBreak | Event::End(TagEnd::Paragraph) => {
                if let Some((_, s)) = &mut current {
                    s.push(' ');
                }
            }
            _ => {}
        }
    }

    let mut r = Reader {
        base,
        blocks: Vec::new(),
        media: BTreeMap::new(),
        para: None,
        bold: 0,
        italic: 0,
        strike: 0,
        sup: 0,
        sub: 0,
        link: None,
        lists: Vec::new(),
        quote: 0,
        heading: None,
        code_block: false,
        table: None,
        in_image: false,
        in_note: false,
        notes,
    };
    for ev in Parser::new_ext(text, options()) {
        match ev {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {}
                Tag::Heading { level, .. } => {
                    r.finish();
                    r.heading = Some(match level {
                        HeadingLevel::H1 => Style::H1,
                        HeadingLevel::H2 => Style::H2,
                        _ => Style::H3,
                    });
                }
                Tag::BlockQuote(_) => {
                    r.finish();
                    r.quote += 1;
                }
                Tag::CodeBlock(_) => {
                    r.finish();
                    r.code_block = true;
                }
                Tag::List(start) => {
                    r.finish();
                    r.lists.push(start.is_some());
                }
                Tag::Item => r.finish(),
                Tag::FootnoteDefinition(_) => {
                    r.finish();
                    r.in_note = true;
                }
                Tag::Table(_) => {
                    r.finish();
                    r.table = Some((Vec::new(), None));
                }
                Tag::TableHead | Tag::TableRow => {
                    if let Some((rows, _)) = &mut r.table {
                        rows.push(Vec::new());
                    }
                }
                Tag::TableCell => {
                    if let Some((_, cell)) = &mut r.table {
                        *cell = Some(String::new());
                    }
                }
                Tag::Emphasis => r.italic += 1,
                Tag::Strong => r.bold += 1,
                Tag::Strikethrough => r.strike += 1,
                Tag::Superscript => r.sup += 1,
                Tag::Subscript => r.sub += 1,
                Tag::Link { dest_url, .. } => r.link = Some(dest_url.to_string()),
                Tag::Image { dest_url, .. } => {
                    if !r.in_note {
                        r.picture(&dest_url);
                    }
                    r.in_image = true;
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => {
                    // In a tight list the item's own end finishes it.
                    r.finish();
                }
                TagEnd::Heading(_) => {
                    r.finish();
                    r.heading = None;
                }
                TagEnd::BlockQuote(_) => {
                    r.finish();
                    r.quote = r.quote.saturating_sub(1);
                }
                TagEnd::CodeBlock => {
                    r.finish();
                    r.code_block = false;
                }
                TagEnd::List(_) => {
                    r.finish();
                    r.lists.pop();
                }
                TagEnd::Item => r.finish(),
                TagEnd::FootnoteDefinition => r.in_note = false,
                TagEnd::TableCell => {
                    if let Some((rows, cell)) = &mut r.table
                        && let (Some(row), Some(c)) = (rows.last_mut(), cell.take())
                    {
                        row.push(c.trim().to_string());
                    }
                }
                TagEnd::Table => {
                    if let Some((rows, _)) = r.table.take() {
                        let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
                        if cols > 0 {
                            let rows: Vec<Vec<String>> = rows
                                .into_iter()
                                .map(|mut row| {
                                    row.resize(cols, String::new());
                                    row
                                })
                                .collect();
                            r.blocks.push(Block::Table(Table { rows, header: true }));
                        }
                    }
                }
                TagEnd::Emphasis => r.italic = r.italic.saturating_sub(1),
                TagEnd::Strong => r.bold = r.bold.saturating_sub(1),
                TagEnd::Strikethrough => r.strike = r.strike.saturating_sub(1),
                TagEnd::Superscript => r.sup = r.sup.saturating_sub(1),
                TagEnd::Subscript => r.sub = r.sub.saturating_sub(1),
                TagEnd::Link => r.link = None,
                TagEnd::Image => r.in_image = false,
                _ => {}
            },
            Event::Text(t) => {
                if r.code_block {
                    // Each line of code is a paragraph of its own.
                    let mut lines = t.split('\n').peekable();
                    while let Some(line) = lines.next() {
                        if !line.is_empty() {
                            r.text(line, true);
                        }
                        if lines.peek().is_some() {
                            r.para();
                            r.finish();
                        }
                    }
                } else {
                    r.text(&t, false);
                }
            }
            Event::Code(t) => r.text(&t, true),
            Event::InlineHtml(h) => match h.trim().to_lowercase().as_str() {
                "<sup>" => r.sup += 1,
                "</sup>" => r.sup = r.sup.saturating_sub(1),
                "<sub>" => r.sub += 1,
                "</sub>" => r.sub = r.sub.saturating_sub(1),
                "<br>" | "<br/>" | "<br />" => r.text(&LINE_BREAK.to_string(), false),
                _ => {}
            },
            Event::SoftBreak => r.text(" ", false),
            Event::HardBreak => r.text(&LINE_BREAK.to_string(), false),
            Event::FootnoteReference(label) => {
                if !r.in_note {
                    let note = r.notes.get(label.as_ref()).cloned().unwrap_or_default();
                    r.para().runs.push(Run::Note(Note { note, end: false }));
                }
            }
            Event::TaskListMarker(done) => r.text(if done { "☑ " } else { "☐ " }, false),
            Event::Rule => {
                r.finish();
                r.blocks.push(Block::Paragraph(Paragraph::default()));
            }
            _ => {}
        }
    }
    r.finish();
    // Paragraphs left empty by a list item holding only a nested list.
    let mut blocks: Vec<Block> = Vec::new();
    for b in r.blocks {
        if let Block::Paragraph(p) = &b
            && p.runs.is_empty()
            && p.list.is_some()
        {
            continue;
        }
        blocks.push(b);
    }
    if blocks.is_empty() {
        blocks.push(Block::Paragraph(Paragraph::default()));
    }
    (Document { blocks, ..Default::default() }, r.media)
}

/// A document from plain text: a paragraph per line.
pub fn read_text(text: &str) -> Document {
    let mut blocks: Vec<Block> =
        text.lines().map(|l| Block::Paragraph(Paragraph::text(Style::Normal, l.trim_end_matches('\r')))).collect();
    if blocks.is_empty() {
        blocks.push(Block::Paragraph(Paragraph::default()));
    }
    Document { blocks, ..Default::default() }
}

// ---------- Writing ----------

/// Characters that would mean something to Markdown, escaped.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '*' | '_' | '`' | '[' | ']' | '<' | '>' | '~' | '|') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// What a line starting with this would turn into, escaped: "# x", "1. x", "- x".
fn escape_start(s: &str) -> String {
    let t = s.trim_start();
    let starts_block = t.starts_with('#')
        || t.starts_with("- ")
        || t.starts_with("+ ")
        || t.starts_with("= ")
        || t.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    if starts_block {
        let lead = s.len() - t.len();
        let first = t.chars().next().map_or(0, char::len_utf8);
        // "1. x" becomes "1\. x"; "# x" becomes "\# x".
        if t.as_bytes()[0].is_ascii_digit() {
            let dot = t.find(". ").unwrap_or(0);
            return format!("{}{}\\{}", &s[..lead], &t[..dot], &t[dot..]);
        }
        return format!("{}\\{}{}", &s[..lead], &t[..first], &t[first..]);
    }
    s.to_string()
}

/// The formatting Markdown keeps, for joining runs that look the same in it.
fn md_look(r: &TextRun) -> (bool, bool, bool, bool, bool, bool, Option<&str>) {
    (r.bold, r.italic, r.strike, r.superscript, r.subscript, is_code(r), r.link.as_deref())
}

/// One stretch of text with its marks, spaces kept outside them.
fn inline(text: &str, r: &TextRun) -> String {
    let lead = &text[..text.len() - text.trim_start().len()];
    let trail = &text[text.trim_end().len()..];
    let core = text.trim();
    if core.is_empty() {
        return text.to_string();
    }
    let mut s = if is_code(r) {
        let ticks = if core.contains('`') { "``" } else { "`" };
        format!("{ticks}{core}{ticks}")
    } else {
        escape(core)
    };
    if r.superscript {
        s = format!("<sup>{s}</sup>");
    } else if r.subscript {
        s = format!("<sub>{s}</sub>");
    }
    if r.strike {
        s = format!("~~{s}~~");
    }
    if r.italic {
        s = format!("*{s}*");
    }
    if r.bold {
        s = format!("**{s}**");
    }
    if let Some(l) = &r.link {
        s = format!("[{s}](<{}>)", l.replace('>', "%3E"));
    }
    format!("{lead}{s}{trail}")
}

/// A paragraph's runs as one line of Markdown, with its footnotes and pictures.
fn runs(p: &Paragraph, notes: &mut Vec<String>, pictures: &mut Vec<String>) -> String {
    let mut out = String::new();
    let mut pending: Option<TextRun> = None;
    let flush = |out: &mut String, pending: &mut Option<TextRun>| {
        if let Some(r) = pending.take() {
            out.push_str(&inline(&r.text, &r));
        }
    };
    for run in &p.runs {
        match run {
            // Deleted text (a tracked change) goes; the document reads as if accepted.
            Run::Text(t) if t.deleted.is_some() => {}
            Run::Text(t) => match &mut pending {
                Some(prev) if md_look(prev) == md_look(t) => prev.text.push_str(&t.text),
                _ => {
                    flush(&mut out, &mut pending);
                    pending = Some(t.clone());
                }
            },
            Run::Note(n) => {
                flush(&mut out, &mut pending);
                notes.push(n.note.clone());
                let _ = write!(out, "[^{}]", notes.len());
            }
            Run::Image(img) => {
                flush(&mut out, &mut pending);
                pictures.push(img.image.clone());
                out.push_str(&format!("![](<{{PICTURES}}/{}>)", img.image));
            }
            Run::Drawing(_) => flush(&mut out, &mut pending),
        }
    }
    flush(&mut out, &mut pending);
    out.replace('\n', " ").replace(LINE_BREAK, "<br>")
}

/// Markdown for `doc`; pictures are linked in the folder `pictures_dir` (relative to
/// the file). Also returns the pictures used and what Markdown couldn't keep.
pub fn write(doc: &Document, pictures_dir: &str) -> (String, Vec<String>, Vec<String>) {
    let mut out = String::new();
    let mut notes: Vec<String> = Vec::new();
    let mut pictures: Vec<String> = Vec::new();
    let mut lost: Vec<&str> = Vec::new();
    let mut lose = |what: &'static str| {
        if !lost.contains(&what) {
            lost.push(what);
        }
    };
    let mut numbers = [0usize; MAX_INDENT as usize + 1];
    let mut prev_list = false;
    for b in &doc.blocks {
        match b {
            Block::Paragraph(p) => {
                let line = runs(p, &mut notes, &mut pictures);
                if p.align != Align::Left {
                    lose("alignment");
                }
                if p.line_spacing.is_some() || p.space_before.is_some() || p.space_after.is_some() {
                    lose("line and paragraph spacing");
                }
                for r in &p.runs {
                    match r {
                        Run::Text(t) => {
                            if t.underline {
                                lose("underlining");
                            }
                            if t.color.is_some() || t.highlight.is_some() {
                                lose("text colors and highlights");
                            }
                            if t.size.is_some() || (t.font.is_some() && !is_code(t)) {
                                lose("fonts and sizes");
                            }
                            if t.comment.is_some() {
                                lose("comments");
                            }
                            if t.inserted.is_some() || t.deleted.is_some() {
                                lose("tracked changes (it's saved as if they were accepted)");
                            }
                        }
                        Run::Drawing(_) => lose("shapes and charts"),
                        Run::Image(i) if i.wrap.is_some() => lose("text wrapped beside pictures"),
                        Run::Note(n) if n.end => lose("endnotes (they become footnotes)"),
                        _ => {}
                    }
                }
                let line = escape_start(&line);
                match p.list {
                    Some(list) => {
                        let lvl = p.indent.min(MAX_INDENT) as usize;
                        if !prev_list {
                            numbers = [0; MAX_INDENT as usize + 1];
                        }
                        for n in numbers.iter_mut().skip(lvl + 1) {
                            *n = 0;
                        }
                        let pad = "    ".repeat(lvl);
                        if list == List::Number {
                            numbers[lvl] += 1;
                            let _ = writeln!(out, "{pad}{}. {line}", numbers[lvl]);
                        } else {
                            let _ = writeln!(out, "{pad}- {line}");
                        }
                        prev_list = true;
                        continue;
                    }
                    None => {
                        if prev_list {
                            out.push('\n');
                        }
                        prev_list = false;
                    }
                }
                match p.style {
                    Style::Title | Style::H1 => {
                        if p.style == Style::Title {
                            lose("the Title style (it becomes a heading)");
                        }
                        let _ = writeln!(out, "# {line}\n");
                    }
                    Style::H2 => {
                        let _ = writeln!(out, "## {line}\n");
                    }
                    Style::H3 => {
                        let _ = writeln!(out, "### {line}\n");
                    }
                    Style::Quote => {
                        let _ = writeln!(out, "> {line}\n");
                    }
                    Style::Normal if line.is_empty() => {}
                    Style::Normal => {
                        let _ = writeln!(out, "{line}\n");
                    }
                }
            }
            Block::Table(t) => {
                if prev_list {
                    out.push('\n');
                    prev_list = false;
                }
                let cell = |c: &str| escape(c).replace('\n', " ");
                for (i, row) in t.rows.iter().enumerate() {
                    let _ = writeln!(out, "| {} |", row.iter().map(|c| cell(c)).collect::<Vec<_>>().join(" | "));
                    if i == 0 {
                        let _ = writeln!(out, "|{}", " --- |".repeat(row.len()));
                    }
                }
                out.push('\n');
            }
            Block::PageBreak => {
                lose("page breaks (they become --- lines)");
                prev_list = false;
                out.push_str("---\n\n");
            }
            Block::Contents => lose("the table of contents"),
            Block::Section(_) => lose("sections and columns"),
        }
    }
    if !notes.is_empty() {
        for (i, n) in notes.iter().enumerate() {
            let _ = writeln!(out, "[^{}]: {}", i + 1, escape(n).replace('\n', " "));
        }
    }
    let p = &doc.page;
    if !p.header.is_empty() || !p.footer.is_empty() || p.page_numbers || p.columns() > 1 {
        lose("the page setup (header, footer, page numbers, columns)");
    }
    if !doc.comments.is_empty() {
        lose("comments");
    }
    let dir = pictures_dir.replace('>', "%3E");
    let text = out.trim_end().replace("{PICTURES}", &dir) + "\n";
    (text, pictures, lost.into_iter().map(String::from).collect())
}

/// Plain text for `doc`: a line per paragraph, lists marked, table cells
/// separated by tabs, footnotes listed at the end.
pub fn write_text(doc: &Document) -> String {
    let mut out = String::new();
    let mut notes: Vec<String> = Vec::new();
    let mut numbers = [0usize; MAX_INDENT as usize + 1];
    for b in &doc.blocks {
        match b {
            Block::Paragraph(p) => {
                let mut line = String::new();
                for r in &p.runs {
                    match r {
                        Run::Text(t) if t.deleted.is_none() => line.push_str(&t.text.replace(LINE_BREAK, "\n")),
                        Run::Note(n) => {
                            notes.push(n.note.clone());
                            let _ = write!(line, "[{}]", notes.len());
                        }
                        _ => {}
                    }
                }
                let pad = "    ".repeat(p.indent as usize);
                match p.list {
                    Some(List::Number) => {
                        let lvl = (p.indent as usize).min(MAX_INDENT as usize);
                        numbers[lvl] += 1;
                        for n in numbers.iter_mut().skip(lvl + 1) {
                            *n = 0;
                        }
                        let _ = writeln!(out, "{pad}{}. {line}", numbers[lvl]);
                    }
                    Some(List::Bullet) => {
                        let _ = writeln!(out, "{pad}• {line}");
                    }
                    None => {
                        numbers = [0; MAX_INDENT as usize + 1];
                        let _ = writeln!(out, "{pad}{line}");
                    }
                }
            }
            Block::Table(t) => {
                for row in &t.rows {
                    let _ = writeln!(out, "{}", row.join("\t"));
                }
            }
            Block::PageBreak | Block::Contents | Block::Section(_) => {}
        }
    }
    if !notes.is_empty() {
        out.push_str("\nNotes\n");
        for (i, n) in notes.iter().enumerate() {
            let _ = writeln!(out, "[{}] {n}", i + 1);
        }
    }
    out
}

/// The folder a Markdown file's pictures go in: "Notes images" beside "Notes.md".
pub fn pictures_dir(path: &Path) -> String {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
    format!("{stem} images")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Trip notes\n\nLine one<br>line two\n\nWe go **by train**, *maybe* ~~by car~~. See [the map](https://example.com/map).\nH<sub>2</sub>O and E = mc<sup>2</sup>.[^1]\n\n## Packing\n\n- Boots\n- Coats\n    - Rain coat\n1. Book\n2. Tickets\n\n> Travel light.\n\n| Day | Where |\n| --- | --- |\n| Mon | Leeds |\n\n```\nlet x = 1;\nlet y = 2;\n```\n\n- [x] Done\n\n[^1]: Roughly.\n";

    fn texts(p: &Paragraph) -> Vec<&TextRun> {
        p.runs.iter().filter_map(|r| if let Run::Text(t) = r { Some(t) } else { None }).collect()
    }

    #[test]
    fn reads_markdown() {
        let (d, _) = read(SAMPLE, None);
        let paras: Vec<&Paragraph> = d.blocks.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).collect();
        assert_eq!(paras[0].style, Style::H1);
        assert_eq!(paras[0].plain_text(), "Trip notes");
        assert_eq!(paras[1].plain_text(), "Line one\u{2028}line two");
        let body = texts(paras[2]);
        assert!(body.iter().any(|t| t.bold && t.text == "by train"));
        assert!(body.iter().any(|t| t.italic && t.text == "maybe"));
        assert!(body.iter().any(|t| t.strike && t.text == "by car"));
        assert!(body.iter().any(|t| t.link.as_deref() == Some("https://example.com/map") && t.text == "the map"));
        assert!(body.iter().any(|t| t.subscript && t.text == "2"));
        assert!(body.iter().any(|t| t.superscript && t.text == "2"));
        assert!(paras[2].runs.iter().any(|r| matches!(r, Run::Note(n) if n.note == "Roughly.")));
        assert_eq!(paras[3].style, Style::H2);
        let listed: Vec<(String, Option<List>, u8)> =
            paras.iter().filter(|p| p.list.is_some()).map(|p| (p.plain_text(), p.list, p.indent)).collect();
        assert_eq!(
            listed,
            [
                ("Boots".to_string(), Some(List::Bullet), 0),
                ("Coats".to_string(), Some(List::Bullet), 0),
                ("Rain coat".to_string(), Some(List::Bullet), 1),
                ("Book".to_string(), Some(List::Number), 0),
                ("Tickets".to_string(), Some(List::Number), 0),
                ("☑ Done".to_string(), Some(List::Bullet), 0),
            ]
        );
        assert!(paras.iter().any(|p| p.style == Style::Quote && p.plain_text() == "Travel light."));
        assert!(d.blocks.iter().any(|b| matches!(b, Block::Table(t) if t.rows == vec![vec!["Day".to_string(), "Where".into()], vec!["Mon".into(), "Leeds".into()]])));
        let code: Vec<&&Paragraph> = paras.iter().filter(|p| texts(p).iter().any(|t| is_code(t))).collect();
        assert_eq!(code.iter().map(|p| p.plain_text()).collect::<Vec<_>>(), ["let x = 1;", "let y = 2;"]);
    }

    #[test]
    fn round_trips() {
        let (d, _) = read(SAMPLE, None);
        let (md, _, lost) = write(&d, "pics");
        let (back, _) = read(&md, None);
        assert_eq!(back.blocks, d.blocks, "\n{md}");
        assert!(lost.is_empty(), "{lost:?}");
    }

    #[test]
    fn escapes_and_notes_what_is_lost() {
        let d = Document {
            blocks: vec![
                Block::Paragraph(Paragraph::text(Style::Normal, "# not a heading, 2*3 = 6 [ok]")),
                Block::Paragraph(Paragraph::text(Style::Normal, "1. not a list")),
                Block::Paragraph(Paragraph {
                    align: Align::Center,
                    runs: vec![Run::styled("red", |r| r.color = Some("#ff0000".into()))],
                    ..Default::default()
                }),
            ],
            ..Default::default()
        };
        let (md, _, lost) = write(&d, "pics");
        let (back, _) = read(&md, None);
        let plain: Vec<String> =
            back.blocks.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.plain_text()) } else { None }).collect();
        assert_eq!(plain, ["# not a heading, 2*3 = 6 [ok]", "1. not a list", "red"]);
        assert!(lost.contains(&"alignment".to_string()) && lost.contains(&"text colors and highlights".to_string()));
    }

    #[test]
    fn plain_text() {
        let d = read_text("one\r\ntwo\n\nfour");
        assert_eq!(d.blocks.len(), 4);
        let (md, _) = read(SAMPLE, None);
        let t = write_text(&md);
        assert!(t.starts_with("Trip notes\n"));
        assert!(t.contains("• Boots\n") && t.contains("    • Rain coat\n") && t.contains("2. Tickets\n"));
        assert!(t.contains("Day\tWhere\n") && t.ends_with("[1] Roughly.\n"));
    }
}
