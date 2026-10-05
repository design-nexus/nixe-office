//! Lay a document out as pages, for PDF export, printing and the page count.
//! Everything is in points: the Pango context runs at 72 dpi.

use super::model::*;
use crate::print::Pages;
use gtk::pango;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

const INK: (f64, f64, f64) = (0x20 as f64 / 255.0, 0x23 as f64 / 255.0, 0x29 as f64 / 255.0);
const CELL_PAD: f64 = 5.0;

enum Item {
    /// Some lines of a paragraph. `top` is where line `lines.start` begins on the page.
    Lines {
        layout: Rc<pango::Layout>,
        lines: std::ops::Range<usize>,
        x: f64,
        top: f64,
        marker: Option<Rc<pango::Layout>>,
    },
    Picture {
        surface: cairo::ImageSurface,
        x: f64,
        top: f64,
        w: f64,
        h: f64,
    },
    Row {
        cells: Vec<Rc<pango::Layout>>,
        x: f64,
        top: f64,
        col_w: f64,
        h: f64,
        header: bool,
    },
}

/// One page: what's in its columns, and the footnotes at the foot of each column.
#[derive(Default)]
struct Page {
    items: Vec<Item>,
    /// (column, layout) per footnote, in order.
    notes: Vec<(usize, Rc<pango::Layout>)>,
}

pub struct Rendered {
    setup: PageSetup,
    pages: Vec<Page>,
    ctx: pango::Context,
    /// The page (from 1) each heading starts on, by block index.
    heading_pages: HashMap<usize, usize>,
}

/// A piece of a paragraph's text: a run, or a footnote's number.
enum Piece<'a> {
    Text(&'a TextRun),
    Mark(usize),
}

const NOTE_SIZE: f64 = 9.0;
/// Room for the rule above a column's footnotes.
const NOTE_RULE: f64 = 10.0;

fn rgb(hex: &str) -> Option<(u16, u16, u16)> {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let p = |i: usize| u16::from_str_radix(&h[i..i + 2], 16).ok().map(|v| v * 257);
    Some((p(0)?, p(2)?, p(4)?))
}

/// A Pango layout for some text runs in a paragraph of `style`.
fn text_layout(ctx: &pango::Context, runs: &[&TextRun], style: Style, align: Align, width: f64) -> pango::Layout {
    let pieces: Vec<Piece> = runs.iter().map(|r| Piece::Text(r)).collect();
    pieces_layout(ctx, &pieces, style, align, width).0
}

