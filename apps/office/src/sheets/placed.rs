//! Charts placed on the sheet over the cells, and blocks of cells styled as tables.

use super::model::*;
use super::{Drag, Sheets, chart};
use crate::{widgets, window};
use gtk::prelude::*;
use std::rc::Rc;

/// The grid's header sizes, where the cells start on screen.
use super::grid::{HEAD_H, HEAD_W};

impl Sheets {
    /// Where a placed chart is on screen: x, y, width, height.
    pub(super) fn chart_rect(&self, c: &PlacedChart) -> (f64, f64, f64, f64) {
        (HEAD_W + c.x - self.hadj.value(), HEAD_H + c.y - self.vadj.value(), c.w, c.h)
    }

    /// The topmost placed chart under (x, y), and whether it's at its resize corner.
    pub(super) fn chart_hit(&self, x: f64, y: f64) -> Option<(usize, bool)> {
        if x < HEAD_W || y < HEAD_H {
            return None;
        }
        let wb = self.wb.borrow();
        wb.sheet().charts.iter().enumerate().rev().find_map(|(i, c)| {
            let (cx, cy, cw, ch) = self.chart_rect(c);
            let corner = (x - (cx + cw)).abs() <= 8.0 && (y - (cy + ch)).abs() <= 8.0;
            (corner || (x >= cx && x <= cx + cw && y >= cy && y <= cy + ch)).then_some((i, corner))
        })
    }

