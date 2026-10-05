//! The text buffer and the document model, each made from the other.
//!
//! Formatting lives in tags. Character tags: `b`, `i`, `u`, `s`, `font:<family>`,
//! `size:<points>`, `fg:#rrggbb`, `bg:#rrggbb`, `cm:<comment id>`, and tracked
//! changes `ins:<revision id>` / `del:<revision id>`. Paragraph tags cover a whole line,
//! newline included: `style:<id>`, `align:<id>`, `list:<bullet|number>`,
//! `indent:<n>`, and `lm:<px>` (the left margin those add up to). The buffer
//! always ends with a newline, so every paragraph, even an empty last one, has a
//! character to carry its tags.

use super::model::*;
use super::picture::{PX, Picture};
use super::marks::{Mark, MarkKind, Marks};
use super::table::TableWidget;
use gtk::prelude::*;
use std::collections::BTreeMap;
use std::rc::Rc;

pub const PARA_PREFIXES: [&str; 5] = ["style:", "align:", "list:", "indent:", "lm:"];
pub const CHAR_PREFIXES: [&str; 7] = ["font:", "size:", "fg:", "bg:", "cm:", "ins:", "del:"];
pub const CHAR_FLAGS: [&str; 4] = ["b", "i", "u", "s"];

/// The table widgets in the text, by their anchor.
pub type Tables = Rc<std::cell::RefCell<Vec<(gtk::TextChildAnchor, Rc<TableWidget>)>>>;

/// Create the fixed tags. Paragraph styles come first, so a run's own size or
/// font (created later, so higher priority) wins over its heading's.
pub fn setup_tags(buffer: &gtk::TextBuffer) {
    let table = buffer.tag_table();
    for style in Style::ALL {
        if style == Style::Normal {
            continue;
        }
        let (size, above, below, bold, italic) = style.look();
        let t = gtk::TextTag::new(Some(&format!("style:{}", style.id())));
        t.set_size_points(size);
        t.set_pixels_above_lines((above * PX) as i32);
        t.set_pixels_below_lines((below * PX) as i32);
        if bold {
            t.set_weight(650);
        }
        if italic {
            t.set_style(gtk::pango::Style::Italic);
            t.set_foreground(Some("#4a5060"));
        }
        table.add(&t);
    }
    for (id, j) in
        [("center", gtk::Justification::Center), ("right", gtk::Justification::Right), ("justify", gtk::Justification::Fill)]
    {
        let t = gtk::TextTag::new(Some(&format!("align:{id}")));
        t.set_justification(j);
        table.add(&t);
    }
    for id in ["bullet", "number"] {
        table.add(&gtk::TextTag::new(Some(&format!("list:{id}"))));
    }
    for n in 1..=MAX_INDENT {
        table.add(&gtk::TextTag::new(Some(&format!("indent:{n}"))));
    }
    let b = gtk::TextTag::new(Some("b"));
    b.set_weight(700);
    table.add(&b);
    let i = gtk::TextTag::new(Some("i"));
    i.set_style(gtk::pango::Style::Italic);
    table.add(&i);
    let u = gtk::TextTag::new(Some("u"));
    u.set_underline(gtk::pango::Underline::Single);
    table.add(&u);
    let s = gtk::TextTag::new(Some("s"));
    s.set_strikethrough(true);
    table.add(&s);
    // Find matches: drawn over everything, never saved.
    let f = gtk::TextTag::new(Some("find"));
    f.set_background(Some("#ffe066"));
    table.add(&f);
}