/// A layout for runs and footnote marks; also where each mark is: (byte index, number).
fn pieces_layout(
    ctx: &pango::Context,
    pieces: &[Piece],
    style: Style,
    align: Align,
    width: f64,
) -> (pango::Layout, Vec<(usize, usize)>) {
    let layout = pango::Layout::new(ctx);
    let (size, _, _, bold, italic) = style.look();
    let mut font = pango::FontDescription::from_string(DEFAULT_FONT);
    font.set_size((size * pango::SCALE as f64) as i32);
    if bold {
        font.set_weight(pango::Weight::Semibold);
    }
    if italic {
        font.set_style(pango::Style::Italic);
    }
    layout.set_font_description(Some(&font));
    layout.set_width((width * pango::SCALE as f64) as i32);
    layout.set_wrap(pango::WrapMode::WordChar);
    layout.set_alignment(match align {
        Align::Center => pango::Alignment::Center,
        Align::Right => pango::Alignment::Right,
        _ => pango::Alignment::Left,
    });
    layout.set_justify(align == Align::Justify);
    let mut text = String::new();
    let attrs = pango::AttrList::new();
    let mut marks = Vec::new();
    for piece in pieces {
        let r = match piece {
            Piece::Text(r) => *r,
            Piece::Mark(n) => {
                // A small raised number.
                let start = text.len() as u32;
                marks.push((text.len(), *n));
                text.push_str(&n.to_string());
                let end = text.len() as u32;
                for mut a in [
                    pango::AttrSize::new((size * 0.62 * pango::SCALE as f64) as i32).into(),
                    pango::AttrInt::new_rise((size * 0.38 * pango::SCALE as f64) as i32).into(),
                ] {
                    let a: &mut pango::Attribute = &mut a;
                    a.set_start_index(start);
                    a.set_end_index(end);
                    attrs.insert(a.clone());
                }
                continue;
            }
        };
        let start = text.len() as u32;
        text.push_str(&r.text);
        let end = text.len() as u32;
        let add = |mut a: pango::Attribute| {
            a.set_start_index(start);
            a.set_end_index(end);
            attrs.insert(a);
        };
        if r.bold {
            add(pango::AttrInt::new_weight(pango::Weight::Bold).into());
        }
        if r.italic {
            add(pango::AttrInt::new_style(pango::Style::Italic).into());
        }
        if r.underline {
            add(pango::AttrInt::new_underline(pango::Underline::Single).into());
        }
        if r.strike {
            add(pango::AttrInt::new_strikethrough(true).into());
        }
        if let Some(f) = &r.font {
            add(pango::AttrString::new_family(f).into());
        }
        if let Some(s) = r.size {
            add(pango::AttrSize::new((s * pango::SCALE as f64) as i32).into());
        }
        let color = r.color.as_deref().or(if style == Style::Quote { Some("#4a5060") } else { None });
        if let Some((cr, cg, cb)) = color.and_then(rgb) {
            add(pango::AttrColor::new_foreground(cr, cg, cb).into());
        }
        if let Some((cr, cg, cb)) = r.highlight.as_deref().and_then(rgb) {
            add(pango::AttrColor::new_background(cr, cg, cb).into());
        }
    }
    layout.set_text(&text);
    layout.set_attributes(Some(&attrs));
    (layout, marks)
}

/// The top of each line and the layout's whole height, in points.
fn line_tops(layout: &pango::Layout) -> (Vec<f64>, f64) {
    let mut tops = Vec::new();
    let mut it = layout.iter();
    loop {
        let (_, logical) = it.line_extents();
        tops.push(logical.y() as f64 / pango::SCALE as f64);
        if !it.next_line() {
            break;
        }
    }
    let (_, h) = layout.size();
    (tops, h as f64 / pango::SCALE as f64)
}

fn picture_surface(bytes: &[u8]) -> Option<cairo::ImageSurface> {
    cairo::ImageSurface::create_from_png(&mut std::io::Cursor::new(bytes)).ok()
}

struct Pager<'a> {
    setup: &'a PageSetup,
    pages: Vec<Page>,
    y: f64,
    col: usize,
    /// Height the current column's footnotes take at its foot.
    foot: f64,
}

impl Pager<'_> {
    fn room(&self) -> f64 {
        self.setup.content_height() - self.foot - self.y
    }
    fn empty(&self) -> bool {
        self.y == 0.0 && self.col == 0 && self.pages.last().is_none_or(|p| p.items.is_empty())
    }
    /// Text runs over: into the next column, or onto a new page.
    fn next_column(&mut self) {
        if self.col + 1 < self.setup.columns() {
            self.col += 1;
            self.y = 0.0;
            self.foot = 0.0;
        } else {
            self.new_page();
        }
    }
    fn new_page(&mut self) {
        self.pages.push(Page::default());
        self.y = 0.0;
        self.col = 0;
        self.foot = 0.0;
    }
    /// Place an item in the current column.
    fn push(&mut self, mut item: Item) {
        let dx = self.col as f64 * (self.setup.column_width() + self.setup.column_gap.max(0.0));
        match &mut item {
            Item::Lines { x, .. } | Item::Picture { x, .. } | Item::Row { x, .. } => *x += dx,
        }
        self.pages.last_mut().expect("a page").items.push(item);
    }
    fn add_note(&mut self, layout: Rc<pango::Layout>, h: f64) {
        let page = self.pages.last_mut().expect("a page");
        if !page.notes.iter().any(|(c, _)| *c == self.col) {
            self.foot += NOTE_RULE;
        }
        page.notes.push((self.col, layout));
        self.foot += h;
    }
    /// Move down by `gap`, unless at the top of a column.
    fn space(&mut self, gap: f64) {
        if self.y > 0.0 {
            self.y = (self.y + gap).min(self.setup.content_height() - self.foot);
        }
    }
}

