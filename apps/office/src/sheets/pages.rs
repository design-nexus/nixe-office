//! Sheets on paper: each sheet's data in landscape pages, down then across,
//! with the header row repeated, and the chart on a page of its own.

use super::chart;
use super::model::*;
use crate::print::{Pages, Paper};
use gtk::pango;

const MARGIN: f64 = 36.0;
const TITLE_H: f64 = 28.0;
/// Screen pixels to points.
const PT: f64 = 0.75;

struct CellItem {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    text: String,
    right: bool,
    format: Format,
    error: bool,
}

enum Page {
    Cells { title: String, cells: Vec<CellItem> },
    Chart { title: String, kind: ChartKind, names: Vec<String>, labels: Vec<String>, series: Vec<Vec<f64>> },
}

pub struct SheetPages {
    size: (f64, f64),
    pages: Vec<Page>,
    ctx: pango::Context,
}

impl SheetPages {
    pub fn new(wb: &Workbook, calc: &Calc) -> SheetPages {
        let paper = Paper::preferred();
        // Landscape: spreadsheets are wide.
        let (pw, ph) = (paper.height, paper.width);
        let (cw, ch) = (pw - 2.0 * MARGIN, ph - 2.0 * MARGIN - TITLE_H);
        let mut pages = Vec::new();
        for (si, s) in wb.sheets.iter().enumerate() {
            let Some((lr, lc)) = s.used() else { continue };
            let rows: Vec<u32> = wb.view_rows(si, calc).into_iter().filter(|r| *r <= lr).collect();
            let header = s.header && rows.first() == Some(&0) && rows.len() > 1;
            let body: Vec<u32> = if header { rows[1..].to_vec() } else { rows.clone() };
            let head_h = if header { s.row_h(0) as f64 * PT } else { 0.0 };
            // Columns that fit across a page.
            let mut col_groups: Vec<Vec<u32>> = vec![Vec::new()];
            let mut x = 0.0;
            for c in 0..=lc {
                let w = (s.col_w(c) as f64 * PT).min(cw);
                if x + w > cw && !col_groups.last().is_some_and(|g| g.is_empty()) {
                    col_groups.push(Vec::new());
                    x = 0.0;
                }
                col_groups.last_mut().expect("a group").push(c);
                x += w;
            }
            // Rows that fit down a page, under the repeated header.
            let mut row_groups: Vec<Vec<u32>> = vec![Vec::new()];
            let mut y = head_h;
            for r in &body {
                let h = (s.row_h(*r) as f64 * PT).min(ch - head_h);
                if y + h > ch && !row_groups.last().is_some_and(|g| g.is_empty()) {
                    row_groups.push(Vec::new());
                    y = head_h;
                }
                row_groups.last_mut().expect("a group").push(*r);
                y += h;
            }
            let multi = col_groups.len() * row_groups.len() > 1;
            let mut n = 0;
            for cols in &col_groups {
                for rws in &row_groups {
                    n += 1;
                    let mut cells = Vec::new();
                    let mut y = 0.0;
                    let all: Vec<u32> =
                        if header { std::iter::once(0).chain(rws.iter().copied()).collect() } else { rws.clone() };
                    // With a filter on, rows aren't the sheet's rows: no merging then.
                    let merges: &[Area] = if s.filter.as_ref().is_some_and(|f| !f.query.trim().is_empty()) { &[] } else { &s.merges };
                    for r in &all {
                        let r = *r;
                        let h = s.row_h(r) as f64 * PT;
                        let mut x = 0.0;
                        for c in cols {
                            let c = *c;
                            let cw = s.col_w(c) as f64 * PT;
                            let merge = merges.iter().find(|m| m.contains(r, c));
                            // A merged block is drawn once, from its top-left cell
                            // (or wherever it first appears on this page).
                            let (w, bh) = match merge {
                                Some(m) => {
                                    let first_r = all.iter().find(|x| (m.top..=m.bottom).contains(*x));
                                    let first_c = cols.iter().find(|x| (m.left..=m.right).contains(*x));
                                    if first_r != Some(&r) || first_c != Some(&c) {
                                        x += cw;
                                        continue;
                                    }
                                    let w: f64 = cols.iter().filter(|x| (m.left..=m.right).contains(*x)).map(|x| s.col_w(*x) as f64 * PT).sum();
                                    let h: f64 = all.iter().filter(|x| (m.top..=m.bottom).contains(*x)).map(|x| s.row_h(*x) as f64 * PT).sum();
                                    (w, h)
                                }
                                None => (cw, h),
                            };
                            let (sr, sc) = merge.map(|m| (m.top, m.left)).unwrap_or((r, c));
                            let v = calc.value(wb, si, sr, sc);
                            let format = calc.look(wb, si, sr, sc);
                            let (text, right) = display(&v, s.raw(sr, sc), &format);
                            cells.push(CellItem { x, y, w, h: bh, text, right, error: v.is_err(), format });
                            x += cw;
                        }
                        y += h;
                    }
                    let title = if multi { format!("{} · page {n}", s.name) } else { s.name.clone() };
                    pages.push(Page::Cells { title, cells });
                }
            }
            if let Some(chart) = &s.chart
                && let Some((names, labels, series)) = wb.chart_data(si, calc)
            {
                pages.push(Page::Chart {
                    title: format!("{} · chart of {}", s.name, chart.range),
                    kind: chart.kind,
                    names,
                    labels,
                    series,
                });
            }
        }
        if pages.is_empty() {
            pages.push(Page::Cells { title: wb.sheet().name.clone(), cells: Vec::new() });
        }
        SheetPages { size: (pw, ph), pages, ctx: crate::print::text_context() }
    }
}