/// A named tag, created on first use (fonts, sizes, colours, margins).
pub fn tag(buffer: &gtk::TextBuffer, name: &str) -> gtk::TextTag {
    let table = buffer.tag_table();
    if let Some(t) = table.lookup(name) {
        return t;
    }
    let t = gtk::TextTag::new(Some(name));
    if let Some(v) = name.strip_prefix("font:") {
        t.set_family(Some(v));
    } else if let Some(v) = name.strip_prefix("size:") {
        t.set_size_points(v.parse().unwrap_or(DEFAULT_SIZE));
    } else if let Some(v) = name.strip_prefix("fg:") {
        t.set_foreground(Some(v));
    } else if let Some(v) = name.strip_prefix("bg:") {
        t.set_background(Some(v));
    } else if let Some(v) = name.strip_prefix("lm:") {
        t.set_left_margin(v.parse().unwrap_or(0));
    } else if let Some(v) = name.strip_prefix("wl:") {
        t.set_left_margin(v.parse().unwrap_or(0));
    } else if let Some(v) = name.strip_prefix("wr:") {
        t.set_right_margin(v.parse().unwrap_or(0));
    } else if let Some(v) = name.strip_prefix("wb:") {
        t.set_pixels_below_lines(v.parse().unwrap_or(0));
    } else if name.starts_with("cm:") {
        // Commented text: a soft yellow, under any highlight of its own.
        t.set_background(Some("#fff1b8"));
    } else if name.starts_with("ins:") {
        t.set_foreground(Some(INSERTED_INK));
        t.set_underline(gtk::pango::Underline::Single);
    } else if name.starts_with("del:") {
        t.set_foreground(Some(DELETED_INK));
        t.set_strikethrough(true);
    }
    table.add(&t);
    if name.starts_with("cm:") {
        t.set_priority(0);
    } else if ["ins:", "del:", "wl:", "wr:", "wb:"].iter().any(|p| name.starts_with(p)) {
        // Changes show over the text's own colour.
        t.set_priority(table.size() - 1);
    }
    t
}

/// How tracked changes look, on screen and in print.
pub const INSERTED_INK: &str = "#1a7f37";
pub const DELETED_INK: &str = "#b42318";

fn tag_names(iter: &gtk::TextIter) -> Vec<String> {
    iter.tags().iter().filter_map(|t| t.name().map(|n| n.to_string())).collect()
}

// ---------- Paragraphs ----------

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ParaProps {
    pub style: Style,
    pub align: Align,
    pub list: Option<List>,
    pub indent: u8,
}

impl ParaProps {
    pub fn of(p: &Paragraph) -> ParaProps {
        ParaProps { style: p.style, align: p.align, list: p.list, indent: p.indent }
    }
    /// The left margin in pixels: indent steps, plus room for a list marker or a quote.
    pub fn margin_px(&self) -> i32 {
        let steps = self.indent as f64 + if self.list.is_some() || self.style == Style::Quote { 1.0 } else { 0.0 };
        (steps * INDENT_STEP * PX).round() as i32
    }
}

/// The paragraph properties at the start of the line holding `iter`.
pub fn para_props(buffer: &gtk::TextBuffer, iter: &gtk::TextIter) -> ParaProps {
    let start = buffer.iter_at_line(iter.line()).unwrap_or_else(|| buffer.start_iter());
    let mut p = ParaProps::default();
    for name in tag_names(&start) {
        if let Some(v) = name.strip_prefix("style:") {
            p.style = Style::from_id(v);
        } else if let Some(v) = name.strip_prefix("align:") {
            p.align = Align::from_id(v);
        } else if let Some(v) = name.strip_prefix("list:") {
            p.list = Some(if v == "number" { List::Number } else { List::Bullet });
        } else if let Some(v) = name.strip_prefix("indent:") {
            p.indent = v.parse().unwrap_or(0);
        }
    }
    p
}

/// The range of whole lines from `first` to `last`, newline included.
pub fn line_range(buffer: &gtk::TextBuffer, first: i32, last: i32) -> (gtk::TextIter, gtk::TextIter) {
    let start = buffer.iter_at_line(first).unwrap_or_else(|| buffer.start_iter());
    let mut end = buffer.iter_at_line(last).unwrap_or_else(|| buffer.end_iter());
    if !end.ends_line() || end.char() != '\n' {
        end.forward_to_line_end();
    }
    end.forward_char();
    (start, end)
}