pub fn paginate(doc: &Document, media: &BTreeMap<String, Vec<u8>>) -> Rendered {
    let first = paginate_with(doc, media, &HashMap::new());
    // A table of contents needs the pages the headings landed on; laying out
    // again with them can move things by a page, so once more settles it.
    if !doc.blocks.iter().any(|b| matches!(b, Block::Contents)) {
        return first;
    }
    let second = paginate_with(doc, media, &first.heading_pages);
    if second.heading_pages == first.heading_pages {
        return second;
    }
    let pages = second.heading_pages.clone();
    paginate_with(doc, media, &pages)
}

fn paginate_with(doc: &Document, media: &BTreeMap<String, Vec<u8>>, heading_pages: &HashMap<usize, usize>) -> Rendered {
    let ctx = crate::print::text_context();
    let setup = &doc.page;
    let width = setup.column_width();
    let mut pg = Pager { setup, pages: vec![Page::default()], y: 0.0, col: 0, foot: 0.0 };
    let mut found_pages = HashMap::new();
    let numbers = numbering(
        &doc.blocks
            .iter()
            .map(|b| match b {
                Block::Paragraph(p) => (p.list, p.indent),
                _ => (None, 0),
            })
            .collect::<Vec<_>>(),
    );
    let mut note_n = 0;

    for (bi, block) in doc.blocks.iter().enumerate() {
        match block {
            Block::Paragraph(p) => {
                let (size, above, below, _, _) = p.style.look();
                let steps = p.indent as f64 + if p.list.is_some() || p.style == Style::Quote { 1.0 } else { 0.0 };
                let left = steps * INDENT_STEP;
                pg.space(above);
                // Text runs between pictures make one layout each; pictures stand on their own lines.
                let mut segments: Vec<Result<Vec<Piece>, &Image>> = vec![Ok(Vec::new())];
                let mut notes_here: Vec<(usize, &str)> = Vec::new();
                for r in &p.runs {
                    let piece = match r {
                        Run::Text(t) => Piece::Text(t),
                        Run::Note(n) => {
                            note_n += 1;
                            notes_here.push((note_n, n.note.as_str()));
                            Piece::Mark(note_n)
                        }
                        Run::Image(i) => {
                            segments.push(Err(i));
                            continue;
                        }
                    };
                    match segments.last_mut() {
                        Some(Ok(v)) => v.push(piece),
                        _ => segments.push(Ok(vec![piece])),
                    }
                }
                segments.retain(|s| !matches!(s, Ok(v) if v.is_empty()));
                if segments.is_empty() {
                    segments.push(Ok(Vec::new()));
                }
                let mut first = true;
                for seg in segments {
                    match seg {
                        Ok(pieces) => {
                            let (layout, marks) = pieces_layout(&ctx, &pieces, p.style, p.align, width - left);
                            let layout = Rc::new(layout);
                            let marker = match (first, p.list) {
                                (true, Some(list)) => {
                                    let label = list_label(list, p.indent, numbers[bi]);
                                    let first_size = pieces
                                        .iter()
                                        .find_map(|x| if let Piece::Text(r) = x { Some(r.size) } else { None })
                                        .flatten()
                                        .unwrap_or(size);
                                    Some(Rc::new(text_layout(
                                        &ctx,
                                        &[&TextRun { text: label, size: Some(first_size), ..Default::default() }],
                                        Style::Normal,
                                        Align::Right,
                                        INDENT_STEP * 0.7,
                                    )))
                                }
                                _ => None,
                            };
                            first = false;
                            let (tops, height) = line_tops(&layout);
                            // Each footnote's layout and height, by the line its mark is on.
                            let mut line_notes: Vec<Vec<(Rc<pango::Layout>, f64)>> = vec![Vec::new(); tops.len()];
                            for (idx, n) in &marks {
                                let text = notes_here.iter().find(|(k, _)| k == n).map(|(_, t)| *t).unwrap_or("");
                                let nl = note_layout(&ctx, *n, text, width);
                                let h = line_tops(&nl).1 + 2.0;
                                let (line, _) = layout.index_to_line_x(*idx as i32, false);
                                if let Some(v) = line_notes.get_mut(line.max(0) as usize) {
                                    v.push((Rc::new(nl), h));
                                }
                            }
                            // Keep a heading with the start of what follows it.
                            if p.style.is_heading() && pg.room() < height + 3.0 * DEFAULT_SIZE && pg.y > 0.0 {
                                pg.next_column();
                            }
                            let mut start = 0;
                            while start < tops.len() {
                                let base = tops[start];
                                let mut end = start;
                                let mut notes_h = 0.0;
                                let rule = |pg: &Pager| {
                                    let has = pg.pages.last().is_some_and(|p| p.notes.iter().any(|(c, _)| *c == pg.col));
                                    if has { 0.0 } else { NOTE_RULE }
                                };
                                while end < tops.len() {
                                    let bottom = if end + 1 < tops.len() { tops[end + 1] } else { height };
                                    let line_h: f64 = line_notes[end].iter().map(|n| n.1).sum();
                                    let extra = notes_h + line_h + if notes_h + line_h > 0.0 { rule(&pg) } else { 0.0 };
                                    if bottom - base + extra > pg.room() && (end > start || pg.y > 0.0) {
                                        break;
                                    }
                                    notes_h += line_h;
                                    end += 1;
                                }
                                if end == start {
                                    pg.next_column();
                                    continue;
                                }
                                let bottom = if end < tops.len() { tops[end] } else { height };
                                let m = if start == 0 { marker.clone() } else { None };
                                if start == 0 && p.style.is_heading() {
                                    found_pages.entry(bi).or_insert(pg.pages.len());
                                }
                                pg.push(Item::Lines { layout: layout.clone(), lines: start..end, x: left, top: pg.y, marker: m });
                                pg.y += bottom - base;
                                for line in &line_notes[start..end] {
                                    for (nl, h) in line {
                                        pg.add_note(nl.clone(), *h);
                                    }
                                }
                                start = end;
                                if start < tops.len() {
                                    pg.next_column();
                                }
                            }
                        }
                        Err(img) => {
                            let Some(surface) = media.get(&img.image).and_then(|b| picture_surface(b)) else { continue };
                            let w = img.width.min(width - left).max(1.0);
                            let h = w * surface.height() as f64 / surface.width().max(1) as f64;
                            let h = h.min(setup.content_height());
                            if pg.room() < h && pg.y > 0.0 {
                                pg.next_column();
                            }
                            let x = left
                                + match p.align {
                                    Align::Center => (width - left - w) / 2.0,
                                    Align::Right => width - left - w,
                                    _ => 0.0,
                                };
                            pg.push(Item::Picture { surface, x, top: pg.y, w, h });
                            pg.y += h;
                        }
                    }
                }
                pg.space(below);
            }
            Block::Table(t) => {
                let cols = t.cols();
                let col_w = width / cols as f64;
                pg.space(4.0);
                for (ri, row) in t.rows.iter().enumerate() {
                    let header = ri == 0 && t.header;
                    let cells: Vec<Rc<pango::Layout>> = (0..cols)
                        .map(|c| {
                            let text = row.get(c).cloned().unwrap_or_default();
                            let run = TextRun { text, bold: header, ..Default::default() };
                            Rc::new(text_layout(&ctx, &[&run], Style::Normal, Align::Left, col_w - 2.0 * CELL_PAD))
                        })
                        .collect();
                    let h = cells.iter().map(|l| line_tops(l).1).fold(DEFAULT_SIZE, f64::max) + 2.0 * CELL_PAD;
                    if pg.room() < h && pg.y > 0.0 {
                        pg.next_column();
                    }
                    pg.push(Item::Row { cells, x: 0.0, top: pg.y, col_w, h, header });
                    pg.y += h;
                }
                pg.space(10.0);
            }
            Block::PageBreak => {
                if !pg.empty() {
                    pg.new_page();
                }
            }
            Block::Contents => {
                let title = TextRun { text: "Contents".into(), ..Default::default() };
                let (_, above, below, _, _) = Style::H2.look();
                pg.space(above);
                let l = Rc::new(text_layout(&ctx, &[&title], Style::H2, Align::Left, width));
                let h = line_tops(&l).1;
                if pg.room() < h * 3.0 && pg.y > 0.0 {
                    pg.next_column();
                }
                pg.push(Item::Lines { layout: l, lines: 0..1, x: 0.0, top: pg.y, marker: None });
                pg.y += h + below;
                for (level, text, hb) in doc.headings() {
                    let indent = level.saturating_sub(1) as f64 * 14.0;
                    let page = heading_pages.get(&hb).map(|p| p.to_string()).unwrap_or_default();
                    let run = TextRun { text: format!("{text}\t{page}"), bold: level <= 1, ..Default::default() };
                    let l = text_layout(&ctx, &[&run], Style::Normal, Align::Left, width - indent);
                    // The page number sits against the right edge.
                    let mut tabs = pango::TabArray::new(1, false);
                    tabs.set_tab(0, pango::TabAlign::Right, ((width - indent - 1.0) * pango::SCALE as f64) as i32);
                    l.set_tabs(Some(&tabs));
                    l.set_ellipsize(pango::EllipsizeMode::None);
                    let h = line_tops(&l).1 + 3.0;
                    if pg.room() < h && pg.y > 0.0 {
                        pg.next_column();
                    }
                    let lines = 0..l.line_count().max(1) as usize;
                    pg.push(Item::Lines { layout: Rc::new(l), lines, x: indent, top: pg.y, marker: None });
                    pg.y += h;
                }
                pg.space(12.0);
            }
        }
    }
    Rendered { setup: doc.page.clone(), pages: pg.pages, ctx, heading_pages: found_pages }
}

