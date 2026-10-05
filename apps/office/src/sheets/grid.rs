//! The cell grid: a drawing area that paints only the cells in view, with its
//! own scrollbars, headers, selection, fill handle and in-cell editor.

use super::model::*;
use super::{Drag, Sheets, formula};
use gtk::prelude::*;
use gtk::{gdk, glib, pango};
use std::rc::Rc;

pub const HEAD_W: f64 = 52.0;
pub const HEAD_H: f64 = 26.0;
/// How close to a header edge counts as grabbing it to resize.
const EDGE: f64 = 4.0;
const HANDLE: f64 = 7.0;
const INK: (f64, f64, f64) = (0.125, 0.137, 0.161);
const LINES: (f64, f64, f64) = (0.886, 0.898, 0.918);

fn rgb(hex: &str) -> (f64, f64, f64) {
    let h = hex.trim_start_matches('#');
    let p = |i: usize| h.get(i..i + 2).and_then(|x| u8::from_str_radix(x, 16).ok()).unwrap_or(0) as f64 / 255.0;
    (p(0), p(2), p(4))
}

fn mix(a: (f64, f64, f64), b: (f64, f64, f64), t: f64) -> (f64, f64, f64) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
}

enum Hit {
    Corner,
    ColHead(u32, bool),
    RowHead(u32, bool),
    Handle,
    Cell(u32, u32),
    Outside,
}