fn remove_para_tags(buffer: &gtk::TextBuffer, start: &gtk::TextIter, end: &gtk::TextIter) {
    let mut names = Vec::new();
    buffer.tag_table().foreach(|t| {
        if let Some(n) = t.name()
            && PARA_PREFIXES.iter().any(|p| n.starts_with(p))
        {
            names.push(n.to_string());
        }
    });
    for n in names {
        buffer.remove_tag_by_name(&n, start, end);
    }
}

/// Set the properties of lines `first..=last`.
pub fn set_para(buffer: &gtk::TextBuffer, first: i32, last: i32, p: ParaProps) {
    let (start, end) = line_range(buffer, first, last);
    remove_para_tags(buffer, &start, &end);
    if p.style != Style::Normal {
        buffer.apply_tag_by_name(&format!("style:{}", p.style.id()), &start, &end);
    }
    if p.align != Align::Left {
        buffer.apply_tag_by_name(&format!("align:{}", p.align.id()), &start, &end);
    }
    if let Some(l) = p.list {
        buffer.apply_tag_by_name(if l == List::Number { "list:number" } else { "list:bullet" }, &start, &end);
    }
    if p.indent > 0 {
        buffer.apply_tag_by_name(&format!("indent:{}", p.indent.min(MAX_INDENT)), &start, &end);
    }
    let m = p.margin_px();
    if m > 0 {
        buffer.apply_tag(&tag(buffer, &format!("lm:{m}")), &start, &end);
    }
}

/// Change some of the properties of lines `first..=last`, each keeping the rest.
pub fn update_para(buffer: &gtk::TextBuffer, first: i32, last: i32, f: impl Fn(&mut ParaProps)) {
    for line in first..=last {
        let it = buffer.iter_at_line(line).unwrap_or_else(|| buffer.start_iter());
        let mut p = para_props(buffer, &it);
        f(&mut p);
        set_para(buffer, line, line, p);
    }
}

/// After an edit, each line's paragraph tags must cover the whole line. A merged
/// line takes the properties of its first character.
pub fn normalize_lines(buffer: &gtk::TextBuffer, first: i32, last: i32) {
    let last = last.min(buffer.line_count() - 1);
    for line in first.max(0)..=last {
        let it = buffer.iter_at_line(line).unwrap_or_else(|| buffer.start_iter());
        let p = para_props(buffer, &it);
        set_para(buffer, line, line, p);
    }
}

// ---------- Characters ----------

/// The formatting of the character after `iter` (text left empty).
pub fn char_format(iter: &gtk::TextIter) -> TextRun {
    let mut r = TextRun::default();
    for name in tag_names(iter) {
        match name.as_str() {
            "b" => r.bold = true,
            "i" => r.italic = true,
            "u" => r.underline = true,
            "s" => r.strike = true,
            n => {
                if let Some(v) = n.strip_prefix("font:") {
                    r.font = Some(v.into());
                } else if let Some(v) = n.strip_prefix("size:") {
                    r.size = v.parse().ok();
                } else if let Some(v) = n.strip_prefix("fg:") {
                    r.color = Some(v.into());
                } else if let Some(v) = n.strip_prefix("bg:") {
                    r.highlight = Some(v.into());
                } else if let Some(v) = n.strip_prefix("cm:") {
                    r.comment = v.parse().ok();
                } else if let Some(v) = n.strip_prefix("ins:") {
                    r.inserted = v.parse().ok();
                } else if let Some(v) = n.strip_prefix("del:") {
                    r.deleted = v.parse().ok();
                }
            }
        }
    }
    r
}

/// The tag names that give a run its formatting.
pub fn format_tags(r: &TextRun) -> Vec<String> {
    let mut out = Vec::new();
    for (on, name) in [(r.bold, "b"), (r.italic, "i"), (r.underline, "u"), (r.strike, "s")] {
        if on {
            out.push(name.to_string());
        }
    }
    if let Some(f) = &r.font {
        out.push(format!("font:{f}"));
    }
    if let Some(s) = r.size {
        out.push(format!("size:{}", fmt_size(s)));
    }
    if let Some(c) = &r.color {
        out.push(format!("fg:{c}"));
    }
    if let Some(c) = &r.highlight {
        out.push(format!("bg:{c}"));
    }
    if let Some(c) = r.comment {
        out.push(format!("cm:{c}"));
    }
    if let Some(c) = r.inserted {
        out.push(format!("ins:{c}"));
    }
    if let Some(c) = r.deleted {
        out.push(format!("del:{c}"));
    }
    out
}