/// A footnote as it's set at the foot of the page: its number, then its words.
fn note_layout(ctx: &pango::Context, n: usize, text: &str, width: f64) -> pango::Layout {
    let num = TextRun { text: format!("{n}  "), size: Some(NOTE_SIZE), bold: true, ..Default::default() };
    let words = TextRun { text: text.replace('\n', " "), size: Some(NOTE_SIZE), ..Default::default() };
    text_layout(ctx, &[&num, &words], Style::Normal, Align::Left, width)
}

impl Rendered {
    /// The page (from 1) each heading starts on, by block index.
    pub fn heading_pages(&self) -> &HashMap<usize, usize> {
        &self.heading_pages
    }

    fn draw_note(&self, cr: &cairo::Context, text: &str, x: f64, y: f64, w: f64, align: pango::Alignment) {
        let layout = pango::Layout::new(&self.ctx);
        let mut font = pango::FontDescription::from_string(DEFAULT_FONT);
        font.set_size(9 * pango::SCALE);
        layout.set_font_description(Some(&font));
        layout.set_width((w * pango::SCALE as f64) as i32);
        layout.set_alignment(align);
        layout.set_ellipsize(pango::EllipsizeMode::End);
        layout.set_text(text);
        let (_, h) = layout.size();
        cr.set_source_rgb(0.42, 0.45, 0.5);
        cr.move_to(x, y - h as f64 / pango::SCALE as f64 / 2.0);
        pangocairo::functions::show_layout(cr, &layout);
    }
}

