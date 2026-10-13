//! The document as saved: paragraphs of styled runs, pictures and tables, plus
//! the page setup. This is `content.json` in a `.nwrite` file, and what the
//! PDF renderer and the Word exchange work from.

use serde::{Deserialize, Serialize};

pub const DEFAULT_FONT: &str = "Noto Sans";
pub const DEFAULT_SIZE: f64 = 11.0;
pub const DEFAULT_COLOR: &str = "#202329";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Document {
    pub page: PageSetup,
    pub blocks: Vec<Block>,
    /// Comments on spans of text; runs point at them by id.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<Comment>,
    /// Who made each tracked change, and when; runs point at them by id.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub revisions: Vec<Revision>,
    /// New edits are tracked as changes.
    #[serde(skip_serializing_if = "is_false")]
    pub tracking: bool,
    /// Changes to how paragraph styles look, by style id.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub styles: std::collections::BTreeMap<String, StyleLook>,
}

/// A paragraph style's look as the document changes it (None: the style's own).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct StyleLook {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    /// `#rrggbb`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
}

/// How a paragraph style's text looks, everything worked out.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub font: String,
    pub size: f64,
    pub color: Option<String>,
    pub bold: bool,
    pub italic: bool,
}

/// A style's look with the document's changes to it.
pub fn look_of(styles: &std::collections::BTreeMap<String, StyleLook>, s: Style) -> Look {
    let (size, _, _, bold, italic) = s.look();
    let base = Look {
        font: DEFAULT_FONT.into(),
        size,
        color: (s == Style::Quote).then(|| QUOTE_COLOR.to_string()),
        bold,
        italic,
    };
    let Some(c) = styles.get(s.id()) else { return base };
    Look {
        font: c.font.clone().unwrap_or(base.font),
        size: c.size.unwrap_or(base.size),
        color: c.color.clone().or(base.color),
        bold: c.bold.unwrap_or(base.bold),
        italic: c.italic.unwrap_or(base.italic),
    }
}