pub fn fmt_size(s: f64) -> String {
    let s = (s * 2.0).round() / 2.0;
    if s.fract() == 0.0 { format!("{}", s as i64) } else { format!("{s}") }
}

pub fn remove_char_tags(buffer: &gtk::TextBuffer, start: &gtk::TextIter, end: &gtk::TextIter, prefixes: &[&str]) {
    let mut names = Vec::new();
    buffer.tag_table().foreach(|t| {
        if let Some(n) = t.name()
            && prefixes.iter().any(|p| if p.ends_with(':') { n.starts_with(p) } else { n.as_str() == *p })
        {
            names.push(n.to_string());
        }
    });
    for n in names {
        buffer.remove_tag_by_name(&n, start, end);
    }
}

/// Give `start..end` exactly the formatting of `r`.
pub fn set_format(buffer: &gtk::TextBuffer, start: &gtk::TextIter, end: &gtk::TextIter, r: &TextRun) {
    let mut all: Vec<&str> = CHAR_FLAGS.to_vec();
    all.extend(CHAR_PREFIXES);
    remove_char_tags(buffer, start, end, &all);
    for name in format_tags(r) {
        buffer.apply_tag(&tag(buffer, &name), start, end);
    }
}

// ---------- Model → buffer ----------

pub struct Loader<'a> {
    pub buffer: &'a gtk::TextBuffer,
    pub view: &'a gtk::TextView,
    pub media: &'a BTreeMap<String, Vec<u8>>,
    pub tables: &'a Tables,
    pub width_px: i32,
    /// Builds the widget for a table (wires up its change and delete callbacks).
    pub make_table: &'a dyn Fn(&Table) -> Rc<TableWidget>,
    pub marks: &'a Marks,
    /// Builds a page break, table of contents or footnote mark.
    pub make_mark: &'a dyn Fn(MarkKind) -> (Mark, gtk::Widget),
}

impl Loader<'_> {
    /// Add a mark at the end of the text.
    fn mark(&self, kind: MarkKind) -> gtk::Widget {
        let b = self.buffer;
        let mut end = b.end_iter();
        let anchor = b.create_child_anchor(&mut end);
        let (mark, widget) = (self.make_mark)(kind);
        self.view.add_child_at_anchor(&widget, &anchor);
        self.marks.borrow_mut().push((anchor, mark, widget.clone()));
        widget
    }

    /// Replace the whole buffer with `blocks`.
    pub fn load(&self, blocks: &[Block]) {
        let b = self.buffer;
        self.tables.borrow_mut().clear();
        self.marks.borrow_mut().clear();
        b.set_text("");
        for block in blocks {
            let line = b.end_iter().line();
            match block {
                Block::Paragraph(p) => {
                    for run in &p.runs {
                        match run {
                            Run::Text(t) => {
                                let mut end = b.end_iter();
                                let names = format_tags(t);
                                let tags: Vec<gtk::TextTag> = names.iter().map(|n| tag(b, n)).collect();
                                let refs: Vec<&gtk::TextTag> = tags.iter().collect();
                                // A run never holds a newline: those split paragraphs.
                                b.insert_with_tags(&mut end, &t.text.replace(['\n', '\r'], " "), &refs);
                            }
                            Run::Image(img) => {
                                if let Some(bytes) = self.media.get(&img.image)
                                    && let Some(pic) = Picture::new(&img.image, bytes, img.width)
                                {
                                    pic.set_wrap(img.wrap);
                                    let mut end = b.end_iter();
                                    b.insert_paintable(&mut end, &pic);
                                }
                            }
                            Run::Note(n) => {
                                self.mark(MarkKind::Note(&n.note));
                            }
                        }
                    }
                    let mut end = b.end_iter();
                    b.insert(&mut end, "\n");
                    set_para(b, line, line, ParaProps::of(p));
                }
                Block::Table(t) => {
                    let mut end = b.end_iter();
                    let anchor = b.create_child_anchor(&mut end);
                    let w = (self.make_table)(t);
                    w.set_width(self.width_px);
                    self.view.add_child_at_anchor(&w.grid, &anchor);
                    self.tables.borrow_mut().push((anchor, w));
                    let mut end = b.end_iter();
                    b.insert(&mut end, "\n");
                }
                Block::PageBreak | Block::Contents => {
                    let w = self.mark(if matches!(block, Block::PageBreak) { MarkKind::PageBreak } else { MarkKind::Contents });
                    w.set_size_request(self.width_px, -1);
                    let mut end = b.end_iter();
                    b.insert(&mut end, "\n");
                }
            }
        }
        if b.char_count() == 0 {
            b.set_text("\n");
        }
        b.place_cursor(&b.start_iter());
    }
}

