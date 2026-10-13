//! A table in the text: a grid of single-line cells sitting on its own line,
//! as wide as the text. Right-click a cell for row and column commands.

use super::model::Table;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct TableWidget {
    pub grid: gtk::Grid,
    cells: RefCell<Vec<Vec<gtk::Entry>>>,
    header: Cell<bool>,
    /// The cell last focused: (row, column).
    current: Cell<(usize, usize)>,
    /// Called on every change to a cell or to the table's shape.
    on_change: Rc<dyn Fn()>,
    /// Called when the table asks to be removed from the text.
    on_delete: RefCell<Option<Rc<dyn Fn()>>>,
}

impl TableWidget {
    pub fn new(table: &Table, width_px: i32, on_change: Rc<dyn Fn()>) -> Rc<TableWidget> {
        let grid = gtk::Grid::new();
        grid.add_css_class("doc-table");
        grid.set_column_homogeneous(true);
        grid.set_size_request(width_px, -1);
        let t = Rc::new(TableWidget {
            grid,
            cells: RefCell::new(Vec::new()),
            header: Cell::new(table.header),
            current: Cell::new((0, 0)),
            on_change,
            on_delete: RefCell::new(None),
        });
        install_actions(&t);
        t.rebuild(&table.rows);
        t
    }

    pub fn set_on_delete(&self, f: Rc<dyn Fn()>) {
        self.on_delete.replace(Some(f));
    }

    pub fn set_width(&self, width_px: i32) {
        self.grid.set_size_request(width_px, -1);
    }

    pub fn model(&self) -> Table {
        let rows = self.cells.borrow().iter().map(|r| r.iter().map(|e| e.text().to_string()).collect()).collect();
        Table { rows, header: self.header.get() }
    }

    /// All the cells, row by row, for find and replace.
    pub fn entries(&self) -> Vec<gtk::Entry> {
        self.cells.borrow().iter().flatten().cloned().collect()
    }

    fn rebuild(self: &Rc<Self>, rows: &[Vec<String>]) {
        while let Some(c) = self.grid.first_child() {
            self.grid.remove(&c);
        }
        let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
        let mut cells = Vec::new();
        let menu = cell_menu();
        for (r, row) in rows.iter().enumerate() {
            let mut line = Vec::new();
            for c in 0..cols {
                let e = gtk::Entry::new();
                e.add_css_class("doc-cell");
                if r == 0 && self.header.get() {
                    e.add_css_class("header");
                }
                e.set_hexpand(true);
                e.set_width_chars(1);
                e.set_text(row.get(c).map(String::as_str).unwrap_or(""));
                e.set_extra_menu(Some(&menu));
                let focus = gtk::EventControllerFocus::new();
                let weak = Rc::downgrade(self);
                focus.connect_enter(move |_| {
                    if let Some(t) = weak.upgrade() {
                        t.current.set((r, c));
                    }
                });
                e.add_controller(focus);
                let changed = self.on_change.clone();
                e.connect_changed(move |_| changed());
                // Tab moves on through the cells, and Enter goes down a row.
                let weak = Rc::downgrade(self);
                let keys = gtk::EventControllerKey::new();
                keys.connect_key_pressed(move |_, key, _, mods| {
                    let Some(t) = weak.upgrade() else { return glib::Propagation::Proceed };
                    let back = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
                    match key {
                        gtk::gdk::Key::Tab | gtk::gdk::Key::ISO_Left_Tab => {
                            t.step(r, c, if back || key == gtk::gdk::Key::ISO_Left_Tab { -1 } else { 1 });
                            glib::Propagation::Stop
                        }
                        gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter => {
                            t.focus_cell(r + 1, c);
                            glib::Propagation::Stop
                        }
                        _ => glib::Propagation::Proceed,
                    }
                });
                e.add_controller(keys);
                self.grid.attach(&e, c as i32, r as i32, 1, 1);
                line.push(e);
            }
            cells.push(line);
        }
        self.cells.replace(cells);
    }

    fn step(self: &Rc<Self>, r: usize, c: usize, dir: i32) {
        let (rows, cols) = self.shape();
        let i = (r * cols + c) as i32 + dir;
        if i < 0 {
            return;
        }
        let i = i as usize;
        if i >= rows * cols {
            // Tab in the last cell adds a row, as in other word processors.
            self.edit(|rows| rows.push(vec![String::new(); cols]));
        }
        self.focus_cell(i / cols, i % cols);
    }

    fn focus_cell(&self, r: usize, c: usize) {
        if let Some(e) = self.cells.borrow().get(r).and_then(|row| row.get(c)) {
            e.grab_focus();
        }
    }

    fn shape(&self) -> (usize, usize) {
        let cells = self.cells.borrow();
        (cells.len(), cells.first().map(Vec::len).unwrap_or(0))
    }

    fn edit(self: &Rc<Self>, f: impl FnOnce(&mut Vec<Vec<String>>)) {
        let mut rows = self.model().rows;
        f(&mut rows);
        if rows.is_empty() || rows[0].is_empty() {
            if let Some(d) = self.on_delete.borrow().clone() {
                d();
            }
            return;
        }
        self.rebuild(&rows);
        (self.on_change)();
    }
}

fn cell_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let rows = gio::Menu::new();
    rows.append(Some("Add row above"), Some("table.row-above"));
    rows.append(Some("Add row below"), Some("table.row-below"));
    rows.append(Some("Add column left"), Some("table.col-left"));
    rows.append(Some("Add column right"), Some("table.col-right"));
    menu.append_section(None, &rows);
    let del = gio::Menu::new();
    del.append(Some("Delete row"), Some("table.del-row"));
    del.append(Some("Delete column"), Some("table.del-col"));
    del.append(Some("Header row on or off"), Some("table.header"));
    del.append(Some("Delete table"), Some("table.delete"));
    menu.append_section(None, &del);
    menu
}

fn install_actions(t: &Rc<TableWidget>) {
    let group = gio::SimpleActionGroup::new();
    let add = |name: &str, f: fn(&Rc<TableWidget>, usize, usize)| {
        let a = gio::SimpleAction::new(name, None);
        let weak = Rc::downgrade(t);
        a.connect_activate(move |_, _| {
            if let Some(t) = weak.upgrade() {
                let (r, c) = t.current.get();
                f(&t, r, c);
            }
        });
        group.add_action(&a);
    };
    add("row-above", |t, r, _| {
        t.edit(|rows| {
            let n = rows[0].len();
            rows.insert(r, vec![String::new(); n]);
        })
    });
    add("row-below", |t, r, _| {
        t.edit(|rows| {
            let n = rows[0].len();
            rows.insert(r + 1, vec![String::new(); n]);
        })
    });
    add("col-left", |t, _, c| t.edit(|rows| rows.iter_mut().for_each(|row| row.insert(c, String::new()))));
    add("col-right", |t, _, c| t.edit(|rows| rows.iter_mut().for_each(|row| row.insert(c + 1, String::new()))));
    add("del-row", |t, r, _| {
        t.edit(|rows| {
            if r < rows.len() {
                rows.remove(r);
            }
        })
    });
    add("del-col", |t, _, c| {
        t.edit(|rows| {
            for row in rows.iter_mut() {
                if c < row.len() {
                    row.remove(c);
                }
            }
        })
    });
    add("header", |t, _, _| {
        t.header.set(!t.header.get());
        t.edit(|_| {});
    });
    add("delete", |t, _, _| {
        if let Some(d) = t.on_delete.borrow().clone() {
            d();
        }
    });
    t.grid.insert_action_group("table", Some(&group));
}