impl Sheets {
    pub(super) fn build_grid(self: &Rc<Self>) -> gtk::Widget {
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&self.area));
        self.entry.set_halign(gtk::Align::Start);
        self.entry.set_valign(gtk::Align::Start);
        overlay.add_overlay(&self.entry);
        overlay.set_hexpand(true);
        overlay.set_vexpand(true);
        let vbar = gtk::Scrollbar::new(gtk::Orientation::Vertical, Some(&self.vadj));
        let hbar = gtk::Scrollbar::new(gtk::Orientation::Horizontal, Some(&self.hadj));
        let row = widgets_hbox();
        row.append(&overlay);
        row.append(&vbar);
        let col = gtk::Box::new(gtk::Orientation::Vertical, 0);
        col.add_css_class("grid-frame");
        col.set_hexpand(true);
        col.set_size_request(320, -1);
        col.append(&row);
        col.append(&hbar);

        let me = Rc::downgrade(self);
        self.area.set_draw_func(move |a, cr, w, h| {
            if let Some(s) = me.upgrade() {
                s.draw(a, cr, w as f64, h as f64);
            }
        });
        let me = Rc::downgrade(self);
        self.area.connect_resize(move |_, _, _| {
            if let Some(s) = me.upgrade() {
                s.update_adjustments();
                s.place_entry();
            }
        });
        for adj in [&self.hadj, &self.vadj] {
            let me = Rc::downgrade(self);
            adj.connect_value_changed(move |_| {
                if let Some(s) = me.upgrade() {
                    s.area.queue_draw();
                    s.place_entry();
                }
            });
        }
        // A cell's comment shows when the pointer rests on it.
        self.area.set_has_tooltip(true);
        let me = Rc::downgrade(self);
        self.area.connect_query_tooltip(move |_, x, y, _, tip| {
            let Some(text) = me.upgrade().and_then(|s| s.comment_at(x as f64, y as f64)) else { return false };
            tip.set_text(Some(&text));
            true
        });
        self.connect_mouse();
        self.connect_keys();
        self.connect_entry();
        col.upcast()
    }

    // ---------- Geometry ----------

    pub(super) fn relayout(&self) {
        let wb = self.wb.borrow();
        let calc = self.calc.borrow();
        let si = wb.active;
        let view = wb.view_rows(si, &calc);
        let s = wb.sheet();
        let mut ys = Vec::with_capacity(view.len() + 1);
        let mut y = 0.0;
        for r in &view {
            ys.push(y);
            y += s.row_h(*r) as f64;
        }
        ys.push(y);
        let mut xs = Vec::with_capacity(COLS as usize + 1);
        let mut x = 0.0;
        for c in 0..COLS {
            xs.push(x);
            x += s.col_w(c) as f64;
        }
        xs.push(x);
        drop((wb, calc));
        *self.view.borrow_mut() = view;
        *self.row_y.borrow_mut() = ys;
        *self.col_x.borrow_mut() = xs;
        self.clamp_cursor();
        self.update_adjustments();
    }

    fn frozen(&self) -> bool {
        self.wb.borrow().sheet().freeze && self.row_y.borrow().len() > 2
    }

    fn frozen_h(&self) -> f64 {
        if self.frozen() {
            let ys = self.row_y.borrow();
            ys[1] - ys[0]
        } else {
            0.0
        }
    }

    /// Content y where the scrolling rows start.
    fn base(&self) -> f64 {
        if self.frozen() { self.row_y.borrow()[1] } else { 0.0 }
    }

    fn update_adjustments(&self) {
        let (w, h) = (self.area.width() as f64, self.area.height() as f64);
        let (rows_h, cols_w) = (*self.row_y.borrow().last().unwrap_or(&0.0), *self.col_x.borrow().last().unwrap_or(&0.0));
        let vpage = (h - HEAD_H - self.frozen_h()).max(1.0);
        let hpage = (w - HEAD_W).max(1.0);
        self.vadj.configure(
            self.vadj.value().min((rows_h - self.base() - vpage).max(0.0)),
            0.0,
            (rows_h - self.base()).max(vpage),
            30.0,
            vpage * 0.9,
            vpage,
        );
        self.hadj.configure(self.hadj.value().min((cols_w - hpage).max(0.0)), 0.0, cols_w.max(hpage), 40.0, hpage * 0.9, hpage);
    }

    /// Top and height of on-screen row `vi`, in widget pixels.
    fn row_rect(&self, vi: u32) -> (f64, f64) {
        let ys = self.row_y.borrow();
        let i = vi as usize;
        if i + 1 >= ys.len() {
            return (-1000.0, 0.0);
        }
        let h = ys[i + 1] - ys[i];
        if self.frozen() && i == 0 {
            return (HEAD_H, h);
        }
        (HEAD_H + self.frozen_h() + ys[i] - self.base() - self.vadj.value(), h)
    }

    fn col_rect(&self, c: u32) -> (f64, f64) {
        let xs = self.col_x.borrow();
        let i = c as usize;
        if i + 1 >= xs.len() {
            return (-1000.0, 0.0);
        }
        (HEAD_W + xs[i] - self.hadj.value(), xs[i + 1] - xs[i])
    }

    /// Where a block of on-screen cells is drawn: x, y, width, height.
    fn area_rect(&self, a: Area) -> (f64, f64, f64, f64) {
        let (y1, _) = self.row_rect(a.top);
        let (y2, h2) = self.row_rect(a.bottom);
        let (x1, _) = self.col_rect(a.left);
        let (x2, w2) = self.col_rect(a.right);
        (x1, y1, x2 + w2 - x1, y2 + h2 - y1)
    }

    /// The on-screen cell at (x, y), if the point is over cells.
    pub(super) fn cell_at(&self, x: f64, y: f64) -> Option<(u32, u32)> {
        Some((self.row_at(y)?, self.col_at(x)?))
    }

    fn row_at(&self, y: f64) -> Option<u32> {
        if y < HEAD_H {
            return None;
        }
        if self.frozen() && y < HEAD_H + self.frozen_h() {
            return Some(0);
        }
        let content = y - HEAD_H - self.frozen_h() + self.base() + self.vadj.value();
        let ys = self.row_y.borrow();
        let i = ys.partition_point(|v| *v <= content);
        (i >= 1 && i < ys.len()).then(|| i as u32 - 1)
    }

    fn col_at(&self, x: f64) -> Option<u32> {
        if x < HEAD_W {
            return None;
        }
        let content = x - HEAD_W + self.hadj.value();
        let xs = self.col_x.borrow();
        let i = xs.partition_point(|v| *v <= content);
        (i >= 1 && i < xs.len()).then(|| i as u32 - 1)
    }

    /// The cell under (x, y), even outside the grid (clamped to the nearest).
    fn cell_near(&self, x: f64, y: f64) -> (u32, u32) {
        let rows = self.view.borrow().len().max(1) as u32;
        let r = self.row_at(y.max(HEAD_H + 1.0)).unwrap_or(if y < HEAD_H { 0 } else { rows - 1 });
        let c = self.col_at(x.max(HEAD_W + 1.0)).unwrap_or(if x < HEAD_W { 0 } else { COLS - 1 });
        (r, c)
    }

    fn hit(&self, x: f64, y: f64) -> Hit {
        if x < HEAD_W && y < HEAD_H {
            return Hit::Corner;
        }
        if y < HEAD_H {
            let Some(c) = self.col_at(x) else { return Hit::Outside };
            let (cx, cw) = self.col_rect(c);
            if (cx + cw - x).abs() <= EDGE {
                return Hit::ColHead(c, true);
            }
            // The left edge resizes the column before.
            if (x - cx).abs() <= EDGE && c > 0 {
                return Hit::ColHead(c - 1, true);
            }
            return Hit::ColHead(c, false);
        }
        if x < HEAD_W {
            let Some(r) = self.row_at(y) else { return Hit::Outside };
            let (ry, rh) = self.row_rect(r);
            if (ry + rh - y).abs() <= EDGE {
                return Hit::RowHead(r, true);
            }
            if (y - ry).abs() <= EDGE && r > 0 {
                return Hit::RowHead(r - 1, true);
            }
            return Hit::RowHead(r, false);
        }
        let sel = self.selection();
        let (bx, bw) = self.col_rect(sel.right);
        let (by, bh) = self.row_rect(sel.bottom);
        if (x - (bx + bw)).abs() <= HANDLE && (y - (by + bh)).abs() <= HANDLE && !self.editing.get() {
            return Hit::Handle;
        }
        match (self.row_at(y), self.col_at(x)) {
            (Some(r), Some(c)) => Hit::Cell(r, c),
            _ => Hit::Outside,
        }
    }

    pub(super) fn scroll_to(&self, r: u32, c: u32) {
        let (y, h) = self.row_rect(r);
        let top = HEAD_H + self.frozen_h();
        let bottom = self.area.height() as f64;
        if !(self.frozen() && r == 0) {
            if y < top {
                self.vadj.set_value(self.vadj.value() - (top - y));
            } else if y + h > bottom {
                self.vadj.set_value(self.vadj.value() + (y + h - bottom));
            }
        }
        let (x, w) = self.col_rect(c);
        let right = self.area.width() as f64;
        if x < HEAD_W {
            self.hadj.set_value(self.hadj.value() - (HEAD_W - x));
        } else if x + w > right {
            self.hadj.set_value(self.hadj.value() + (x + w - right));
        }
    }

    pub(super) fn clamp_cursor(&self) {
        let rows = self.view.borrow().len().max(1) as u32;
        let fix = |(r, c): (u32, u32)| (r.min(rows - 1), c.min(COLS - 1));
        self.cursor.set(fix(self.cursor.get()));
        self.anchor.set(fix(self.anchor.get()));
    }

    pub(super) fn move_to(&self, r: u32, c: u32, extend: bool) {
        let rows = self.view.borrow().len().max(1) as u32;
        let (mut r, mut c) = (r.min(rows - 1), c.min(COLS - 1));
        // A merged block is one cell: the cursor sits on its top-left.
        if !extend
            && self.merges_shown()
            && let Some(m) = self.wb.borrow().sheet().merge_at(r, c)
        {
            (r, c) = (m.top, m.left);
        }
        self.cursor.set((r, c));
        if !extend {
            self.anchor.set((r, c));
        }
        self.scroll_to(r, c);
        self.sync();
        self.area.queue_draw();
    }

    /// One cell along (dr, dc), stepping over the whole merged block the cursor is in.
    pub(super) fn step(&self, dr: i64, dc: i64) -> (u32, u32) {
        let (r, c) = self.cursor.get();
        let m = self.cursor_merge().unwrap_or(Area::cell(r, c));
        let nr = match dr {
            1 => m.bottom as i64 + 1,
            -1 => m.top as i64 - 1,
            _ => r as i64,
        };
        let nc = match dc {
            1 => m.right as i64 + 1,
            -1 => m.left as i64 - 1,
            _ => c as i64,
        };
        (nr.max(0) as u32, nc.max(0) as u32)
    }

    /// Ctrl+arrow: to the edge of the block of data, or the next data across a gap.
    fn jump(&self, dr: i64, dc: i64) -> (u32, u32) {
        let (r, c) = self.cursor.get();
        let rows = self.view.borrow().len() as i64;
        let filled = |r: i64, c: i64| {
            r >= 0
                && c >= 0
                && r < rows
                && c < COLS as i64
                && !self.wb.borrow().sheet().raw(self.src_row(r as u32), c as u32).is_empty()
        };
        let (mut rr, mut cc) = (r as i64, c as i64);
        let inside = |r: i64, c: i64| r >= 0 && c >= 0 && r < rows && c < COLS as i64;
        if filled(rr, cc) && filled(rr + dr, cc + dc) {
            while filled(rr + dr, cc + dc) {
                rr += dr;
                cc += dc;
            }
        } else {
            rr += dr;
            cc += dc;
            // Stop at the next filled cell; past the data, go to the sheet's edge
            // (but no further than just past the data for rows).
            let mut steps = 0;
            while inside(rr, cc) && !filled(rr, cc) && steps < 20_000 {
                rr += dr;
                cc += dc;
                steps += 1;
            }
            if !inside(rr, cc) {
                rr = rr.clamp(0, rows - 1);
                cc = cc.clamp(0, COLS as i64 - 1);
                if dr > 0 {
                    let last = self.wb.borrow().sheet().used().map(|(r, _)| r as i64 + 1).unwrap_or(0);
                    rr = rr.min(last.max(r as i64));
                }
            }
        }
        (rr.clamp(0, rows - 1) as u32, cc.clamp(0, COLS as i64 - 1) as u32)
    }

    // ---------- Drawing ----------

    fn draw(&self, a: &gtk::DrawingArea, cr: &cairo::Context, w: f64, h: f64) {
        let pal = crate::theme::palette();
        let (bg, text, dim, accent, border) =
            (rgb(&pal.bg), rgb(&pal.text), rgb(&pal.dim_text), rgb(&pal.accent), rgb(&pal.border));
        let hairline = mix(bg, border, 0.82);
        let wb = self.wb.borrow();
        let calc = self.calc.borrow();
        let s = wb.sheet();
        let sel = self.selection();
        let rows = self.view.borrow().len() as u32;

        // The cells: white paper.
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().ok();

        // Rows and columns in view.
        let first_col = self.col_at(HEAD_W + 0.5).unwrap_or(0);
        let last_col = self.col_at(w - 1.0).unwrap_or(COLS - 1);
        let top = HEAD_H + self.frozen_h();
        let first_row = self.row_at(top + 0.5).unwrap_or(0);
        let last_row = self.row_at(h - 1.0).unwrap_or(rows.saturating_sub(1));
        let mut visible: Vec<u32> = (first_row..=last_row.min(rows.saturating_sub(1))).collect();
        if self.frozen() {
            visible.retain(|r| *r != 0);
            visible.insert(0, 0);
        }

        let layout = a.create_pango_layout(None);
        let base_font = pango::FontDescription::from_string("Noto Sans 10");
        let found = self.found.borrow();
        let view = self.view.borrow();
        let merges: Vec<Area> = if self.merges_shown() {
            s.merges.iter().filter(|m| (m.bottom >= first_row || m.top == 0) && m.top <= last_row && m.right >= first_col && m.left <= last_col).copied().collect()
        } else {
            Vec::new()
        };
        let in_merge = |r: u32, c: u32| merges.iter().any(|m| m.contains(r, c));

        for &vi in &visible {
            let (y, rh) = self.row_rect(vi);
            let src = view[vi as usize];
            cr.save().ok();
            // The frozen row draws over everything; others are clipped below it.
            let clip_top = if self.frozen() && vi == 0 { HEAD_H } else { top };
            cr.rectangle(HEAD_W, clip_top, w - HEAD_W, h - clip_top);
            cr.clip();
            // Fills, highlights and gridlines.
            for c in first_col..=last_col {
                let (x, cw) = self.col_rect(c);
                let f = calc.look(&wb, wb.active, src, c);
                if let Some(fill) = &f.fill {
                    let (r, g, b) = rgb(fill);
                    cr.set_source_rgb(r, g, b);
                    cr.rectangle(x, y, cw, rh);
                    cr.fill().ok();
                }
                if found.contains(&(src, c)) {
                    cr.set_source_rgb(1.0, 0.94, 0.55);
                    cr.rectangle(x, y, cw, rh);
                    cr.fill().ok();
                }
            }
            cr.set_source_rgb(LINES.0, LINES.1, LINES.2);
            cr.set_line_width(1.0);
            for c in first_col..=last_col {
                let (x, cw) = self.col_rect(c);
                cr.move_to((x + cw).floor() - 0.5, y);
                cr.line_to((x + cw).floor() - 0.5, y + rh);
            }
            cr.move_to(HEAD_W, (y + rh).floor() - 0.5);
            cr.line_to(w, (y + rh).floor() - 0.5);
            cr.stroke().ok();

            // Text. Words that don't fit run on into empty cells to their right.
            for c in first_col..=last_col {
                let raw = s.raw(src, c);
                if raw.is_empty() {
                    continue;
                }
                if self.editing.get() && (vi, c) == self.cursor.get() || in_merge(src, c) {
                    continue;
                }
                let f = calc.look(&wb, wb.active, src, c);
                let v = calc.value(&wb, wb.active, src, c);
                let (shown, right) = display(&v, raw, &f);
                let mut font = base_font.clone();
                if f.bold {
                    font.set_weight(pango::Weight::Bold);
                }
                if f.italic {
                    font.set_style(pango::Style::Italic);
                }
                layout.set_font_description(Some(&font));
                layout.set_text(&shown);
                let (tw, th) = layout.pixel_size();
                let (x, cw) = self.col_rect(c);
                let align = f.align.unwrap_or(if right { HAlign::Right } else { HAlign::Left });
                let pad = 6.0;
                let mut room = cw;
                if align == HAlign::Left && tw as f64 > cw - 2.0 * pad && !matches!(v, formula::Value::Num(_)) {
                    let mut n = c + 1;
                    while n < COLS && s.raw(src, n).is_empty() && n <= last_col + 1 {
                        room += self.col_rect(n).1;
                        n += 1;
                    }
                }
                let tx = match align {
                    HAlign::Left => x + pad,
                    HAlign::Center => x + (cw - tw as f64) / 2.0,
                    HAlign::Right => x + cw - pad - tw as f64,
                };
                cr.save().ok();
                cr.rectangle(x + 1.0, y, room - 2.0, rh);
                cr.clip();
                let (r, g, b) = match (&v, &f.color) {
                    (formula::Value::Err(_), _) => (0.776, 0.157, 0.157),
                    (_, Some(c)) => rgb(c),
                    _ => INK,
                };
                cr.set_source_rgb(r, g, b);
                cr.move_to(tx, y + (rh - th as f64) / 2.0);
                pangocairo::functions::show_layout(cr, &layout);
                cr.restore().ok();
            }
            cr.restore().ok();
        }

        // Merged blocks: one cell's fill, outline and text over the whole block.
        for m in &merges {
            let (x, y, mw, mh) = self.area_rect(*m);
            cr.save().ok();
            let clip_top = if self.frozen() && m.top == 0 { HEAD_H } else { top };
            cr.rectangle(HEAD_W, clip_top, w - HEAD_W, h - clip_top);
            cr.clip();
            let f = calc.look(&wb, wb.active, m.top, m.left);
            let (r, g, b) = f.fill.as_deref().map(rgb).unwrap_or((1.0, 1.0, 1.0));
            let (r, g, b) = if found.contains(&(m.top, m.left)) { (1.0, 0.94, 0.55) } else { (r, g, b) };
            cr.set_source_rgb(r, g, b);
            cr.rectangle(x, y, mw - 1.0, mh - 1.0);
            cr.fill().ok();
            let editing_here = self.editing.get() && (m.top, m.left) == self.cursor.get();
            let raw = s.raw(m.top, m.left);
            if !raw.is_empty() && !editing_here {
                let v = calc.value(&wb, wb.active, m.top, m.left);
                let (shown, right) = display(&v, raw, &f);
                let mut font = base_font.clone();
                if f.bold {
                    font.set_weight(pango::Weight::Bold);
                }
                if f.italic {
                    font.set_style(pango::Style::Italic);
                }
                layout.set_font_description(Some(&font));
                layout.set_text(&shown);
                let (tw, th) = layout.pixel_size();
                let pad = 6.0;
                let tx = match f.align.unwrap_or(if right { HAlign::Right } else { HAlign::Left }) {
                    HAlign::Left => x + pad,
                    HAlign::Center => x + (mw - tw as f64) / 2.0,
                    HAlign::Right => x + mw - pad - tw as f64,
                };
                cr.rectangle(x + 1.0, y, mw - 2.0, mh);
                cr.clip();
                let (r, g, b) = match (&v, &f.color) {
                    (formula::Value::Err(_), _) => (0.776, 0.157, 0.157),
                    (_, Some(c)) => rgb(c),
                    _ => INK,
                };
                cr.set_source_rgb(r, g, b);
                cr.move_to(tx, y + (mh - th as f64) / 2.0);
                pangocairo::functions::show_layout(cr, &layout);
            }
            cr.restore().ok();
        }

        // Comments: a small corner mark.
        cr.save().ok();
        cr.rectangle(HEAD_W, HEAD_H, w - HEAD_W, h - HEAD_H);
        cr.clip();
        cr.set_source_rgb(0.90, 0.55, 0.0);
        for &vi in &visible {
            let src = view[vi as usize];
            for ((_, c), _) in s.cells.range((src, first_col)..=(src, last_col)).filter(|(_, cell)| cell.comment.is_some()) {
                let (x, y, cw, _) = match merges.iter().find(|m| m.top == src && m.left == *c) {
                    Some(m) => self.area_rect(*m),
                    None => {
                        let (x, cw) = self.col_rect(*c);
                        (x, self.row_rect(vi).0, cw, 0.0)
                    }
                };
                let right = x + cw - 1.0;
                cr.move_to(right - 7.0, y);
                cr.line_to(right, y);
                cr.line_to(right, y + 7.0);
                cr.close_path();
                cr.fill().ok();
            }
        }
        cr.restore().ok();

        // The selection: a tint and an accent outline, with the fill handle.
        let rect = |area: &Area| -> Option<(f64, f64, f64, f64)> {
            let (y1, _) = self.row_rect(area.top);
            let (y2, h2) = self.row_rect(area.bottom);
            let (x1, _) = self.col_rect(area.left);
            let (x2, w2) = self.col_rect(area.right);
            (y2 + h2 > HEAD_H && x2 + w2 > HEAD_W).then_some((x1, y1, x2 + w2 - x1, y2 + h2 - y1))
        };
        cr.save().ok();
        cr.rectangle(HEAD_W, HEAD_H, w - HEAD_W, h - HEAD_H);
        cr.clip();
        if let Some((x, y, sw, sh)) = rect(&sel) {
            if !sel.is_cell() {
                cr.set_source_rgba(accent.0, accent.1, accent.2, 0.10);
                cr.rectangle(x, y, sw, sh);
                cr.fill().ok();
            }
            cr.set_source_rgb(accent.0, accent.1, accent.2);
            cr.set_line_width(2.0);
            cr.rectangle(x + 0.5, y + 0.5, sw - 1.0, sh - 1.0);
            cr.stroke().ok();
            if !self.editing.get() {
                cr.rectangle(x + sw - 4.0, y + sh - 4.0, 7.0, 7.0);
                cr.fill().ok();
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.set_line_width(1.0);
                cr.rectangle(x + sw - 4.5, y + sh - 4.5, 8.0, 8.0);
                cr.stroke().ok();
            }
        }
        // Copied or cut cells: a dashed outline until they're pasted.
        if let Some(clip) = self.clip.borrow().as_ref() {
            let src = Area::new(
                clip.from.0,
                clip.from.1,
                clip.from.0 + clip.cells.len() as u32 - 1,
                clip.from.1 + clip.cells[0].len() as u32 - 1,
            );
            if self.view.borrow().len() == ROWS as usize
                && let Some((x, y, sw, sh)) = rect(&src)
            {
                cr.set_source_rgb(accent.0, accent.1, accent.2);
                cr.set_line_width(1.5);
                cr.set_dash(&[5.0, 3.0], 0.0);
                cr.rectangle(x + 1.5, y + 1.5, sw - 3.0, sh - 3.0);
                cr.stroke().ok();
                cr.set_dash(&[], 0.0);
            }
        }
        // While dragging the fill handle: where the fill will go.
        if self.drag.get() == Drag::Fill
            && let Some((x, y, sw, sh)) = rect(&self.fill_preview())
        {
            cr.set_source_rgb(accent.0, accent.1, accent.2);
            cr.set_line_width(1.0);
            cr.set_dash(&[3.0, 3.0], 0.0);
            cr.rectangle(x + 0.5, y + 0.5, sw - 1.0, sh - 1.0);
            cr.stroke().ok();
            cr.set_dash(&[], 0.0);
        }
        cr.restore().ok();

        // Headers, in the window's colours.
        let head_bg = bg;
        cr.set_source_rgb(head_bg.0, head_bg.1, head_bg.2);
        cr.rectangle(0.0, 0.0, w, HEAD_H);
        cr.rectangle(0.0, 0.0, HEAD_W, h);
        cr.fill().ok();
        let small = pango::FontDescription::from_string("JetBrains Mono 8.5");
        layout.set_font_description(Some(&small));
        let active_head = mix(bg, accent, 0.18);
        for c in first_col..=last_col {
            let (x, cw) = self.col_rect(c);
            if c >= sel.left && c <= sel.right {
                cr.set_source_rgb(active_head.0, active_head.1, active_head.2);
                cr.rectangle(x.max(HEAD_W), 0.0, cw, HEAD_H);
                cr.fill().ok();
            }
            layout.set_text(&formula::col_name(c));
            let (tw, th) = layout.pixel_size();
            let on = c >= sel.left && c <= sel.right;
            let col = if on { text } else { dim };
            cr.set_source_rgb(col.0, col.1, col.2);
            cr.move_to(x + (cw - tw as f64) / 2.0, (HEAD_H - th as f64) / 2.0);
            if x + cw > HEAD_W {
                pangocairo::functions::show_layout(cr, &layout);
            }
            cr.set_source_rgb(hairline.0, hairline.1, hairline.2);
            cr.move_to((x + cw).floor() - 0.5, 4.0);
            cr.line_to((x + cw).floor() - 0.5, HEAD_H - 4.0);
            cr.stroke().ok();
        }
        // Repaint the corner over column labels that scrolled under it.
        cr.set_source_rgb(head_bg.0, head_bg.1, head_bg.2);
        cr.rectangle(0.0, 0.0, HEAD_W, h);
        cr.fill().ok();
        for &vi in &visible {
            let (y, rh) = self.row_rect(vi);
            if y + rh <= HEAD_H {
                continue;
            }
            let on = vi >= sel.top && vi <= sel.bottom;
            if on {
                cr.set_source_rgb(active_head.0, active_head.1, active_head.2);
                cr.rectangle(0.0, y.max(HEAD_H), HEAD_W, rh);
                cr.fill().ok();
            }
            layout.set_text(&(view[vi as usize] + 1).to_string());
            let (tw, th) = layout.pixel_size();
            let col = if on { text } else { dim };
            cr.set_source_rgb(col.0, col.1, col.2);
            cr.move_to(HEAD_W - 8.0 - tw as f64, y + (rh - th as f64) / 2.0);
            pangocairo::functions::show_layout(cr, &layout);
        }
        cr.set_source_rgb(head_bg.0, head_bg.1, head_bg.2);
        cr.rectangle(0.0, 0.0, HEAD_W, HEAD_H);
        cr.fill().ok();
        cr.set_source_rgb(hairline.0, hairline.1, hairline.2);
        cr.set_line_width(1.0);
        cr.move_to(0.0, HEAD_H - 0.5);
        cr.line_to(w, HEAD_H - 0.5);
        cr.move_to(HEAD_W - 0.5, 0.0);
        cr.line_to(HEAD_W - 0.5, h);
        cr.stroke().ok();
        if self.frozen() {
            let y = HEAD_H + self.frozen_h();
            cr.set_source_rgb(border.0, border.1, border.2);
            cr.set_line_width(2.0);
            cr.move_to(0.0, y);
            cr.line_to(w, y);
            cr.stroke().ok();
        }
    }

    // ---------- Mouse ----------

    fn fill_target(&self) -> Option<(u32, u32)> {
        self.fill_to.get()
    }

    /// The area a fill would cover, for drawing.
    fn fill_preview(&self) -> Area {
        let sel = self.selection();
        let Some((tr, tc)) = self.fill_target() else { return sel };
        let vd = if tr < sel.top { sel.top - tr } else { tr.saturating_sub(sel.bottom) };
        let hd = if tc < sel.left { sel.left - tc } else { tc.saturating_sub(sel.right) };
        if vd >= hd {
            Area::new(sel.top.min(tr), sel.left, sel.bottom.max(tr), sel.right)
        } else {
            Area::new(sel.top, sel.left.min(tc), sel.bottom, sel.right.max(tc))
        }
    }

    /// The editor being typed in: the cell's, or the formula bar.
    fn active_editor(&self) -> Option<gtk::Entry> {
        if self.fx.has_focus() {
            Some(self.fx.clone())
        } else if self.editing.get() {
            Some(self.entry.clone())
        } else {
            None
        }
    }

    /// Can a click put a reference into the formula being typed?
    fn can_pick(&self) -> Option<(gtk::Entry, i32)> {
        let e = self.active_editor()?;
        let text = e.text().to_string();
        if !text.starts_with('=') {
            return None;
        }
        let pos = e.position();
        let before: String = text.chars().take(pos as usize).collect();
        let last = before.trim_end().chars().last()?;
        if let Drag::Pick { at, len, .. } = self.drag.get()
            && at + len == pos
        {
            return Some((e, at));
        }
        "=(+-*/,:<>&^".contains(last).then_some((e, pos))
    }

    fn connect_mouse(self: &Rc<Self>) {
        let drag = gtk::GestureDrag::new();
        drag.set_button(1);
        let me = Rc::downgrade(self);
        drag.connect_drag_begin(move |g, x, y| {
            let Some(s) = me.upgrade() else { return };
            let shift = g.current_event_state().contains(gdk::ModifierType::SHIFT_MASK);
            s.press(x, y, shift);
        });
        let me = Rc::downgrade(self);
        drag.connect_drag_update(move |g, dx, dy| {
            let Some(s) = me.upgrade() else { return };
            if let Some((x0, y0)) = g.start_point() {
                s.drag_to(x0 + dx, y0 + dy, dx, dy);
            }
        });
        let me = Rc::downgrade(self);
        drag.connect_drag_end(move |_, _, _| {
            if let Some(s) = me.upgrade() {
                s.release();
            }
        });
        self.area.add_controller(drag);

        let click = gtk::GestureClick::new();
        click.set_button(1);
        let me = Rc::downgrade(self);
        click.connect_pressed(move |_, n, x, y| {
            let Some(s) = me.upgrade() else { return };
            if n == 2
                && let Hit::Cell(r, c) = s.hit(x, y)
                && !s.editing.get()
            {
                s.move_to(r, c, false);
                s.begin_edit(None, true);
            }
        });
        self.area.add_controller(click);

        let motion = gtk::EventControllerMotion::new();
        let me = Rc::downgrade(self);
        motion.connect_motion(move |_, x, y| {
            let Some(s) = me.upgrade() else { return };
            if s.drag.get() != Drag::None {
                return;
            }
            let name = match s.hit(x, y) {
                Hit::ColHead(_, true) => "col-resize",
                Hit::RowHead(_, true) => "row-resize",
                Hit::Handle => "crosshair",
                Hit::Cell(..) => "cell",
                _ => "default",
            };
            s.area.set_cursor_from_name(Some(name));
        });
        self.area.add_controller(motion);

        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        let me = Rc::downgrade(self);
        scroll.connect_scroll(move |c, dx, dy| {
            let Some(s) = me.upgrade() else { return glib::Propagation::Proceed };
            let shift = c.current_event_state().contains(gdk::ModifierType::SHIFT_MASK);
            let unit = if c.unit() == gdk::ScrollUnit::Surface { 1.0 } else { 60.0 };
            let (dx, dy) = if shift && dx == 0.0 { (dy, 0.0) } else { (dx, dy) };
            s.vadj.set_value(s.vadj.value() + dy * unit);
            s.hadj.set_value(s.hadj.value() + dx * unit);
            glib::Propagation::Stop
        });
        self.area.add_controller(scroll);
    }

    fn press(&self, x: f64, y: f64, shift: bool) {
        // Picking a cell for a formula being typed keeps the editor's focus.
        if let Hit::Cell(r, c) = self.hit(x, y)
            && let Some((e, at)) = self.can_pick()
        {
            let name = formula::cell_name(self.src_row(r), c);
            let len_before = if let Drag::Pick { at: a, len, .. } = self.drag.get()
                && a == at
            {
                len
            } else {
                0
            };
            e.delete_text(at, at + len_before);
            let mut pos = at;
            e.insert_text(&name, &mut pos);
            e.set_position(pos);
            self.drag.set(Drag::Pick { at, len: name.chars().count() as i32, from: (r, c) });
            return;
        }
        if self.editing.get() {
            self.end_edit(true);
        }
        self.area.grab_focus();
        let rows = self.view.borrow().len().max(1) as u32;
        match self.hit(x, y) {
            Hit::Corner => {
                self.anchor.set((0, 0));
                self.cursor.set((rows - 1, COLS - 1));
                self.sync();
            }
            Hit::ColHead(c, true) => {
                let w0 = self.wb.borrow().sheet().col_w(c);
                self.push_undo();
                self.drag.set(Drag::ResizeCol { c, x0: x, w0 });
            }
            Hit::RowHead(r, true) => {
                let h0 = self.wb.borrow().sheet().row_h(self.src_row(r));
                self.push_undo();
                self.drag.set(Drag::ResizeRow { r, y0: y, h0 });
            }
            Hit::ColHead(c, false) => {
                let ac = if shift { self.anchor.get().1 } else { c };
                self.anchor.set((0, ac));
                self.cursor.set((rows - 1, c));
                self.drag.set(Drag::Cols);
                self.sync();
            }
            Hit::RowHead(r, false) => {
                let ar = if shift { self.anchor.get().0 } else { r };
                self.anchor.set((ar, 0));
                self.cursor.set((r, COLS - 1));
                self.drag.set(Drag::Rows);
                self.sync();
            }
            Hit::Handle => {
                self.fill_to.set(None);
                self.drag.set(Drag::Fill);
            }
            Hit::Cell(r, c) => {
                self.cursor.set((r, c));
                if !shift {
                    self.anchor.set((r, c));
                }
                self.drag.set(Drag::Select);
                self.sync();
            }
            Hit::Outside => {}
        }
        self.area.queue_draw();
    }

    fn drag_to(&self, x: f64, y: f64, dx: f64, dy: f64) {
        // Past the edge, scroll along.
        let (w, h) = (self.area.width() as f64, self.area.height() as f64);
        if matches!(self.drag.get(), Drag::Select | Drag::Fill | Drag::Pick { .. } | Drag::Rows | Drag::Cols) {
            if y > h - 8.0 {
                self.vadj.set_value(self.vadj.value() + 20.0);
            } else if y < HEAD_H + 4.0 && self.vadj.value() > 0.0 {
                self.vadj.set_value(self.vadj.value() - 20.0);
            }
            if x > w - 8.0 {
                self.hadj.set_value(self.hadj.value() + 20.0);
            } else if x < HEAD_W + 4.0 && self.hadj.value() > 0.0 {
                self.hadj.set_value(self.hadj.value() - 20.0);
            }
        }
        let (r, c) = self.cell_near(x, y);
        match self.drag.get() {
            Drag::Select => {
                self.cursor.set((r, c));
                self.sync();
            }
            Drag::Cols => {
                self.cursor.set((self.cursor.get().0, c));
                self.sync();
            }
            Drag::Rows => {
                self.cursor.set((r, self.cursor.get().1));
                self.sync();
            }
            Drag::Fill => self.fill_to.set(Some((r, c))),
            Drag::Pick { at, len, from } => {
                let Some(e) = self.active_editor() else { return };
                let a = Area::new(self.src_row(from.0), from.1, self.src_row(r), c);
                let name = a.name();
                e.delete_text(at, at + len);
                let mut pos = at;
                e.insert_text(&name, &mut pos);
                e.set_position(pos);
                self.drag.set(Drag::Pick { at, len: name.chars().count() as i32, from });
            }
            Drag::ResizeCol { c, x0: _, w0 } => {
                let nw = (w0 as f64 + dx).round().max(36.0) as u32;
                self.wb.borrow_mut().set_col_w(c, nw);
                self.relayout();
            }
            Drag::ResizeRow { r, y0: _, h0 } => {
                let nh = (h0 as f64 + dy).round().max(20.0) as u32;
                let src = self.src_row(r);
                self.wb.borrow_mut().set_row_h(src, nh);
                self.relayout();
            }
            Drag::None => {}
        }
        self.area.queue_draw();
    }

    fn release(&self) {
        match self.drag.get() {
            Drag::Fill => {
                if let Some(to) = self.fill_to.take() {
                    let src = self.selection_src();
                    let to = (self.src_row(to.0), to.1);
                    let mut done = src;
                    self.change(|wb| done = wb.fill(src, to));
                    self.select_src(done);
                }
            }
            Drag::ResizeCol { .. } | Drag::ResizeRow { .. } => {
                // The undo step was noted when the drag began.
                if self.undo.borrow().last() == Some(&*self.wb.borrow()) {
                    self.undo.borrow_mut().pop();
                } else {
                    self.redo.borrow_mut().clear();
                    crate::doc::edited();
                }
            }
            // Keep the pick open so a following click replaces it.
            Drag::Pick { .. } => return,
            _ => {}
        }
        self.drag.set(Drag::None);
        self.area.queue_draw();
    }

    fn push_undo(&self) {
        let snap = self.wb.borrow().clone();
        self.undo.borrow_mut().push(snap);
    }

    // ---------- Keys ----------

    fn connect_keys(self: &Rc<Self>) {
        let keys = gtk::EventControllerKey::new();
        let me = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, mods| {
            let Some(s) = me.upgrade() else { return glib::Propagation::Proceed };
            if s.grid_key(key, mods) { glib::Propagation::Stop } else { glib::Propagation::Proceed }
        });
        self.area.add_controller(keys);
    }

    fn grid_key(&self, key: gdk::Key, mods: gdk::ModifierType) -> bool {
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
        let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
        let alt = mods.contains(gdk::ModifierType::ALT_MASK);
        let (r, c) = self.cursor.get();
        let rows = self.view.borrow().len().max(1) as u32;
        let page = ((self.area.height() as f64 - HEAD_H) / ROW_H as f64).max(1.0) as u32;
        match key {
            gdk::Key::Up | gdk::Key::Down | gdk::Key::Left | gdk::Key::Right => {
                let (dr, dc) = match key {
                    gdk::Key::Up => (-1, 0),
                    gdk::Key::Down => (1, 0),
                    gdk::Key::Left => (0, -1),
                    _ => (0, 1),
                };
                let (nr, nc) = if ctrl {
                    self.jump(dr, dc)
                } else if shift {
                    ((r as i64 + dr).max(0) as u32, (c as i64 + dc).max(0) as u32)
                } else {
                    self.step(dr, dc)
                };
                self.move_to(nr, nc, shift);
            }
            gdk::Key::Page_Down if !ctrl => self.move_to(r + page, c, shift),
            gdk::Key::Page_Up if !ctrl => self.move_to(r.saturating_sub(page), c, shift),
            gdk::Key::Home if ctrl => self.move_to(0, 0, shift),
            gdk::Key::Home => self.move_to(r, 0, shift),
            gdk::Key::End if ctrl => {
                let (lr, lc) = self.wb.borrow().sheet().used().unwrap_or((0, 0));
                self.move_to(lr, lc, shift);
            }
            gdk::Key::Tab | gdk::Key::ISO_Left_Tab => {
                let back = shift || key == gdk::Key::ISO_Left_Tab;
                let (nr, nc) = self.step(0, if back { -1 } else { 1 });
                self.move_to(nr, nc, false);
            }
            gdk::Key::Return | gdk::Key::KP_Enter => {
                let (nr, nc) = self.step(if shift { -1 } else { 1 }, 0);
                self.move_to(nr.min(rows - 1), nc, false)
            }
            gdk::Key::F2 => self.begin_edit(None, true),
            gdk::Key::Delete | gdk::Key::KP_Delete => {
                let areas = self.selection_rows();
                self.change(|wb| {
                    for a in areas {
                        wb.clear(a);
                    }
                });
            }
            gdk::Key::BackSpace => {
                self.change(|wb| wb.set(self.src_row(r), c, ""));
                self.begin_edit(Some(""), true);
            }
            gdk::Key::Escape => {
                if self.clip.borrow().is_some() {
                    *self.clip.borrow_mut() = None;
                    self.area.queue_draw();
                } else {
                    return false;
                }
            }
            _ if ctrl && !alt => match key.to_lower() {
                gdk::Key::a => {
                    self.anchor.set((0, 0));
                    self.cursor.set((rows - 1, COLS - 1));
                    self.sync();
                    self.area.queue_draw();
                }
                gdk::Key::c => self.copy(false),
                gdk::Key::x => self.copy(true),
                gdk::Key::v => self.paste(),
                gdk::Key::d => self.fill_down_or_right(true),
                gdk::Key::r => self.fill_down_or_right(false),
                _ => return false,
            },
            _ if !ctrl && !alt => {
                // Typing starts a new entry in the cell.
                let Some(ch) = key.to_unicode().filter(|c| !c.is_control()) else { return false };
                self.begin_edit(Some(&ch.to_string()), true);
            }
            _ => return false,
        }
        true
    }

    // ---------- The cell editor ----------

    fn place_entry(&self) {
        if !self.editing.get() {
            return;
        }
        let (r, c) = self.cursor.get();
        let (x, y, w, h) = self.area_rect(self.cursor_merge().unwrap_or(Area::cell(r, c)));
        self.entry.set_margin_start(x.max(HEAD_W) as i32);
        self.entry.set_margin_top(y.max(HEAD_H) as i32);
        // Long entries widen the editor, as far as the grid allows.
        let chars = self.entry.text().chars().count() as f64 * 8.0 + 24.0;
        let width = w.max(chars).min((self.area.width() as f64 - x).max(w));
        self.entry.set_size_request(width as i32, h as i32);
    }

    /// Start editing the active cell: with `initial` text (typing replaces), or what's there.
    pub(super) fn begin_edit(&self, initial: Option<&str>, focus: bool) {
        if !self.editing.get() {
            self.anchor.set(self.cursor.get());
        }
        let (r, c) = self.cursor.get();
        self.scroll_to(r, c);
        let raw = self.wb.borrow().sheet().raw(self.src_row(r), c).to_string();
        self.editing.set(true);
        self.syncing.set(true);
        self.entry.set_text(initial.unwrap_or(&raw));
        self.syncing.set(false);
        self.entry.set_visible(true);
        self.place_entry();
        if focus {
            self.entry.grab_focus();
            self.entry.set_position(-1);
        }
        self.area.queue_draw();
        crate::doc::status_changed();
    }

    pub(super) fn end_edit(&self, commit: bool) {
        if !self.editing.get() {
            return;
        }
        let text = self.entry.text().to_string();
        self.editing.set(false);
        self.entry.set_visible(false);
        if matches!(self.drag.get(), Drag::Pick { .. }) {
            self.drag.set(Drag::None);
        }
        if commit {
            let (r, c) = self.cursor.get();
            self.commit_raw(r, c, &text);
        }
        self.sync();
        self.area.queue_draw();
    }

    fn connect_entry(self: &Rc<Self>) {
        let me = Rc::downgrade(self);
        self.entry.connect_changed(move |e| {
            let Some(s) = me.upgrade() else { return };
            if s.syncing.get() {
                return;
            }
            // Typing ends a reference pick.
            if let Drag::Pick { at, len, .. } = s.drag.get()
                && e.position() != at + len
            {
                s.drag.set(Drag::None);
            }
            s.syncing.set(true);
            s.fx.set_text(&e.text());
            s.syncing.set(false);
            s.place_entry();
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let me = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, mods| {
            let Some(s) = me.upgrade() else { return glib::Propagation::Proceed };
            let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
            let (r, c) = s.cursor.get();
            let formula = s.entry.text().starts_with('=');
            // Any key but a pick ends the pick.
            if let Drag::Pick { .. } = s.drag.get()
                && !matches!(key, gdk::Key::Shift_L | gdk::Key::Shift_R)
            {
                s.drag.set(Drag::None);
            }
            let next = match key {
                gdk::Key::Return | gdk::Key::KP_Enter => Some(if shift { (r.saturating_sub(1), c) } else { (r + 1, c) }),
                gdk::Key::Tab | gdk::Key::ISO_Left_Tab => {
                    Some(if shift || key == gdk::Key::ISO_Left_Tab { (r, c.saturating_sub(1)) } else { (r, c + 1) })
                }
                // Arrows move the text cursor in a formula, and leave a plain entry.
                gdk::Key::Up if !formula => Some((r.saturating_sub(1), c)),
                gdk::Key::Down if !formula => Some((r + 1, c)),
                gdk::Key::Escape => {
                    s.end_edit(false);
                    s.area.grab_focus();
                    return glib::Propagation::Stop;
                }
                _ => None,
            };
            match next {
                Some((nr, nc)) => {
                    s.end_edit(true);
                    s.move_to(nr, nc, false);
                    s.area.grab_focus();
                    glib::Propagation::Stop
                }
                None => glib::Propagation::Proceed,
            }
        });
        self.entry.add_controller(keys);
    }
}

fn widgets_hbox() -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Horizontal, 0)
}