fn rgb(hex: &str) -> (f64, f64, f64) {
    let h = hex.trim_start_matches('#');
    let p = |i: usize| h.get(i..i + 2).and_then(|x| u8::from_str_radix(x, 16).ok()).unwrap_or(0) as f64 / 255.0;
    (p(0), p(2), p(4))
}

impl Pages for SheetPages {
    fn count(&self) -> usize {
        self.pages.len()
    }

    fn size(&self) -> (f64, f64) {
        self.size
    }

    fn draw(&self, cr: &cairo::Context, index: usize) {
        let (pw, ph) = self.size;
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.rectangle(0.0, 0.0, pw, ph);
        cr.fill().ok();
        let Some(page) = self.pages.get(index) else { return };
        let layout = pango::Layout::new(&self.ctx);
        let title = match page {
            Page::Cells { title, .. } | Page::Chart { title, .. } => title,
        };
        let mut font = pango::FontDescription::from_string("Noto Sans");
        font.set_size(12 * pango::SCALE);
        font.set_weight(pango::Weight::Semibold);
        layout.set_font_description(Some(&font));
        layout.set_text(title);
        cr.set_source_rgb(0.125, 0.137, 0.161);
        cr.move_to(MARGIN, MARGIN);
        pangocairo::functions::show_layout(cr, &layout);
        let (ox, oy) = (MARGIN, MARGIN + TITLE_H);
        match page {
            Page::Chart { kind, names, labels, series, .. } => {
                chart::draw(
                    cr,
                    &self.ctx,
                    *kind,
                    &chart::Data { names, labels, series },
                    (ox, oy, pw - 2.0 * MARGIN, ph - 2.0 * MARGIN - TITLE_H),
                );
            }
            Page::Cells { cells, .. } => {
                let mut font = pango::FontDescription::from_string("Noto Sans");
                font.set_size((8.5 * pango::SCALE as f64) as i32);
                layout.set_ellipsize(pango::EllipsizeMode::End);
                for item in cells {
                    let (x, y) = (ox + item.x, oy + item.y);
                    if let Some(fill) = &item.format.fill {
                        let (r, g, b) = rgb(fill);
                        cr.set_source_rgb(r, g, b);
                        cr.rectangle(x, y, item.w, item.h);
                        cr.fill().ok();
                    }
                    cr.set_source_rgb(0.81, 0.83, 0.86);
                    cr.set_line_width(0.5);
                    cr.rectangle(x, y, item.w, item.h);
                    cr.stroke().ok();
                    if item.text.is_empty() {
                        continue;
                    }
                    let mut f = font.clone();
                    if item.format.bold {
                        f.set_weight(pango::Weight::Bold);
                    }
                    if item.format.italic {
                        f.set_style(pango::Style::Italic);
                    }
                    layout.set_font_description(Some(&f));
                    layout.set_width(((item.w - 8.0).max(1.0) * pango::SCALE as f64) as i32);
                    let align = item.format.align.unwrap_or(if item.right { HAlign::Right } else { HAlign::Left });
                    layout.set_alignment(match align {
                        HAlign::Left => pango::Alignment::Left,
                        HAlign::Center => pango::Alignment::Center,
                        HAlign::Right => pango::Alignment::Right,
                    });
                    layout.set_text(&item.text);
                    let (_, th) = layout.size();
                    let (r, g, b) = if item.error {
                        (0.776, 0.157, 0.157)
                    } else {
                        item.format.color.as_deref().map(rgb).unwrap_or((0.125, 0.137, 0.161))
                    };
                    cr.set_source_rgb(r, g, b);
                    cr.move_to(x + 4.0, y + (item.h - th as f64 / pango::SCALE as f64) / 2.0);
                    pangocairo::functions::show_layout(cr, &layout);
                }
            }
        }
    }
}