    /// Draw the placed charts over the cells, the picked one outlined.
    pub(super) fn draw_placed(&self, cr: &cairo::Context, (wb, calc): (&Workbook, &Calc), (w, h): (f64, f64), accent: (f64, f64, f64)) {
        let s = wb.sheet();
        if s.charts.is_empty() {
            return;
        }
        let ctx = crate::print::text_context();
        cr.save().ok();
        cr.rectangle(HEAD_W, HEAD_H, w - HEAD_W, h - HEAD_H);
        cr.clip();
        for (i, c) in s.charts.iter().enumerate() {
            let (x, y, cw, ch) = self.chart_rect(c);
            if x > w || y > h || x + cw < HEAD_W || y + ch < HEAD_H {
                continue;
            }
            // A card with a soft edge, the chart inside.
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.08);
            cr.rectangle(x + 2.0, y + 3.0, cw, ch);
            cr.fill().ok();
            match wb.chart_data_for(wb.active, &c.range, calc) {
                Some((names, labels, series)) => {
                    chart::draw(cr, &ctx, c.kind, &chart::Data { names: &names, labels: &labels, series: &series }, (x, y, cw, ch))
                }
                None => {
                    cr.set_source_rgb(1.0, 1.0, 1.0);
                    cr.rectangle(x, y, cw, ch);
                    cr.fill().ok();
                }
            }
            cr.set_source_rgb(0.82, 0.84, 0.87);
            cr.set_line_width(1.0);
            cr.rectangle(x + 0.5, y + 0.5, cw - 1.0, ch - 1.0);
            cr.stroke().ok();
            if self.chart_sel.get() == Some(i) {
                cr.set_source_rgb(accent.0, accent.1, accent.2);
                cr.set_line_width(2.0);
                cr.rectangle(x, y, cw, ch);
                cr.stroke().ok();
                cr.rectangle(x + cw - 5.0, y + ch - 5.0, 10.0, 10.0);
                cr.fill().ok();
            }
        }
        cr.restore().ok();
    }

    /// A press on a placed chart: pick it, and start moving or resizing it.
    pub(super) fn press_chart(&self, x: f64, y: f64) -> bool {
        let Some((i, corner)) = self.chart_hit(x, y) else {
            if self.chart_sel.take().is_some() {
                self.area.queue_draw();
            }
            return false;
        };
        self.end_edit(true);
        self.chart_sel.set(Some(i));
        let c = self.wb.borrow().sheet().charts[i].clone();
        self.push_undo();
        self.drag.set(if corner { Drag::SizeChart { i, w0: c.w, h0: c.h } } else { Drag::MoveChart { i, x0: c.x, y0: c.y } });
        self.area.grab_focus();
        self.area.queue_draw();
        true
    }

    pub(super) fn drag_chart(&self, dx: f64, dy: f64) {
        let mut wb = self.wb.borrow_mut();
        let charts = &mut wb.sheet_mut().charts;
        match self.drag.get() {
            Drag::MoveChart { i, x0, y0 } => {
                if let Some(c) = charts.get_mut(i) {
                    c.x = (x0 + dx).max(0.0);
                    c.y = (y0 + dy).max(0.0);
                }
            }
            Drag::SizeChart { i, w0, h0 } => {
                if let Some(c) = charts.get_mut(i) {
                    c.w = (w0 + dx).clamp(160.0, 2000.0);
                    c.h = (h0 + dy).clamp(120.0, 1400.0);
                }
            }
            _ => {}
        }
        drop(wb);
        self.area.queue_draw();
    }

    /// Put a chart of the selection on the sheet, beside it.
    pub(super) fn place_chart(&self) {
        let sel = self.selection_src();
        let range = if sel.is_cell() {
            match self.wb.borrow().sheet().used() {
                Some((r, c)) => Area::new(0, 0, r, c.min(CHART_SERIES as u32)),
                None => {
                    window::toast("Put some numbers on the sheet to chart them.");
                    return;
                }
            }
        } else {
            sel
        };
        let (x, y) = {
            let xs = self.col_x.borrow();
            let ys = self.row_y.borrow();
            let right = xs.get(range.right as usize + 1).copied().unwrap_or(0.0);
            let top = ys.get(range.top as usize).copied().unwrap_or(0.0);
            (right + 24.0, top)
        };
        let n = self.wb.borrow().sheet().charts.len();
        self.change(|wb| {
            wb.sheet_mut().charts.push(PlacedChart { range: range.name(), kind: ChartKind::Bar, x, y, w: 420.0, h: 260.0 })
        });
        self.chart_sel.set(Some(n));
        self.area.queue_draw();
    }

    pub(super) fn delete_chart(&self) {
        let Some(i) = self.chart_sel.take() else { return };
        self.change(|wb| {
            if i < wb.sheet().charts.len() {
                wb.sheet_mut().charts.remove(i);
            }
        });
    }

    /// Change a placed chart's cells and kind.
    pub(super) fn edit_chart(&self, i: usize) {
        let Some(c) = self.wb.borrow().sheet().charts.get(i).cloned() else { return };
        let (dialog, card) = widgets::dialog("Chart", 380);
        let range = gtk::Entry::new();
        range.set_text(&c.range);
        let kinds: Vec<&str> = ChartKind::ALL.iter().map(|k| k.name()).collect();
        let kind = gtk::DropDown::from_strings(&kinds);
        kind.set_selected(ChartKind::ALL.iter().position(|k| *k == c.kind).unwrap_or(0) as u32);
        let list = widgets::vbox(0);
        list.add_css_class("group-list");
        list.append(&widgets::row("Cells", "Labels in the first column, one series per other column.", Some(range.upcast_ref())));
        list.append(&widgets::row("Kind", "", Some(kind.upcast_ref())));
        card.append(&list);
        let err = widgets::label("", "danger-text");
        err.set_visible(false);
        card.append(&err);
        let buttons = widgets::hbox(8);
        let remove = gtk::Button::with_label("Remove chart");
        remove.add_css_class("danger-text");
        let spacer = widgets::hbox(0);
        spacer.set_hexpand(true);
        let ok = gtk::Button::with_label("Done");
        ok.add_css_class("suggested-action");
        buttons.append(&remove);
        buttons.append(&spacer);
        buttons.append(&ok);
        card.append(&buttons);
        let (me, d) = (self.weak(), dialog.clone());
        remove.connect_clicked(move |_| {
            d.close();
            if let Some(s) = me.upgrade() {
                s.chart_sel.set(Some(i));
                s.delete_chart();
            }
        });
        let (me, d) = (self.weak(), dialog.clone());
        ok.connect_clicked(move |_| {
            let Some(s) = me.upgrade() else { return };
            let Some(a) = Area::parse(&range.text()).filter(|a| !a.is_cell()) else {
                err.set_text("Type the cells as a range, like A1:C12.");
                err.set_visible(true);
                return;
            };
            d.close();
            let k = ChartKind::ALL[(kind.selected() as usize).min(ChartKind::ALL.len() - 1)];
            s.change(|wb| {
                if let Some(c) = wb.sheet_mut().charts.get_mut(i) {
                    c.range = a.name();
                    c.kind = k;
                }
            });
        });
        dialog.present();
    }

    // ---------- Tables ----------

    /// Style the selection (or the table the cursor is in) as a table.
    pub(super) fn table_dialog(self: &Rc<Self>) {
        self.end_edit(true);
        let sel = self.selection_src();
        let existing = {
            let wb = self.wb.borrow();
            wb.sheet().tables.iter().position(|t| t.area().is_some_and(|a| a.overlaps(&sel)))
        };
        let i = match existing {
            Some(i) => i,
            None => {
                let wb = self.wb.borrow();
                let s = wb.sheet();
                // A single cell: the block of data around it.
                let area = if sel.is_cell() {
                    match s.used() {
                        Some((r, c)) => Area::new(0, 0, r, c),
                        None => {
                            window::toast("Put some data on the sheet, then make it a table.");
                            return;
                        }
                    }
                } else {
                    sel
                };
                if area.rows() < 2 {
                    window::toast("A table needs a heading row and at least one row under it.");
                    return;
                }
                let n = s.tables.len();
                let name = (1..).map(|k| format!("Table{k}")).find(|nm| !s.tables.iter().any(|t| &t.name == nm)).unwrap_or_default();
                drop(wb);
                self.change(|wb| {
                    wb.sheet_mut().tables.push(TableRegion {
                        name,
                        range: area.name(),
                        look: n % TABLE_LOOKS.len(),
                        header: true,
                        banded: true,
                        total: false,
                    })
                });
                n
            }
        };
        let Some(t) = self.wb.borrow().sheet().tables.get(i).cloned() else { return };
        let (dialog, card) = widgets::dialog(&format!("Table · {}", t.range), 380);
        let looks: Vec<&str> = TABLE_LOOKS.iter().map(|l| l.0).collect();
        let look = gtk::DropDown::from_strings(&looks);
        look.set_selected(t.look.min(TABLE_LOOKS.len() - 1) as u32);
        let header = gtk::Switch::new();
        header.set_active(t.header);
        let banded = gtk::Switch::new();
        banded.set_active(t.banded);
        let total = gtk::Switch::new();
        total.set_active(t.total);
        let list = widgets::vbox(0);
        list.add_css_class("group-list");
        list.append(&widgets::row("Colour", "", Some(look.upcast_ref())));
        list.append(&widgets::row("Heading row", "The first row names the columns.", Some(header.upcast_ref())));
        list.append(&widgets::row("Banded rows", "Every other row shaded.", Some(banded.upcast_ref())));
        list.append(&widgets::row("Total row", "The last row stands apart, in bold.", Some(total.upcast_ref())));
        card.append(&list);
        let apply = {
            let (me, look, header, banded, total) = (self.weak(), look.clone(), header.clone(), banded.clone(), total.clone());
            move || {
                let Some(s) = me.upgrade() else { return };
                s.change(|wb| {
                    if let Some(t) = wb.sheet_mut().tables.get_mut(i) {
                        t.look = look.selected() as usize;
                        t.header = header.is_active();
                        t.banded = banded.is_active();
                        t.total = total.is_active();
                    }
                });
            }
        };
        let a = apply.clone();
        look.connect_selected_notify(move |_| a());
        for sw in [&header, &banded, &total] {
            let a = apply.clone();
            sw.connect_active_notify(move |_| a());
        }
        let buttons = widgets::hbox(8);
        let remove = gtk::Button::with_label("Not a table");
        remove.set_tooltip_text(Some("Keep the cells, drop the table's look"));
        let spacer = widgets::hbox(0);
        spacer.set_hexpand(true);
        let ok = gtk::Button::with_label("Done");
        ok.add_css_class("suggested-action");
        buttons.append(&remove);
        buttons.append(&spacer);
        buttons.append(&ok);
        card.append(&buttons);
        let (me, d) = (self.weak(), dialog.clone());
        remove.connect_clicked(move |_| {
            d.close();
            if let Some(s) = me.upgrade() {
                s.change(|wb| {
                    if i < wb.sheet().tables.len() {
                        wb.sheet_mut().tables.remove(i);
                    }
                });
            }
        });
        let d = dialog.clone();
        ok.connect_clicked(move |_| d.close());
        dialog.present();
    }
}