fn draw_lines(cr: &cairo::Context, layout: &pango::Layout, lines: &std::ops::Range<usize>, x: f64, top: f64) {
    let mut it = layout.iter();
    let mut i = 0;
    let mut base_top = None;
    loop {
        if lines.contains(&i) {
            let (_, logical) = it.line_extents();
            let line_top = logical.y() as f64 / pango::SCALE as f64;
            let t = *base_top.get_or_insert(line_top);
            let baseline = it.baseline() as f64 / pango::SCALE as f64;
            if let Some(line) = it.line_readonly() {
                cr.move_to(x + logical.x() as f64 / pango::SCALE as f64, top + baseline - t);
                pangocairo::functions::show_layout_line(cr, &line);
            }
        }
        i += 1;
        if i >= lines.end || !it.next_line() {
            break;
        }
    }
}

impl Pages for Rendered {
    fn count(&self) -> usize {
        self.pages.len().max(1)
    }

    fn size(&self) -> (f64, f64) {
        let p = self.setup.paper();
        (p.width, p.height)
    }

    fn draw(&self, cr: &cairo::Context, index: usize) {
        let (pw, ph) = self.size();
        let [mt, mb, ml, _] = self.setup.margins;
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.rectangle(0.0, 0.0, pw, ph);
        let _ = cr.fill();
        let width = self.setup.content_width();
        if !self.setup.header.is_empty() {
            self.draw_note(cr, &self.setup.header, ml, mt / 2.0, width, pango::Alignment::Left);
        }
        let numbers = self.setup.page_numbers;
        if !self.setup.footer.is_empty() {
            let w = if numbers { width * 0.75 } else { width };
            self.draw_note(cr, &self.setup.footer, ml, ph - mb / 2.0, w, pango::Alignment::Left);
        }
        if numbers {
            let align = if self.setup.footer.is_empty() { pango::Alignment::Center } else { pango::Alignment::Right };
            self.draw_note(cr, &format!("{}", index + 1), ml, ph - mb / 2.0, width, align);
        }
        let Some(page) = self.pages.get(index) else { return };
        cr.translate(ml, mt);
        // Footnotes, at the foot of their column under a short rule.
        let col_step = self.setup.column_width() + self.setup.column_gap.max(0.0);
        let bottom = self.setup.content_height();
        for col in 0..self.setup.columns() {
            let notes: Vec<&Rc<pango::Layout>> = page.notes.iter().filter(|(c, _)| *c == col).map(|(_, l)| l).collect();
            if notes.is_empty() {
                continue;
            }
            let total: f64 = notes.iter().map(|l| line_tops(l).1 + 2.0).sum();
            let x = col as f64 * col_step;
            let mut y = bottom - total;
            cr.set_source_rgb(0.6, 0.63, 0.67);
            cr.set_line_width(0.5);
            cr.move_to(x, y - NOTE_RULE / 2.0);
            cr.line_to(x + 72.0, y - NOTE_RULE / 2.0);
            let _ = cr.stroke();
            for l in notes {
                cr.set_source_rgb(INK.0, INK.1, INK.2);
                cr.move_to(x, y);
                pangocairo::functions::show_layout(cr, l);
                y += line_tops(l).1 + 2.0;
            }
        }
        for item in &page.items {
            cr.set_source_rgb(INK.0, INK.1, INK.2);
            match item {
                Item::Lines { layout, lines, x, top, marker } => {
                    if let Some(m) = marker {
                        let mw = INDENT_STEP * 0.7;
                        // Line the marker's baseline up with the first line's.
                        let first = layout.iter().baseline() as f64 / pango::SCALE as f64;
                        let mark = m.iter().baseline() as f64 / pango::SCALE as f64;
                        cr.move_to(x - INDENT_STEP * 0.3 - mw, top + first - mark);
                        pangocairo::functions::show_layout(cr, m);
                    }
                    cr.set_source_rgb(INK.0, INK.1, INK.2);
                    draw_lines(cr, layout, lines, *x, *top);
                }
                Item::Picture { surface, x, top, w, h } => {
                    let _ = cr.save();
                    cr.translate(*x, *top);
                    cr.scale(w / surface.width().max(1) as f64, h / surface.height().max(1) as f64);
                    let _ = cr.set_source_surface(surface, 0.0, 0.0);
                    let _ = cr.paint();
                    let _ = cr.restore();
                }
                Item::Row { cells, x, top, col_w, h, header } => {
                    let total = col_w * cells.len() as f64;
                    if *header {
                        cr.set_source_rgb(0.93, 0.94, 0.96);
                        cr.rectangle(*x, *top, total, *h);
                        let _ = cr.fill();
                    }
                    cr.set_source_rgb(0.6, 0.63, 0.67);
                    cr.set_line_width(0.6);
                    for (c, cell) in cells.iter().enumerate() {
                        let cx = x + c as f64 * col_w;
                        cr.rectangle(cx, *top, *col_w, *h);
                        let _ = cr.stroke();
                        cr.set_source_rgb(INK.0, INK.1, INK.2);
                        cr.move_to(cx + CELL_PAD, top + CELL_PAD);
                        pangocairo::functions::show_layout(cr, cell);
                        cr.set_source_rgb(0.6, 0.63, 0.67);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn para(text: &str) -> Block {
        Block::Paragraph(Paragraph::text(Style::Normal, text))
    }

    #[test]
    fn long_text_spills_onto_more_pages() {
        let words = "word ".repeat(400);
        let doc = Document { blocks: (0..12).map(|_| para(&words)).collect(), ..Default::default() };
        let r = paginate(&doc, &BTreeMap::new());
        assert!(r.count() >= 4, "{} pages", r.count());
        let short = Document { blocks: vec![para("Hello")], ..Default::default() };
        assert_eq!(paginate(&short, &BTreeMap::new()).count(), 1);
    }

    #[test]
    fn breaks_columns_and_footnotes() {
        let words = "word ".repeat(300);
        // A page break starts a new page.
        let doc = Document { blocks: vec![para("a"), Block::PageBreak, para("b")], ..Default::default() };
        assert_eq!(paginate(&doc, &BTreeMap::new()).count(), 2);
        // Two columns hold about twice as much per page.
        let long = Document { blocks: (0..8).map(|_| para(&words)).collect(), ..Default::default() };
        let one = paginate(&long, &BTreeMap::new()).count();
        let mut two_cols = long.clone();
        two_cols.page.columns = 2;
        let two = paginate(&two_cols, &BTreeMap::new());
        assert!(two.count() >= one, "{} vs {one}", two.count());
        assert!(two.pages[0].items.iter().any(|i| matches!(i, Item::Lines { x, .. } if *x > 100.0)));
        // Footnotes go at the foot of the page their mark is on.
        let noted = Document {
            blocks: vec![Block::Paragraph(Paragraph {
                runs: vec![Run::text("Text"), Run::Note(Note { note: "A note.".into() })],
                ..Default::default()
            })],
            ..Default::default()
        };
        let r = paginate(&noted, &BTreeMap::new());
        assert_eq!(r.pages[0].notes.len(), 1);
        // A table of contents gets each heading's page.
        let toc = Document {
            blocks: vec![
                Block::Contents,
                Block::Paragraph(Paragraph::text(Style::H1, "One")),
                Block::PageBreak,
                Block::Paragraph(Paragraph::text(Style::H1, "Two")),
            ],
            ..Default::default()
        };
        let r = paginate(&toc, &BTreeMap::new());
        assert_eq!(r.heading_pages().get(&1), Some(&1));
        assert_eq!(r.heading_pages().get(&3), Some(&2));
    }

    #[test]
    fn a4_is_taller() {
        let mut doc = Document { blocks: vec![para("x")], ..Default::default() };
        doc.page.paper = "a4".into();
        let r = paginate(&doc, &BTreeMap::new());
        assert!((r.size().1 - 841.89).abs() < 0.1);
    }
}
