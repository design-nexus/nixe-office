//! The right-click menu on cells and on row and column headings: cut, copy,
//! paste, inserting or deleting rows and columns, and hiding or showing them.

use super::Sheets;
use super::grid::Hit;
use super::model::*;
use super::structure::{Axis, Edit};
use crate::{fmt, widgets, window};
use gtk::gdk;
use gtk::prelude::*;
use std::rc::Rc;

impl Sheets {
    pub(super) fn connect_menu(self: &Rc<Self>) {
        let click = gtk::GestureClick::new();
        click.set_button(gdk::BUTTON_SECONDARY);
        let me = self.weak();
        click.connect_pressed(move |g, _, x, y| {
            let Some(s) = me.upgrade() else { return };
            g.set_state(gtk::EventSequenceState::Claimed);
            s.menu_at(x, y);
        });
        self.area.add_controller(click);
    }

    /// Select what was clicked (unless it's already in the selection), then
    /// show the menu there.
    fn menu_at(&self, x: f64, y: f64) {
        if self.editing.get() {
            self.end_edit(true);
        }
        self.area.grab_focus();
        let rows = self.view.borrow().len().max(1) as u32;
        let sel = self.selection();
        match self.hit(x, y) {
            Hit::Cell(r, c) if !sel.contains(r, c) => {
                self.anchor.set((r, c));
                self.cursor.set((r, c));
            }
            Hit::RowHead(r, _) if !(sel.contains(r, 0) && self.whole_rows(sel)) => {
                self.anchor.set((r, 0));
                self.cursor.set((r, COLS - 1));
            }
            Hit::ColHead(c, _) if !(sel.contains(0, c) && self.whole_cols(sel)) => {
                self.anchor.set((0, c));
                self.cursor.set((rows - 1, c));
            }
            Hit::Corner => {
                self.anchor.set((0, 0));
                self.cursor.set((rows - 1, COLS - 1));
            }
            Hit::Cell(..) | Hit::RowHead(..) | Hit::ColHead(..) | Hit::Edge | Hit::Handle => {}
            Hit::Outside => return,
        }
        self.sync();
        self.area.queue_draw();
        self.show_menu(x, y);
    }

    fn whole_rows(&self, a: Area) -> bool {
        a.left == 0 && a.right == COLS - 1
    }

    pub(super) fn whole_cols(&self, a: Area) -> bool {
        a.top == 0 && a.bottom as usize + 1 >= self.view.borrow().len()
    }

    fn show_menu(&self, x: f64, y: f64) {
        let sel = self.selection();
        let (rows, cols) = (self.whole_rows(sel), self.whole_cols(sel));
        let (nr, nc) = (sel.rows(), sel.cols());

        let pop = gtk::Popover::new();
        pop.add_css_class("menu-popover");
        pop.set_has_arrow(false);
        pop.set_position(gtk::PositionType::Bottom);
        pop.set_halign(gtk::Align::Start);
        pop.set_parent(&self.area);
        pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        let list = widgets::vbox(0);
        let item = |label: &str, danger: bool, f: Box<dyn Fn(&Sheets)>| {
            let b = gtk::Button::with_label(label);
            b.add_css_class("menu-item");
            if danger {
                b.add_css_class("danger-text");
            }
            if let Some(l) = b.child().and_downcast::<gtk::Label>() {
                l.set_xalign(0.0);
            }
            let me = self.weak();
            let p = pop.clone();
            b.connect_clicked(move |_| {
                p.popdown();
                if let Some(s) = me.upgrade() {
                    f(&s);
                }
            });
            list.append(&b);
        };
        item("Cut", false, Box::new(|s| s.copy(true)));
        item("Copy", false, Box::new(|s| s.copy(false)));
        item("Paste", false, Box::new(|s| s.paste()));
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let r = fmt::count(nr as usize, "row", "rows");
        let c = fmt::count(nc as usize, "column", "columns");
        if !cols {
            item(&format!("Insert {r} above"), false, Box::new(move |s| s.insert_rows(false)));
            item(&format!("Insert {r} below"), false, Box::new(move |s| s.insert_rows(true)));
        }
        if !rows {
            item(&format!("Insert {c} left"), false, Box::new(move |s| s.insert_cols(false)));
            item(&format!("Insert {c} right"), false, Box::new(move |s| s.insert_cols(true)));
        }
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        if !cols {
            item(&format!("Delete {r}"), true, Box::new(|s| s.delete_rows()));
        }
        if !rows {
            item(&format!("Delete {c}"), true, Box::new(|s| s.delete_cols()));
        }
        let (any_rows, any_cols) = self.hidden_in_selection();
        if (rows && !cols) || (cols && !rows) || any_rows || any_cols {
            list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }
        if rows && !cols {
            item(&format!("Hide {r}"), false, Box::new(|s| s.hide_rows(true)));
        }
        if cols && !rows {
            item(&format!("Hide {c}"), false, Box::new(|s| s.hide_cols(true)));
        }
        if any_rows {
            item("Show hidden rows", false, Box::new(|s| s.hide_rows(false)));
        }
        if any_cols {
            item("Show hidden columns", false, Box::new(|s| s.hide_cols(false)));
        }
        pop.set_child(Some(&list));
        pop.connect_closed(|p| p.unparent());
        pop.popup();
    }