/// Quotes' text colour, unless the document changes it.
pub const QUOTE_COLOR: &str = "#4a5060";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Revision {
    pub id: u32,
    pub author: String,
    /// `YYYY-MM-DDTHH:MM:SSZ` (empty if unknown).
    pub date: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Comment {
    pub id: u32,
    pub author: String,
    /// When it was written, `YYYY-MM-DDTHH:MM:SSZ` (empty if unknown).
    pub date: String,
    pub text: String,
    /// Dealt with: kept, but shown faded.
    #[serde(skip_serializing_if = "is_false")]
    pub resolved: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PageSetup {
    /// `letter`, `a4`, `legal` or `a5` (`print::PAPERS`).
    pub paper: String,
    /// The paper turned on its side.
    pub landscape: bool,
    /// Margins in points: top, bottom, left, right.
    pub margins: [f64; 4],
    pub header: String,
    pub footer: String,
    pub page_numbers: bool,
    /// The first page has no header, footer or page number (a letter, a title page).
    #[serde(default, skip_serializing_if = "is_false")]
    pub plain_first_page: bool,
    /// Even pages' own header and footer (None: the same as odd pages).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub even_header: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub even_footer: Option<String>,
    /// Text columns on the page, 1 to 3.
    pub columns: u8,
    /// Points between columns.
    pub column_gap: f64,
}

impl Default for PageSetup {
    fn default() -> Self {
        Self { paper: "letter".into(), landscape: false, margins: [72.0; 4], header: String::new(),
            footer: String::new(),
            page_numbers: false,
            plain_first_page: false,
            even_header: None,
            even_footer: None,
            columns: 1,
            column_gap: 18.0,
        }
    }
}

impl PageSetup {
    /// The page's size, turned when it's landscape.
    pub fn paper(&self) -> crate::print::Paper {
        let p = crate::print::Paper::from_id(&self.paper);
        if self.landscape { p.landscape() } else { p }
    }
    pub fn content_width(&self) -> f64 {
        (self.paper().width - self.margins[2] - self.margins[3]).max(72.0)
    }
    pub fn columns(&self) -> usize {
        self.columns.clamp(1, 3) as usize
    }
    /// The width of one text column.
    pub fn column_width(&self) -> f64 {
        let n = self.columns() as f64;
        ((self.content_width() - (n - 1.0) * self.column_gap.max(0.0)) / n).max(72.0)
    }
    /// The header and footer on page `index` (from 0): even pages may have their own.
    pub fn header_footer(&self, index: usize) -> (&str, &str) {
        if index % 2 == 1 {
            (self.even_header.as_deref().unwrap_or(&self.header), self.even_footer.as_deref().unwrap_or(&self.footer))
        } else {
            (&self.header, &self.footer)
        }
    }
    /// Even pages have their own header or footer.
    pub fn odd_even(&self) -> bool {
        self.even_header.is_some() || self.even_footer.is_some()
    }
    pub fn content_height(&self) -> f64 {
        (self.paper().height - self.margins[0] - self.margins[1]).max(72.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Block {
    Paragraph(Paragraph),
    Table(Table),
    /// What follows starts on a new page.
    #[serde(rename = "pagebreak")]
    PageBreak,
    /// A table of contents: the headings with their page numbers.
    Contents,
    /// What follows has its own columns, and starts a new page or carries on.
    Section(Section),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Section {
    /// Text columns from here on, 1 to 3.
    pub columns: u8,
    /// Start on a new page (otherwise straight on, below what's before).
    pub new_page: bool,
}

impl Section {
    pub fn describe(&self) -> String {
        let cols = match self.columns {
            0 | 1 => "one column",
            2 => "two columns",
            _ => "three columns",
        };
        format!("Section break · {cols}{}", if self.new_page { " · new page" } else { "" })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    #[default]
    Normal,
    Title,
    H1,
    H2,
    H3,
    Quote,
}

impl Style {
    pub const ALL: [Style; 6] = [Style::Normal, Style::Title, Style::H1, Style::H2, Style::H3, Style::Quote];

    pub fn id(self) -> &'static str {
        match self {
            Style::Normal => "normal",
            Style::Title => "title",
            Style::H1 => "h1",
            Style::H2 => "h2",
            Style::H3 => "h3",
            Style::Quote => "quote",
        }
    }
    pub fn from_id(id: &str) -> Style {
        Style::ALL.into_iter().find(|s| s.id() == id).unwrap_or_default()
    }
    pub fn name(self) -> &'static str {
        match self {
            Style::Normal => "Normal text",
            Style::Title => "Title",
            Style::H1 => "Heading 1",
            Style::H2 => "Heading 2",
            Style::H3 => "Heading 3",
            Style::Quote => "Quote",
        }
    }
    /// Size in points, spacing above and below in points, bold, italic.
    pub fn look(self) -> (f64, f64, f64, bool, bool) {
        match self {
            Style::Normal => (DEFAULT_SIZE, 0.0, 6.0, false, false),
            Style::Title => (26.0, 0.0, 10.0, true, false),
            Style::H1 => (20.0, 16.0, 6.0, true, false),
            Style::H2 => (15.5, 14.0, 4.0, true, false),
            Style::H3 => (13.0, 12.0, 4.0, true, false),
            Style::Quote => (DEFAULT_SIZE, 4.0, 10.0, false, true),
        }
    }
    /// Space above and below in points.
    pub fn spacing(self) -> (f64, f64) {
        let (_, above, below, _, _) = self.look();
        (above, below)
    }
    pub fn is_heading(self) -> bool {
        matches!(self, Style::Title | Style::H1 | Style::H2 | Style::H3)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

impl Align {
    pub fn id(self) -> &'static str {
        match self {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
            Align::Justify => "justify",
        }
    }
    pub fn from_id(id: &str) -> Align {
        match id {
            "center" => Align::Center,
            "right" => Align::Right,
            "justify" => Align::Justify,
            _ => Align::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum List {
    Bullet,
    Number,
}

/// Points each indent level moves a paragraph in by.
pub const INDENT_STEP: f64 = 24.0;
pub const MAX_INDENT: u8 = 6;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Paragraph {
    pub style: Style,
    pub align: Align,
    pub list: Option<List>,
    pub indent: u8,
    /// Line spacing as a multiple (1.5 = one and a half); None is single.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_spacing: Option<f64>,
    /// Space above and below the paragraph in points; None is the style's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_before: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_after: Option<f64>,
    pub runs: Vec<Run>,
}

/// A line break inside a paragraph (Shift+Enter): Unicode's line separator,
/// which breaks the line without starting a new paragraph.
pub const LINE_BREAK: char = '\u{2028}';

/// The line spacings offered, as in other word processors.
pub const LINE_SPACINGS: [f64; 4] = [1.0, 1.15, 1.5, 2.0];

impl Paragraph {
    /// Space above and below in points, the style's unless set.
    pub fn spacing(&self) -> (f64, f64) {
        let (above, below) = self.style.spacing();
        (self.space_before.unwrap_or(above), self.space_after.unwrap_or(below))
    }
    pub fn text(style: Style, text: &str) -> Paragraph {
        Paragraph { style, runs: vec![Run::text(text)], ..Default::default() }
    }
    /// The paragraph's words, without pictures or footnotes.
    pub fn plain_text_any(&self) -> String {
        self.runs.iter().filter_map(|r| if let Run::Text(t) = r { Some(t.text.as_str()) } else { None }).collect()
    }
    #[cfg(test)]
    pub fn plain_text(&self) -> String {
        self.runs
            .iter()
            .filter_map(|r| match r {
                Run::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// Untagged: a picture is the object with an `image` field. It's tried first,
// since every text-run field has a default and would match anything.
#[serde(untagged)]
pub enum Run {
    Image(Image),
    Note(Note),
    Drawing(Drawing),
    Text(TextRun),
}

/// A shape or a chart in the line, drawn rather than a picture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Drawing {
    pub drawing: Figure,
    /// Points on the page.
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Figure {
    Shape {
        shape: crate::present::model::Shape,
        fill: String,
    },
    /// Labels, one list of values per series, as in a slide's chart.
    Chart {
        chart: crate::sheets::model::ChartKind,
        names: Vec<String>,
        labels: Vec<String>,
        series: Vec<Vec<f64>>,
    },
}

impl Figure {
    pub fn sample_chart() -> Figure {
        match crate::present::model::Kind::sample_chart() {
            crate::present::model::Kind::Chart { chart, names, labels, series } => Figure::Chart { chart, names, labels, series },
            _ => unreachable!("the sample is a chart"),
        }
    }
}

/// A footnote: its mark sits in the text, its words at the foot of the page.
/// An endnote's words go at the end of the document instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Note {
    pub note: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub end: bool,
}

/// An endnote's mark: i, ii, iii…
pub fn endnote_label(n: usize) -> String {
    roman(n)
}

impl Run {
    pub fn text(t: &str) -> Run {
        Run::Text(TextRun { text: t.into(), ..Default::default() })
    }
    pub fn styled(t: &str, f: impl FnOnce(&mut TextRun)) -> Run {
        let mut r = TextRun { text: t.into(), ..Default::default() };
        f(&mut r);
        Run::Text(r)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TextRun {
    pub text: String,
    #[serde(skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub underline: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub strike: bool,
    /// Raised and smaller (never with `subscript`).
    #[serde(skip_serializing_if = "is_false")]
    pub superscript: bool,
    /// Lowered and smaller.
    #[serde(skip_serializing_if = "is_false")]
    pub subscript: bool,
    /// The web address or file this text links to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    /// Points.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    /// `#rrggbb`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Highlight behind the text, `#rrggbb`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight: Option<String>,
    /// The comment this text is part of.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<u32>,
    /// Added while tracking changes: the revision's id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inserted: Option<u32>,
    /// Deleted while tracking changes: still shown, struck through, until accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted: Option<u32>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl TextRun {
    /// Same formatting, ignoring the text.
    pub fn same_format(&self, o: &TextRun) -> bool {
        self.bold == o.bold
            && self.italic == o.italic
            && self.underline == o.underline
            && self.strike == o.strike
            && self.superscript == o.superscript
            && self.subscript == o.subscript
            && self.link == o.link
            && self.font == o.font
            && self.size == o.size
            && self.color == o.color
            && self.highlight == o.highlight
            && self.comment == o.comment
            && self.inserted == o.inserted
            && self.deleted == o.deleted
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Image {
    /// The picture's file name under `media/`.
    pub image: String,
    /// Width on the page, in points. Height follows the picture's proportions.
    pub width: f64,
    /// Set at this side of its paragraph with the text wrapping beside it;
    /// otherwise the picture sits in the line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<Side>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Table {
    /// Rows of cells; every row has the same number of cells.
    pub rows: Vec<Vec<String>>,
    /// The first row is a header (bold, shaded).
    pub header: bool,
}

impl Table {
    pub fn new(rows: usize, cols: usize) -> Table {
        Table { rows: vec![vec![String::new(); cols.max(1)]; rows.max(1)], header: true }
    }
    pub fn cols(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(1).max(1)
    }
}

impl Document {
    /// How text in a paragraph of style `s` looks here.
    pub fn look(&self, s: Style) -> Look {
        look_of(&self.styles, s)
    }

    /// Names of the pictures the text uses.
    pub fn image_names(&self) -> Vec<String> {
        let mut out = Vec::new();
        for b in &self.blocks {
            if let Block::Paragraph(p) = b {
                for r in &p.runs {
                    if let Run::Image(i) = r {
                        out.push(i.image.clone());
                    }
                }
            }
        }
        out
    }

    /// The headings for a table of contents: (level 0–3, text, block index).
    pub fn headings(&self) -> Vec<(usize, String, usize)> {
        self.blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| {
                let Block::Paragraph(p) = b else { return None };
                let level = match p.style {
                    Style::Title => 0,
                    Style::H1 => 1,
                    Style::H2 => 2,
                    Style::H3 => 3,
                    _ => return None,
                };
                let text: String =
                    p.runs.iter().filter_map(|r| if let Run::Text(t) = r { Some(t.text.as_str()) } else { None }).collect();
                (!text.trim().is_empty()).then(|| (level, text.trim().to_string(), i))
            })
            .collect()
    }

    /// Drop comments no text points at any more.
    pub fn prune_comments(&mut self) {
        let used: Vec<u32> = self
            .blocks
            .iter()
            .filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None })
            .flat_map(|p| p.runs.iter())
            .filter_map(|r| if let Run::Text(t) = r { t.comment } else { None })
            .collect();
        self.comments.retain(|c| used.contains(&c.id));
        let changed: Vec<u32> = self
            .blocks
            .iter()
            .filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None })
            .flat_map(|p| p.runs.iter())
            .filter_map(|r| if let Run::Text(t) = r { t.inserted.or(t.deleted) } else { None })
            .collect();
        self.revisions.retain(|r| changed.contains(&r.id));
    }

    /// Merge neighbouring runs with the same formatting and drop empty ones.
    pub fn tidy(&mut self) {
        for b in &mut self.blocks {
            if let Block::Paragraph(p) = b {
                let mut runs: Vec<Run> = Vec::with_capacity(p.runs.len());
                for r in p.runs.drain(..) {
                    match (runs.last_mut(), r) {
                        (_, Run::Text(t)) if t.text.is_empty() => {}
                        (Some(Run::Text(prev)), Run::Text(t)) if prev.same_format(&t) => prev.text.push_str(&t.text),
                        (_, r) => runs.push(r),
                    }
                }
                p.runs = runs;
            }
        }
        if self.blocks.is_empty() {
            self.blocks.push(Block::Paragraph(Paragraph::default()));
        }
    }
}

/// A list item's marker: bullets change shape with depth; numbers go 1. a. i.
pub fn list_label(list: List, indent: u8, n: usize) -> String {
    match list {
        List::Bullet => ["•", "◦", "▪"][indent as usize % 3].to_string(),
        List::Number => match indent % 3 {
            0 => format!("{n}."),
            1 => format!("{}.", letters(n)),
            _ => format!("{}.", roman(n)),
        },
    }
}

fn letters(mut n: usize) -> String {
    let mut s = Vec::new();
    while n > 0 {
        n -= 1;
        s.push((b'a' + (n % 26) as u8) as char);
        n /= 26;
    }
    s.iter().rev().collect()
}

fn roman(mut n: usize) -> String {
    const TABLE: [(usize, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut s = String::new();
    for (v, r) in TABLE {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

/// Number each numbered paragraph: the count restarts after anything that isn't a
/// numbered item at the same depth or deeper. `items` is (list, indent) per paragraph.
pub fn numbering(items: &[(Option<List>, u8)]) -> Vec<usize> {
    let mut out = vec![0; items.len()];
    // counters[depth]
    let mut counters = [0usize; MAX_INDENT as usize + 1];
    for (i, (list, indent)) in items.iter().enumerate() {
        let d = (*indent).min(MAX_INDENT) as usize;
        match list {
            Some(List::Number) => {
                counters[d] += 1;
                for c in counters.iter_mut().skip(d + 1) {
                    *c = 0;
                }
                out[i] = counters[d];
            }
            Some(List::Bullet) => {
                for c in counters.iter_mut().skip(d + 1) {
                    *c = 0;
                }
                counters[d] = 0;
            }
            None => counters = [0; MAX_INDENT as usize + 1],
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_lists() {
        let n = Some(List::Number);
        let items = [(n, 0), (n, 0), (n, 1), (n, 1), (n, 0), (None, 0), (n, 0)];
        assert_eq!(numbering(&items), vec![1, 2, 1, 2, 3, 0, 1]);
        assert_eq!(list_label(List::Number, 1, 2), "b.");
        assert_eq!(list_label(List::Number, 2, 4), "iv.");
        assert_eq!(list_label(List::Number, 1, 27), "aa.");
    }

    #[test]
    fn tidy_merges_runs() {
        let mut d = Document {
            blocks: vec![Block::Paragraph(Paragraph {
                runs: vec![Run::text("a"), Run::text("b"), Run::styled("c", |r| r.bold = true), Run::text("")],
                ..Default::default()
            })],
            ..Default::default()
        };
        d.tidy();
        let Block::Paragraph(p) = &d.blocks[0] else { panic!() };
        assert_eq!(p.runs.len(), 2);
        assert_eq!(p.plain_text(), "abc");
    }

    #[test]
    fn json_round_trip() {
        let d = Document {
            blocks: vec![
                Block::Paragraph(Paragraph::text(Style::H1, "Hello")),
                Block::Paragraph(Paragraph {
                    list: Some(List::Number),
                    runs: vec![
                        Run::styled("x", |r| r.color = Some("#ff0000".into())),
                        Run::Image(Image { image: "a.png".into(), width: 100.0, wrap: Some(Side::Left) }),
                    ],
                    ..Default::default()
                }),
                Block::Table(Table::new(2, 3)),
                Block::PageBreak,
                Block::Contents,
                Block::Paragraph(Paragraph {
                    runs: vec![
                        Run::styled("noted", |r| r.comment = Some(1)),
                        Run::Note(Note { note: "A footnote.".into(), end: false }),
                    ],
                    ..Default::default()
                }),
            ],
            comments: vec![Comment { id: 1, author: "Ken".into(), text: "Check".into(), ..Default::default() }],
            page: PageSetup { columns: 2, ..Default::default() },
            ..Default::default()
        };
        let text = serde_json::to_string(&d).unwrap();
        let back: Document = serde_json::from_str(&text).unwrap();
        assert_eq!(d, back);
    }
}