// ---------- Buffer → model ----------

pub fn read(buffer: &gtk::TextBuffer, tables: &Tables, marks: &Marks) -> Vec<Block> {
    let mut blocks = Vec::new();
    // The last line is the empty one after the final newline.
    let lines = buffer.line_count() - 1;
    for line in 0..lines.max(1) {
        let Some(start) = buffer.iter_at_line(line) else { break };
        let mut end = start;
        if !end.ends_line() {
            end.forward_to_line_end();
        }
        if let Some(anchor) = start.child_anchor()
            && let Some((_, w)) = tables.borrow().iter().find(|(a, _)| *a == anchor)
        {
            blocks.push(Block::Table(w.model()));
            continue;
        }
        if let Some(anchor) = start.child_anchor() {
            match marks.borrow().iter().find(|(a, _, _)| *a == anchor).map(|(_, m, _)| m) {
                Some(Mark::PageBreak) => {
                    blocks.push(Block::PageBreak);
                    continue;
                }
                Some(Mark::Contents(_)) => {
                    blocks.push(Block::Contents);
                    continue;
                }
                _ => {}
            }
        }
        let props = para_props(buffer, &start);
        let mut p =
            Paragraph { style: props.style, align: props.align, list: props.list, indent: props.indent, runs: Vec::new() };
        let mut it = start;
        while it < end {
            let mut next = it;
            next.forward_to_tag_toggle(None::<&gtk::TextTag>);
            if next > end {
                next = end;
            }
            if next == it {
                next.forward_char();
            }
            let format = char_format(&it);
            let text = buffer.slice(&it, &next, true);
            let mut chunk = String::new();
            let base = it.offset();
            for (i, ch) in text.chars().enumerate() {
                if ch == '\u{FFFC}' {
                    let at = buffer.iter_at_offset(base + i as i32);
                    if let Some(pic) = at.paintable().and_then(|p| p.downcast::<Picture>().ok()) {
                        if !chunk.is_empty() {
                            p.runs.push(Run::Text(TextRun { text: std::mem::take(&mut chunk), ..format.clone() }));
                        }
                        p.runs.push(Run::Image(Image { image: pic.name(), width: pic.width_pt(), wrap: pic.wrap() }));
                    } else if let Some(anchor) = at.child_anchor()
                        && let Some(Mark::Note(n)) = marks.borrow().iter().find(|(a, _, _)| *a == anchor).map(|(_, m, _)| m)
                    {
                        if !chunk.is_empty() {
                            p.runs.push(Run::Text(TextRun { text: std::mem::take(&mut chunk), ..format.clone() }));
                        }
                        p.runs.push(Run::Note(super::model::Note { note: n.text.borrow().clone() }));
                    }
                } else {
                    chunk.push(ch);
                }
            }
            if !chunk.is_empty() {
                p.runs.push(Run::Text(TextRun { text: chunk, ..format }));
            }
            it = next;
        }
        blocks.push(Block::Paragraph(p));
    }
    let mut doc = Document { blocks, ..Default::default() };
    doc.tidy();
    doc.blocks
}