    /// The sheet rows the selection spans.
    fn selected_src_rows(&self) -> Vec<u32> {
        let sel = self.selection();
        (sel.top..=sel.bottom).map(|r| self.src_row(r)).collect()
    }

    /// Whether the selection spans hidden rows, and hidden columns.
    fn hidden_in_selection(&self) -> (bool, bool) {
        let sel = self.selection();
        let wb = self.wb.borrow();
        let s = wb.sheet();
        let rows = self.selected_src_rows().iter().any(|r| s.hidden_rows.contains(r));
        let cols = s.hidden_cols.range(sel.left..=sel.right).next().is_some();
        (rows, cols)
    }

    /// Hide the selected rows, or show the hidden ones among them (Ctrl+9, Ctrl+Shift+9).
    pub(super) fn hide_rows(&self, hide: bool) {
        let rows = self.selected_src_rows();
        if hide && rows.len() >= self.view.borrow().len() {
            window::toast("At least one row has to stay in view.");
            return;
        }
        self.change(|wb| {
            for &r in &rows {
                wb.hide_rows(r, r, hide);
            }
        });
        self.after_hiding(1, 0);
    }

    /// Hide the selected columns, or show the hidden ones among them (Ctrl+0, Ctrl+Shift+0).
    pub(super) fn hide_cols(&self, hide: bool) {
        let sel = self.selection();
        if hide && sel.cols() >= COLS {
            window::toast("At least one column has to stay in view.");
            return;
        }
        self.change(|wb| wb.hide_cols(sel.left, sel.right, hide));
        self.after_hiding(0, 1);
    }

    /// Keep the cursor on a cell that's shown.
    fn after_hiding(&self, dr: i64, dc: i64) {
        let (r, c) = self.cursor.get();
        let (nr, nc) = self.past_hidden(r, c, dr, dc);
        let (nr, nc) = self.past_hidden(nr, nc, -dr, -dc);
        self.anchor.set((nr, nc));
        self.cursor.set((nr, nc));
        self.sync();
        self.area.queue_draw();
    }

    /// Rows can't be put in or taken out while a filter hides some.
    fn rows_editable(&self) -> bool {
        if !self.merges_shown() {
            window::toast("Turn the filter off to insert or delete rows.");
            return false;
        }
        true
    }

    fn apply(&self, edit: Edit) -> bool {
        let mut done = false;
        self.change(|wb| {
            let si = wb.active;
            done = wb.restructure(si, edit);
        });
        if !done && edit.insert {
            let what = if edit.axis == Axis::Rows { "rows" } else { "columns" };
            window::toast(&format!("There's no room: the last {what} of the sheet aren't empty."));
        }
        done
    }

    pub(super) fn insert_rows(&self, below: bool) {
        if !self.rows_editable() {
            return;
        }
        let sel = self.selection();
        let at = if below { sel.bottom + 1 } else { sel.top };
        if at >= ROWS {
            return;
        }
        let n = sel.rows().min(ROWS - at);
        if self.apply(Edit { axis: Axis::Rows, at, n, insert: true }) {
            // The new rows are selected, across the same columns.
            self.anchor.set((at, sel.left));
            self.cursor.set((at + n - 1, sel.right));
            self.sync();
        }
    }

    pub(super) fn insert_cols(&self, right: bool) {
        let sel = self.selection();
        let at = if right { sel.right + 1 } else { sel.left };
        if at >= COLS {
            return;
        }
        let n = sel.cols().min(COLS - at);
        if self.apply(Edit { axis: Axis::Cols, at, n, insert: true }) {
            self.anchor.set((sel.top, at));
            self.cursor.set((sel.bottom, at + n - 1));
            self.sync();
        }
    }

    pub(super) fn delete_rows(&self) {
        if !self.rows_editable() {
            return;
        }
        let sel = self.selection();
        if self.apply(Edit { axis: Axis::Rows, at: sel.top, n: sel.rows(), insert: false }) {
            let r = sel.top.min(ROWS - 1);
            self.anchor.set((r, sel.left));
            self.cursor.set((r, sel.left));
            self.sync();
        }
    }

    pub(super) fn delete_cols(&self) {
        let sel = self.selection();
        if self.apply(Edit { axis: Axis::Cols, at: sel.left, n: sel.cols(), insert: false }) {
            let c = sel.left.min(COLS - 1);
            self.anchor.set((sel.top, c));
            self.cursor.set((sel.top, c));
            self.sync();
        }
    }
}
