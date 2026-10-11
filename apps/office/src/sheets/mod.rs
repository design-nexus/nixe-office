//! Nexus Sheets: the spreadsheet.

mod cells;
pub mod chart;
mod finance;
pub mod formula;
mod functions;
pub(super) mod grid;
mod lists;
pub mod model;
mod numfmt;
mod pages;
mod menu;
mod placed;
mod rules;
mod series;
mod sortfilter;
mod structure;
mod summary;
mod templates;
mod xlsx;

use crate::doc::{self, Find, Template};
use crate::print::Pages;
use crate::{color, container, widgets, window};
use anyhow::{Result, bail};
use gtk::prelude::*;
use gtk::{gdk, glib};
use model::*;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::{Rc, Weak};

const FORMAT: &str = "nexus-sheets";
const UNDO_LIMIT: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    None,
    /// Selecting cells from the anchor.
    Select,
    /// Picking cells for a formula being typed: the text at `at` (`len` chars) is the reference.
    Pick {
        at: i32,
        len: i32,
        from: (u32, u32),
    },
    ResizeCol {
        c: u32,
        x0: f64,
        w0: u32,
    },
    ResizeRow {
        r: u32,
        y0: f64,
        h0: u32,
    },
    /// Dragging the fill handle; the selection grows towards the pointer.
    Fill,
    /// Selecting whole columns or rows from the headers.
    Cols,
    Rows,
    /// Dragging the selection by its edge: the grabbed cell's place in it.
    MoveCells {
        dr: u32,
        dc: u32,
    },
    /// Moving or resizing a chart placed on the sheet: where it was.
    MoveChart {
        i: usize,
        x0: f64,
        y0: f64,
    },
    SizeChart {
        i: usize,
        w0: f64,
        h0: f64,
    },
}

struct Controls {
    bold: gtk::ToggleButton,
    italic: gtk::ToggleButton,
    numbers: Vec<(NumFmt, gtk::ToggleButton)>,
    formats: gtk::MenuButton,
    align: Vec<(Option<HAlign>, gtk::ToggleButton)>,
    header: gtk::ToggleButton,
    freeze: gtk::MenuButton,
    chart: gtk::ToggleButton,
    color: Rc<color::ColorButton>,
    fill: Rc<color::ColorButton>,
    filter: gtk::MenuButton,
    wrap: gtk::ToggleButton,
    checkbox: gtk::ToggleButton,
}

struct ChartPane {
    pane: gtk::Box,
    area: gtk::DrawingArea,
    range: gtk::Entry,
    kind: gtk::DropDown,
    /// The chart's title, and its axes' (across, up).
    titles: [gtk::Entry; 3],
}

pub struct Sheets {
    me: RefCell<Weak<Sheets>>,
    root: gtk::Box,
    wb: RefCell<Workbook>,
    calc: RefCell<Calc>,
    /// The rows on screen, in order (all of them, unless a filter hides some).
    view: RefCell<Vec<u32>>,
    /// Where each on-screen row starts, from the top of the first: one more entry than rows.
    row_y: RefCell<Vec<f64>>,
    col_x: RefCell<Vec<f64>>,
    /// The selection, in on-screen rows. `cursor` is the active cell, `anchor` the other corner.
    cursor: Cell<(u32, u32)>,
    anchor: Cell<(u32, u32)>,
    drag: Cell<Drag>,
    editing: Cell<bool>,
    area: gtk::DrawingArea,
    hadj: gtk::Adjustment,
    vadj: gtk::Adjustment,
    entry: gtk::Entry,
    /// Where the fill handle is being dragged to.
    fill_to: Cell<Option<(u32, u32)>>,
    name: gtk::Label,
    fx: gtk::Entry,
    sheet_list: gtk::Box,
    tab_scroll: gtk::ScrolledWindow,
    chart: ChartPane,
    controls: RefCell<Option<Controls>>,
    clip: RefCell<Option<Clip>>,
    /// The copy was a cut: pasting moves the cells.
    cut: Cell<bool>,
    undo: RefCell<Vec<Workbook>>,
    redo: RefCell<Vec<Workbook>>,
    syncing: Cell<bool>,
    /// Cells matching the find bar, drawn highlighted.
    found: RefCell<Vec<(u32, u32)>>,
    /// The sheet names the tabs were last built for.
    tab_names: RefCell<Vec<String>>,
    /// The placed chart picked on the sheet, if any.
    chart_sel: Cell<Option<usize>>,
    /// While dragging cells: where their top left would land, and whether Ctrl copies.
    move_to: Cell<Option<(u32, u32)>>,
    drag_copy: Cell<bool>,
    /// The grid's zoom box.
    zoomed: RefCell<Option<gtk::Box>>,
}

thread_local! {
    static SHEETS: RefCell<Option<Rc<Sheets>>> = const { RefCell::new(None) };
}

/// Now as a date number (TODAY takes the whole days, NOW the time as well).
fn today() -> f64 {
    let now = glib::DateTime::now_local().ok();
    now.map(|d| {
        formula::date_serial(d.year() as i64, d.month() as i64, d.day_of_month() as i64)
            + (d.hour() as f64 * 3600.0 + d.minute() as f64 * 60.0 + d.seconds()) / 86400.0
    })
    .unwrap_or(0.0)
}

impl Sheets {
    pub fn create() -> Rc<dyn doc::Editor> {
        Sheets::build()
    }

    fn build() -> Rc<Sheets> {
        let wb = Workbook::default();
        let calc = Calc::new(&wb, today());
        let area = gtk::DrawingArea::new();
        area.set_hexpand(true);
        area.set_vexpand(true);
        area.set_focusable(true);
        area.add_css_class("sheet-grid");
        let entry = gtk::Entry::new();
        entry.add_css_class("cell-editor");
        entry.set_has_frame(false);
        entry.set_visible(false);
        let hadj = gtk::Adjustment::new(0.0, 0.0, 1.0, 40.0, 400.0, 1.0);
        let vadj = gtk::Adjustment::new(0.0, 0.0, 1.0, 30.0, 300.0, 1.0);

        let range = gtk::Entry::new();
        range.set_placeholder_text(Some("A1:C12"));
        range.set_width_chars(10);
        let kind = gtk::DropDown::from_strings(&ChartKind::ALL.map(|k| k.name()));
        let chart_area = gtk::DrawingArea::new();
        chart_area.set_vexpand(true);
        chart_area.set_content_height(220);
        let pane = widgets::vbox(10);
        pane.add_css_class("chart-pane");
        pane.set_visible(false);
        // The grid takes the spare width, not the pane.
        pane.set_hexpand(false);
        pane.set_size_request(340, -1);

        let s = Rc::new(Sheets {
            me: RefCell::new(Weak::new()),
            root: widgets::vbox(0),
            wb: RefCell::new(wb),
            calc: RefCell::new(calc),
            view: RefCell::new((0..ROWS).collect()),
            row_y: RefCell::new(Vec::new()),
            col_x: RefCell::new(Vec::new()),
            cursor: Cell::new((0, 0)),
            anchor: Cell::new((0, 0)),
            drag: Cell::new(Drag::None),
            editing: Cell::new(false),
            area,
            hadj,
            vadj,
            entry,
            fill_to: Cell::new(None),
            name: widgets::label("A1", "cell-name"),
            fx: gtk::Entry::new(),
            sheet_list: widgets::hbox(2),
            tab_scroll: gtk::ScrolledWindow::new(),
            chart: ChartPane { pane, area: chart_area, range, kind, titles: std::array::from_fn(|_| gtk::Entry::new()) },
            controls: RefCell::new(None),
            clip: RefCell::new(None),
            cut: Cell::new(false),
            undo: RefCell::new(Vec::new()),
            redo: RefCell::new(Vec::new()),
            syncing: Cell::new(false),
            found: RefCell::new(Vec::new()),
            tab_names: RefCell::new(Vec::new()),
            chart_sel: Cell::new(None),
            move_to: Cell::new(None),
            drag_copy: Cell::new(false),
            zoomed: RefCell::new(None),
        });
        *s.me.borrow_mut() = Rc::downgrade(&s);
        s.root.add_css_class("sheets-editor");
        let toolbar = s.build_toolbar();
        s.root.append(&toolbar);
        s.root.append(&s.build_formula_bar());
        let body = widgets::hbox(0);
        body.set_vexpand(true);
        body.append(&s.build_grid());
        s.build_chart_pane();
        body.append(&s.chart.pane);
        s.root.append(&body);
        s.root.append(&s.build_tabs());
        s.relayout();
        s.refresh_sheets();
        SHEETS.with(|x| *x.borrow_mut() = Some(s.clone()));
        s
    }

    fn weak(&self) -> Weak<Sheets> {
        self.me.borrow().clone()
    }

    fn rc(&self) -> Rc<Sheets> {
        self.me.borrow().upgrade().expect("the editor is alive")
    }

    // ---------- Toolbar ----------

    fn build_toolbar(self: &Rc<Self>) -> gtk::Box {
        let bar = widgets::hbox(0);
        bar.add_css_class("toolbar");
        bar.add_css_class("doc-toolbar");
        let flow = widgets::wrap_row(4, 4);
        flow.add_css_class("tool-row");
        flow.set_hexpand(true);
        flow.set_valign(gtk::Align::Center);
        bar.append(&flow);
        let group = || {
            let g = widgets::hbox(2);
            g.add_css_class("tool-group");
            g
        };
        let button = |icon: &str, tip: &str, f: fn(&Sheets)| {
            let b = widgets::bar_button(icon, tip);
            let me = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(s) = me.upgrade() {
                    f(&s);
                }
            });
            b
        };
        let toggle = |icon: &str, tip: &str, f: fn(&Sheets, bool)| {
            let b = gtk::ToggleButton::new();
            b.set_icon_name(icon);
            b.add_css_class("tool-toggle");
            b.set_tooltip_text(Some(tip));
            let me = Rc::downgrade(self);
            b.connect_toggled(move |b| {
                if let Some(s) = me.upgrade()
                    && !s.syncing.get()
                {
                    f(&s, b.is_active());
                }
            });
            b
        };

        let g = group();
        g.append(&button("nxo-undo-2-symbolic", "Undo (Ctrl+Z)", doc::Editor::undo));
        g.append(&button("nxo-redo-2-symbolic", "Redo (Ctrl+Shift+Z)", doc::Editor::redo));
        flow.append(&g);

        let g = group();
        let mut numbers = Vec::new();
        for (fmt, icon, tip) in [
            (NumFmt::General, "nxo-type-symbolic", "General: numbers as typed"),
            (NumFmt::Decimal, "nxo-decimal-symbolic", "Two decimal places"),
            (NumFmt::Currency, "nxo-currency-symbolic", "Currency"),
            (NumFmt::Percent, "nxo-percent-symbolic", "Percent"),
            (NumFmt::Date, "nxo-calendar-symbolic", "Date"),
        ] {
            let b = gtk::ToggleButton::new();
            b.set_icon_name(icon);
            b.add_css_class("tool-toggle");
            b.set_tooltip_text(Some(tip));
            let me = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(s) = me.upgrade()
                    && !s.syncing.get()
                {
                    s.format_selection(move |f| {
                        f.number = fmt;
                        f.pattern = None;
                    });
                }
            });
            g.append(&b);
            numbers.push((fmt, b));
        }
        let formats = self.formats_button();
        g.append(&formats);
        flow.append(&g);

        let g = group();
        let bold = toggle("nxo-bold-symbolic", "Bold (Ctrl+B)", |s, on| s.format_selection(move |f| f.bold = on));
        let italic = toggle("nxo-italic-symbolic", "Italic (Ctrl+I)", |s, on| s.format_selection(move |f| f.italic = on));
        g.append(&bold);
        g.append(&italic);
        let me = Rc::downgrade(self);
        let color = color::button("nxo-baseline-symbolic", "Text color", color::TEXT, "Automatic", move |c| {
            if let Some(s) = me.upgrade() {
                s.format_selection(move |f| f.color = c.clone());
            }
        });
        let me = Rc::downgrade(self);
        let fill = color::button("nxo-paint-bucket-symbolic", "Fill color", color::HIGHLIGHT, "None", move |c| {
            if let Some(s) = me.upgrade() {
                s.format_selection(move |f| f.fill = c.clone());
            }
        });
        g.append(&color.button);
        g.append(&fill.button);
        g.append(&self.borders_button());
        let wrap = toggle("nxo-wrap-text-symbolic", "Wrap text in the cell", |s, on| s.set_wrap(on));
        g.append(&wrap);
        let checkbox = toggle("nxo-square-check-symbolic", "Checkboxes: click one or press Space to tick it", |s, on| {
            let a = s.selection_src();
            s.change(|wb| {
                wb.format(a, |f| f.checkbox = on);
                // A new checkbox starts unticked.
                if on {
                    for (r, c) in a.iter() {
                        if wb.sheet().raw(r, c).is_empty() {
                            wb.set(r, c, "FALSE");
                        }
                    }
                }
            });
            s.area.grab_focus();
        });
        g.append(&checkbox);
        flow.append(&g);

        let g = group();
        let mut align = Vec::new();
        for (a, icon, tip) in [
            (Some(HAlign::Left), "nxo-align-left-symbolic", "Align left"),
            (Some(HAlign::Center), "nxo-align-center-symbolic", "Center"),
            (Some(HAlign::Right), "nxo-align-right-symbolic", "Align right"),
        ] {
            let b = gtk::ToggleButton::new();
            b.set_icon_name(icon);
            b.add_css_class("tool-toggle");
            b.set_tooltip_text(Some(tip));
            let me = Rc::downgrade(self);
            b.connect_clicked(move |b| {
                if let Some(s) = me.upgrade()
                    && !s.syncing.get()
                {
                    // Clicking the active alignment goes back to automatic.
                    let a = if b.is_active() { a } else { None };
                    s.format_selection(move |f| f.align = a);
                }
            });
            g.append(&b);
            align.push((a, b));
        }
        flow.append(&g);

        let g = group();
        g.append(&button("nxo-table-cells-merge-symbolic", "Merge the selected cells, or unmerge them", |s| s.merge_cells()));
        g.append(&button("nxo-message-square-text-symbolic", "Comment on this cell (Ctrl+Alt+M)", |s| s.edit_comment()));
        g.append(&button("nxo-highlighter-symbolic", "Highlight cells that meet a rule", |s| s.highlight_dialog()));
        g.append(&button("nxo-sigma-symbolic", "Summarize: totals by group, on a new sheet", |s| s.summarize_dialog()));
        g.append(&button("nxo-table-2-symbolic", "Format as a table: heading row, banded rows, a colour", |s| s.rc().table_dialog()));
        g.append(&button("nxo-chart-area-symbolic", "Put a chart of the selection on the sheet (drag it anywhere)", |s| s.place_chart()));
        flow.append(&g);

        let g = group();
        g.append(&button("nxo-arrow-up-narrow-wide-symbolic", "Sort A to Z by this column", |s| s.sort(true)));
        g.append(&button("nxo-arrow-down-symbolic", "Sort Z to A by this column", |s| s.sort(false)));
        g.append(&button("nxo-sliders-horizontal-symbolic", "Sort by several columns…", |s| s.rc().sort_dialog()));
        let filter = self.filter_button();
        g.append(&filter);
        let header =
            toggle("nxo-table-rows-split-symbolic", "First row is a header (sorting and filtering leave it on top)", |s, on| {
                s.change(|wb| wb.sheet_mut().header = on);
            });
        let freeze = self.freeze_button();
        g.append(&header);
        g.append(&freeze);
        flow.append(&g);

        let g = group();
        let chart = toggle("nxo-chart-column-symbolic", "Chart", |s, on| s.show_chart(on));
        g.append(&chart);
        flow.append(&g);

        bar.append(&doc::file_menu(Vec::new()));
        *self.controls.borrow_mut() = Some(Controls { bold, italic, numbers, formats, align, header, freeze, chart, color, fill, filter, wrap, checkbox });
        bar
    }

    /// Number formats beyond the toolbar's: thousands, currencies, date styles, times,
    /// and a format code of one's own.
    fn formats_button(self: &Rc<Self>) -> gtk::MenuButton {
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("nxo-chevron-down-symbolic");
        menu.add_css_class("bar-button");
        menu.set_tooltip_text(Some("More number formats"));
        menu.set_valign(gtk::Align::Center);
        let pop = gtk::Popover::new();
        pop.add_css_class("menu-popover");
        let content = widgets::vbox(0);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(420)
            .build();
        let list = widgets::vbox(0);
        // Each shown with an example, so the choice is easy to see.
        let sample = |code: &str| {
            let n = if code.contains(['d', 'y', 'h']) { formula::date_serial(2026, 10, 5) + 14.5 / 24.0 } else { 1234.5 };
            numfmt::render(code, n).unwrap_or_default()
        };
        let apply = |s: &Sheets, code: Option<String>| {
            s.format_selection(move |f| {
                f.number = NumFmt::General;
                f.pattern = code.clone();
            });
        };
        for (name, code) in numfmt::PRESETS {
            let b = gtk::Button::new();
            b.add_css_class("menu-item");
            let row = widgets::hbox(16);
            let l = widgets::label(name, "");
            l.set_xalign(0.0);
            l.set_hexpand(true);
            row.append(&l);
            row.append(&widgets::label(&sample(code), "dim"));
            b.set_child(Some(&row));
            let (me, p, code) = (self.weak(), pop.clone(), code.to_string());
            b.connect_clicked(move |_| {
                p.popdown();
                if let Some(s) = me.upgrade() {
                    apply(&s, Some(code.clone()));
                }
            });
            list.append(&b);
        }
        scroll.set_child(Some(&list));
        content.append(&scroll);

        // A code of one's own, with a preview as it's typed.
        let custom = widgets::vbox(6);
        custom.add_css_class("prompt-box");
        custom.append(&widgets::label("Format code", "dim"));
        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some("#,##0.00 \"kg\""));
        custom.append(&entry);
        let preview = widgets::label("", "dim");
        preview.set_xalign(0.0);
        custom.append(&preview);
        let p = preview.clone();
        entry.connect_changed(move |e| {
            let code = e.text();
            p.set_text(&match numfmt::render(&code, 1234.5) {
                _ if code.is_empty() => String::new(),
                Some(t) => format!("1234.5 shows as {t}"),
                None => "That code isn't understood here.".into(),
            });
        });
        let (me, p) = (self.weak(), pop.clone());
        entry.connect_activate(move |e| {
            let code = e.text().trim().to_string();
            if numfmt::render(&code, 1.0).is_none() {
                return;
            }
            p.popdown();
            if let Some(s) = me.upgrade() {
                apply(&s, (!code.is_empty()).then_some(code));
            }
        });
        content.append(&custom);
        // Opening the menu shows the selection's own code.
        let (me, e) = (self.weak(), entry.clone());
        pop.connect_show(move |_| {
            let Some(s) = me.upgrade() else { return };
            let (r, c) = s.cursor.get();
            let code = s.wb.borrow().sheet().format(s.src_row(r), c).pattern.unwrap_or_default();
            e.set_text(&code);
        });
        pop.set_child(Some(&content));
        menu.set_popover(Some(&pop));
        menu
    }

    /// Keep rows and columns in view while the rest scrolls.
    fn freeze_button(self: &Rc<Self>) -> gtk::MenuButton {
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("nxo-snowflake-symbolic");
        menu.add_css_class("bar-button");
        menu.set_tooltip_text(Some("Keep rows or columns in view while scrolling"));
        menu.set_valign(gtk::Align::Center);
        let pop = gtk::Popover::new();
        pop.add_css_class("menu-popover");
        let list = widgets::vbox(0);
        // Each item: its label, and how many rows and columns it keeps in view.
        type Freeze = (&'static str, fn(&Sheets) -> (u32, u32));
        let items: [Freeze; 4] = [
            ("Keep the top row in view", |_| (1, 0)),
            ("Keep the first column in view", |_| (0, 1)),
            ("Keep the rows above and columns left of the cursor", |s| {
                let (r, c) = s.cursor.get();
                (r.min(MAX_FROZEN), c.min(MAX_FROZEN))
            }),
            ("Let everything scroll", |_| (0, 0)),
        ];
        for (label, f) in items {
            let b = gtk::Button::with_label(label);
            b.add_css_class("menu-item");
            if let Some(l) = b.child().and_downcast::<gtk::Label>() {
                l.set_xalign(0.0);
            }
            let (me, p) = (self.weak(), pop.clone());
            b.connect_clicked(move |_| {
                p.popdown();
                let Some(s) = me.upgrade() else { return };
                let (rows, cols) = f(&s);
                s.vadj.set_value(0.0);
                s.hadj.set_value(0.0);
                s.change(|wb| {
                    let sh = wb.sheet_mut();
                    sh.freeze_rows = rows;
                    sh.freeze_cols = cols;
                });
                s.area.grab_focus();
            });
            list.append(&b);
        }
        pop.set_child(Some(&list));
        menu.set_popover(Some(&pop));
        menu
    }

    fn build_formula_bar(self: &Rc<Self>) -> gtk::Box {
        let bar = widgets::hbox(10);
        bar.add_css_class("formula-bar");
        self.name.set_width_chars(7);
        self.name.add_css_class("mono");
        bar.append(&self.name);
        let fx = widgets::label("fx", "dim");
        fx.add_css_class("fx");
        bar.append(&fx);
        self.fx.set_hexpand(true);
        self.fx.add_css_class("fx-entry");
        self.fx.set_placeholder_text(Some("A value, or a formula such as =SUM(B2:B9)"));
        bar.append(&self.fx);
        let me = Rc::downgrade(self);
        self.fx.connect_activate(move |e| {
            if let Some(s) = me.upgrade() {
                let (r, c) = s.cursor.get();
                let text = e.text().to_string();
                s.commit_raw(r, c, &text);
                s.end_edit(false);
                s.area.grab_focus();
            }
        });
        // Typing in the formula bar shows in the cell as you go.
        let me = Rc::downgrade(self);
        self.fx.connect_changed(move |e| {
            if let Some(s) = me.upgrade()
                && !s.syncing.get()
                && e.has_focus()
            {
                s.syncing.set(true);
                if !s.editing.get() {
                    s.begin_edit(None, false);
                }
                s.entry.set_text(&e.text());
                s.syncing.set(false);
            }
        });
        let keys = gtk::EventControllerKey::new();
        let me = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape
                && let Some(s) = me.upgrade()
            {
                s.end_edit(false);
                s.sync();
                s.area.grab_focus();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.fx.add_controller(keys);
        bar
    }

    fn build_chart_pane(self: &Rc<Self>) {
        let c = &self.chart;
        let head = widgets::hbox(8);
        head.append(&widgets::label("CHART", "group-title"));
        let spacer = widgets::hbox(0);
        spacer.set_hexpand(true);
        head.append(&spacer);
        let close = widgets::bar_button("window-close-symbolic", "Hide the chart");
        let me = Rc::downgrade(self);
        close.connect_clicked(move |_| {
            if let Some(s) = me.upgrade()
                && let Some(ctl) = s.controls.borrow().as_ref()
            {
                ctl.chart.set_active(false);
            }
        });
        head.append(&close);
        c.pane.append(&head);
        let card = widgets::vbox(0);
        card.add_css_class("chart-card");
        card.append(&c.area);
        c.pane.append(&card);
        let row = widgets::hbox(8);
        row.append(&widgets::label("Cells", "dim"));
        c.range.set_hexpand(true);
        c.range.set_width_chars(8);
        row.append(&c.range);
        let pick = gtk::Button::with_label("Use selection");
        row.append(&pick);
        c.pane.append(&row);
        let row = widgets::hbox(8);
        row.append(&widgets::label("Kind", "dim"));
        c.kind.set_hexpand(true);
        row.append(&c.kind);
        let remove = gtk::Button::with_label("Remove");
        row.append(&remove);
        c.pane.append(&row);
        // Titles: set when Enter is pressed or the field is left.
        for (i, (label, hint)) in [("Title", "None"), ("Across", "Bottom axis"), ("Up", "Side axis")].into_iter().enumerate() {
            let row = widgets::hbox(8);
            let l = widgets::label(label, "dim");
            l.set_width_chars(6);
            l.set_xalign(0.0);
            row.append(&l);
            let e = &c.titles[i];
            e.set_hexpand(true);
            e.set_placeholder_text(Some(hint));
            row.append(e);
            c.pane.append(&row);
            let me = Rc::downgrade(self);
            let commit = move |text: String| {
                let Some(s) = me.upgrade() else { return };
                if s.syncing.get() {
                    return;
                }
                let chart = s.wb.borrow().sheet().chart.clone();
                if let Some(mut ch) = chart {
                    let field = match i {
                        0 => &mut ch.text.title,
                        1 => &mut ch.text.x_title,
                        _ => &mut ch.text.y_title,
                    };
                    if *field != text {
                        *field = text;
                        s.change(|wb| wb.sheet_mut().chart = Some(ch.clone()));
                    }
                }
            };
            let cm = commit.clone();
            e.connect_activate(move |e| cm(e.text().trim().to_string()));
            let focus = gtk::EventControllerFocus::new();
            let e2 = e.clone();
            focus.connect_leave(move |_| commit(e2.text().trim().to_string()));
            e.add_controller(focus);
        }
        let note = widgets::label(
            "Labels in the first column, one series per other column. A first row of words names the series.",
            "dim",
        );
        note.set_wrap(true);
        // A wrapping label asks for its one-line width unless told otherwise.
        note.set_max_width_chars(36);
        note.set_width_chars(36);
        note.add_css_class("chart-note");
        c.pane.append(&note);

        let me = Rc::downgrade(self);
        c.area.set_draw_func(move |a, cr, w, h| {
            if let Some(s) = me.upgrade() {
                s.draw_chart(a, cr, w as f64, h as f64);
            }
        });
        let me = Rc::downgrade(self);
        let set_range = move |text: String| {
            if let Some(s) = me.upgrade() {
                let text = text.trim().to_uppercase();
                if formula::parse_range(&text).is_none() {
                    window::toast("Give the chart's cells as a range, such as A1:C12.");
                    return;
                }
                let kind = s.wb.borrow().sheet().chart.as_ref().map(|c| c.kind).unwrap_or_default();
                let titles = s.wb.borrow().sheet().chart.as_ref().map(|c| c.text.clone()).unwrap_or_default();
                s.change(|wb| wb.sheet_mut().chart = Some(Chart { range: text.clone(), kind, text: titles.clone() }));
            }
        };
        let sr = set_range.clone();
        c.range.connect_activate(move |e| sr(e.text().to_string()));
        let me = Rc::downgrade(self);
        pick.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                let a = s.selection_src();
                set_range(a.name());
            }
        });
        let me = Rc::downgrade(self);
        c.kind.connect_selected_notify(move |d| {
            if let Some(s) = me.upgrade()
                && !s.syncing.get()
            {
                let kind = ChartKind::ALL[d.selected() as usize % ChartKind::ALL.len()];
                let chart = s.wb.borrow().sheet().chart.clone();
                if let Some(c) = chart {
                    s.change(|wb| wb.sheet_mut().chart = Some(Chart { kind, ..c.clone() }));
                }
            }
        });
        let me = Rc::downgrade(self);
        remove.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.change(|wb| wb.sheet_mut().chart = None);
            }
        });
    }

    fn show_chart(&self, on: bool) {
        self.chart.pane.set_visible(on);
        if on && self.wb.borrow().sheet().chart.is_none() {
            // Start from the selection, or the data around the cursor.
            let a = self.selection_src();
            let range = if a.is_cell() {
                self.wb
                    .borrow()
                    .sheet()
                    .used()
                    .map(|(r, c)| Area::new(0, 0, r.min(CHART_ROWS), c.min(CHART_SERIES as u32)).name())
            } else {
                Some(a.name())
            };
            if let Some(range) = range {
                self.change(|wb| wb.sheet_mut().chart = Some(Chart { range: range.clone(), kind: ChartKind::Bar, text: ChartText::default() }));
            }
        }
        self.sync();
    }

    fn draw_chart(&self, a: &gtk::DrawingArea, cr: &cairo::Context, w: f64, h: f64) {
        let wb = self.wb.borrow();
        let Some((kind, titles)) = wb.sheet().chart.as_ref().map(|c| (c.kind, c.text.clone())) else {
            return;
        };
        let calc = self.calc.borrow();
        let ctx = a.pango_context();
        match wb.chart_data(wb.active, &calc) {
            Some((names, labels, series)) => {
                let d = chart::Data { names: &names, labels: &labels, series: &series };
                chart::draw_titled(cr, &ctx, kind, &d, &titles, (0.0, 0.0, w, h))
            }
            None => {
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.paint().ok();
                let layout = a.create_pango_layout(Some("Put numbers in the chart's cells to see it here."));
                layout.set_width(((w - 32.0).max(1.0) * gtk::pango::SCALE as f64) as i32);
                layout.set_alignment(gtk::pango::Alignment::Center);
                layout.set_wrap(gtk::pango::WrapMode::Word);
                let (_, th) = layout.pixel_size();
                cr.set_source_rgb(0.42, 0.45, 0.5);
                cr.move_to(16.0, (h - th as f64) / 2.0);
                pangocairo::functions::show_layout(cr, &layout);
            }
        }
    }

    // ---------- Sheet tabs ----------

    /// The strip under the grid: an add button, then a tab per sheet.
    fn build_tabs(self: &Rc<Self>) -> gtk::Box {
        let bar = widgets::hbox(4);
        bar.add_css_class("sheet-tabs");
        let add = widgets::bar_button("nxo-plus-symbolic", "Add a sheet");
        let me = self.weak();
        add.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.add_sheet();
            }
        });
        bar.append(&add);
        self.tab_scroll.set_policy(gtk::PolicyType::External, gtk::PolicyType::Never);
        self.tab_scroll.set_hexpand(true);
        self.tab_scroll.set_child(Some(&self.sheet_list));
        // A mouse wheel scrolls the tabs sideways.
        let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let adj = self.tab_scroll.hadjustment();
        wheel.connect_scroll(move |_, _, dy| {
            adj.set_value(adj.value() + dy * 40.0);
            glib::Propagation::Stop
        });
        self.tab_scroll.add_controller(wheel);
        bar.append(&self.tab_scroll);
        bar
    }

    fn add_sheet(&self) {
        let mut ok = true;
        self.change(|wb| ok = wb.add_sheet());
        if !ok {
            window::toast(&format!("A workbook holds up to {MAX_SHEETS} sheets."));
        }
        self.area.grab_focus();
    }

    /// Rebuild the tabs when sheets were added, removed, renamed or moved;
    /// otherwise just mark the active one.
    fn refresh_sheets(&self) {
        let wb = self.wb.borrow();
        let names: Vec<String> = wb.sheets.iter().map(|s| s.name.clone()).collect();
        if *self.tab_names.borrow() != names {
            while let Some(c) = self.sheet_list.first_child() {
                self.sheet_list.remove(&c);
            }
            for (i, name) in names.iter().enumerate() {
                self.sheet_list.append(&self.tab(i, name));
            }
            *self.tab_names.borrow_mut() = names;
        }
        let mut child = self.sheet_list.first_child();
        let mut i = 0;
        while let Some(c) = child {
            if i == wb.active {
                c.add_css_class("active");
                self.scroll_to_tab(&c);
            } else {
                c.remove_css_class("active");
            }
            child = c.next_sibling();
            i += 1;
        }
    }

    fn tab(&self, i: usize, name: &str) -> gtk::Button {
        let b = gtk::Button::new();
        b.add_css_class("sheet-tab");
        let l = widgets::label(name, "");
        l.set_ellipsize(gtk::pango::EllipsizeMode::End);
        l.set_max_width_chars(24);
        b.set_child(Some(&l));
        b.set_tooltip_text(Some(&format!("{name} (double-click to rename, drag to move, right-click for more)")));
        let me = self.weak();
        b.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.select_sheet(i);
            }
        });
        // Watch clicks before the button takes them, to catch a double-click.
        let double = gtk::GestureClick::new();
        double.set_propagation_phase(gtk::PropagationPhase::Capture);
        let me = self.weak();
        double.connect_pressed(move |_, n, _, _| {
            if n == 2
                && let Some(s) = me.upgrade()
            {
                s.rename_sheet(i);
            }
        });
        b.add_controller(double);
        let menu = gtk::GestureClick::new();
        menu.set_button(3);
        let me = self.weak();
        let bb = b.clone();
        menu.connect_pressed(move |_, _, _, _| {
            if let Some(s) = me.upgrade() {
                s.sheet_menu(i, &bb);
            }
        });
        b.add_controller(menu);
        let drag = gtk::DragSource::new();
        drag.set_actions(gtk::gdk::DragAction::MOVE);
        drag.set_content(Some(&gtk::gdk::ContentProvider::for_value(&(i as u32).to_value())));
        let bb = b.clone();
        drag.connect_drag_begin(move |d, _| {
            d.set_icon(Some(&gtk::WidgetPaintable::new(Some(&bb))), 0, 0);
        });
        b.add_controller(drag);
        let drop = gtk::DropTarget::new(u32::static_type(), gtk::gdk::DragAction::MOVE);
        let me = self.weak();
        drop.connect_drop(move |_, v, _, _| {
            let (Ok(from), Some(s)) = (v.get::<u32>(), me.upgrade()) else { return false };
            s.change(|wb| {
                wb.move_sheet_to(from as usize, i);
            });
            true
        });
        b.add_controller(drop);
        b
    }

    /// Scroll the tab strip so `tab` is in view, once it has been laid out.
    fn scroll_to_tab(&self, tab: &gtk::Widget) {
        let (tab, list, adj) = (tab.clone(), self.sheet_list.clone(), self.tab_scroll.hadjustment());
        glib::idle_add_local_once(move || {
            let Some(r) = tab.compute_bounds(&list) else { return };
            let (x, w) = (r.x() as f64, r.width() as f64);
            if x < adj.value() {
                adj.set_value(x);
            } else if x + w > adj.value() + adj.page_size() {
                adj.set_value(x + w - adj.page_size());
            }
        });
    }

    fn select_sheet(&self, i: usize) {
        self.end_edit(true);
        if i == self.wb.borrow().active {
            window::navigate("document");
            return;
        }
        self.wb.borrow_mut().active = i;
        self.cursor.set((0, 0));
        self.anchor.set((0, 0));
        self.hadj.set_value(0.0);
        self.vadj.set_value(0.0);
        self.after_change(false);
        window::navigate("document");
        self.area.grab_focus();
    }

    fn sheet_menu(&self, i: usize, parent: &gtk::Button) {
        let pop = gtk::Popover::new();
        pop.add_css_class("menu-popover");
        pop.set_parent(parent);
        let list = widgets::vbox(0);
        let item = |label: &str, f: Box<dyn Fn(&Sheets)>| {
            let b = gtk::Button::with_label(label);
            b.add_css_class("menu-item");
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
        item("Rename…", Box::new(move |s| s.rename_sheet(i)));
        item(
            "Duplicate",
            Box::new(move |s| {
                s.change(|wb| {
                    wb.duplicate_sheet(i);
                })
            }),
        );
        item(
            "Move left",
            Box::new(move |s| {
                s.change(|wb| {
                    wb.move_sheet(i, -1);
                })
            }),
        );
        item(
            "Move right",
            Box::new(move |s| {
                s.change(|wb| {
                    wb.move_sheet(i, 1);
                })
            }),
        );
        if self.wb.borrow().sheets.len() > 1 {
            let del = gtk::Button::with_label("Delete sheet");
            del.add_css_class("menu-item");
            del.add_css_class("danger-text");
            if let Some(l) = del.child().and_downcast::<gtk::Label>() {
                l.set_xalign(0.0);
            }
            // Two clicks: the first arms it.
            let armed = Rc::new(Cell::new(false));
            let me = self.weak();
            let p = pop.clone();
            del.connect_clicked(move |b| {
                if !armed.replace(true) {
                    b.set_label("Click again to delete");
                    return;
                }
                p.popdown();
                if let Some(s) = me.upgrade() {
                    s.change(|wb| {
                        wb.remove_sheet(i);
                    });
                }
            });
            list.append(&del);
        }
        pop.set_child(Some(&list));
        pop.connect_closed(|p| p.unparent());
        pop.popup();
    }

    fn rename_sheet(&self, i: usize) {
        let current = self.wb.borrow().sheets.get(i).map(|s| s.name.clone()).unwrap_or_default();
        let (dialog, card) = widgets::dialog("Rename sheet", 380);
        let entry = gtk::Entry::new();
        entry.set_text(&current);
        entry.set_max_length(31);
        card.append(&entry);
        let err = widgets::label("", "danger-text");
        err.set_visible(false);
        card.append(&err);
        let buttons = widgets::hbox(8);
        buttons.set_halign(gtk::Align::End);
        let cancel = gtk::Button::with_label("Cancel");
        let ok = gtk::Button::with_label("Rename");
        ok.add_css_class("suggested-action");
        buttons.append(&cancel);
        buttons.append(&ok);
        card.append(&buttons);
        let d = dialog.clone();
        cancel.connect_clicked(move |_| d.close());
        let me = self.weak();
        let (d, e) = (dialog.clone(), entry.clone());
        let go = move || {
            let Some(s) = me.upgrade() else { return };
            let name = e.text().to_string();
            let mut test = s.wb.borrow().clone();
            match test.rename_sheet(i, &name) {
                Ok(()) => {
                    d.close();
                    s.change(|wb| {
                        let _ = wb.rename_sheet(i, &name);
                    });
                }
                Err(msg) => {
                    err.set_text(msg);
                    err.set_visible(true);
                }
            }
        };
        let g = go.clone();
        ok.connect_clicked(move |_| g());
        entry.connect_activate(move |_| go());
        dialog.present();
        entry.grab_focus();
        entry.select_region(0, -1);
    }

    // ---------- Changes ----------

    /// Make a change as one undo step, then recalculate and redraw.
    fn change(&self, f: impl FnOnce(&mut Workbook)) {
        let before = self.wb.borrow().clone();
        f(&mut self.wb.borrow_mut());
        if *self.wb.borrow() == before {
            self.sync();
            return;
        }
        let mut undo = self.undo.borrow_mut();
        undo.push(before);
        if undo.len() > UNDO_LIMIT {
            undo.remove(0);
        }
        drop(undo);
        self.redo.borrow_mut().clear();
        self.after_change(true);
    }

    /// The workbook changed: recalculate, re-lay out, redraw and tell the shell.
    fn after_change(&self, edited: bool) {
        *self.calc.borrow_mut() = Calc::new(&self.wb.borrow(), today());
        self.relayout();
        self.refresh_sheets();
        self.area.queue_draw();
        self.chart.area.queue_draw();
        self.sync();
        if edited {
            doc::edited();
        } else {
            doc::status_changed();
        }
    }

    fn commit_raw(&self, r: u32, c: u32, raw: &str) {
        let src = self.src_row(r);
        // A cell with a list takes only its choices.
        let Some(raw) = self.checked_entry(src, c, raw) else { return };
        let raw = raw.as_str();
        if self.wb.borrow().sheet().raw(src, c) == raw {
            return;
        }
        self.change(|wb| wb.set(src, c, raw));
    }

    /// The source row for on-screen row `r`.
    fn src_row(&self, r: u32) -> u32 {
        self.view.borrow().get(r as usize).copied().unwrap_or(r)
    }

    /// The selection on screen, as on-screen rows.
    fn selection(&self) -> Area {
        let (a, b) = (self.anchor.get(), self.cursor.get());
        let area = Area::new(a.0, a.1, b.0, b.1);
        if self.merges_shown() { self.wb.borrow().sheet().grow(area) } else { area }
    }

    /// Merged cells show as one only while every row is in view (no filter),
    /// since on-screen rows are then the sheet's rows.
    fn merges_shown(&self) -> bool {
        self.view.borrow().len() == ROWS as usize
    }

    /// The merged block the cursor is in, when merges are shown.
    fn cursor_merge(&self) -> Option<Area> {
        let (r, c) = self.cursor.get();
        if !self.merges_shown() {
            return None;
        }
        self.wb.borrow().sheet().merge_at(r, c)
    }

    /// The selection in source rows (top-left to bottom-right of what's shown).
    fn selection_src(&self) -> Area {
        let a = self.selection();
        Area::new(self.src_row(a.top), a.left, self.src_row(a.bottom), a.right)
    }

    /// Each source row in the selection, as its own one-row area (for filters).
    fn selection_rows(&self) -> Vec<Area> {
        let a = self.selection();
        (a.top..=a.bottom).map(|r| Area::new(self.src_row(r), a.left, self.src_row(r), a.right)).collect()
    }

    fn format_selection(&self, f: impl Fn(&mut Format)) {
        let rows = self.selection_rows();
        self.change(|wb| {
            for a in rows {
                wb.format(a, &f);
            }
        });
        self.area.grab_focus();
    }

    fn sort(&self, ascending: bool) {
        let col = self.cursor.get().1;
        let calc = Calc::new(&self.wb.borrow(), today());
        let mut sorted = true;
        self.change(|wb| {
            let si = wb.active;
            sorted = wb.sort_rows(si, col, ascending, &calc);
        });
        if !sorted {
            window::toast("Unmerge the merged cells in the data to sort it.");
            return;
        }
        window::toast(&format!("Sorted by column {}, {}.", formula::col_name(col), if ascending { "A to Z" } else { "Z to A" }));
    }

    // ---------- Clipboard ----------

    fn copy(&self, cut: bool) {
        let a = self.selection_src();
        let clip = self.wb.borrow().copy_with_values(a, &self.calc.borrow());
        if let Some(d) = gdk::Display::default() {
            d.clipboard().set_text(&clip.text);
        }
        *self.clip.borrow_mut() = Some(clip);
        self.cut.set(cut);
        self.area.queue_draw();
    }

    fn paste(&self) {
        let Some(display) = gdk::Display::default() else { return };
        let me = self.weak();
        display.clipboard().read_text_async(gio_none(), move |res| {
            let Some(s) = me.upgrade() else { return };
            let text = res.ok().flatten().map(|t| t.to_string()).unwrap_or_default();
            let clip = s.clip.borrow().clone();
            let target = s.selection_src();
            match clip {
                // Our own copy: formats come too, and formulas move.
                Some(clip) if clip.text == text || text.is_empty() => {
                    if s.cut.get() {
                        let src = Area::new(
                            clip.from.0,
                            clip.from.1,
                            clip.from.0 + clip.cells.len() as u32 - 1,
                            clip.from.1 + clip.cells[0].len() as u32 - 1,
                        );
                        let mut dest = None;
                        s.change(|wb| dest = wb.move_area(src, (target.top, target.left)));
                        s.cut.set(false);
                        *s.clip.borrow_mut() = None;
                        if let Some(d) = dest {
                            s.select_src(d);
                        }
                    } else {
                        s.change(|wb| wb.paste(target, &clip));
                    }
                }
                _ if !text.is_empty() => {
                    let mut pasted = target;
                    s.change(|wb| pasted = wb.paste_text((target.top, target.left), &text));
                    s.select_src(pasted);
                }
                _ => {}
            }
        });
    }

    /// Paste only the values, or only the formatting, of what was copied here.
    /// (Text from another app has no formatting: values paste it as it is.)
    fn paste_special(&self, what: Paste) {
        let target = self.selection_src();
        let Some(clip) = self.clip.borrow().clone() else {
            if what == Paste::Values {
                self.paste();
            } else {
                window::toast("Copy some cells first, then paste their formatting.");
            }
            return;
        };
        self.change(|wb| wb.paste_as(target, &clip, what));
        // A cut stays put when only part of it is pasted.
        self.cut.set(false);
        self.area.grab_focus();
    }

    /// Select source area `a` (when no filter is hiding rows, rows match the screen).
    fn select_src(&self, a: Area) {
        let view = self.view.borrow();
        let find = |r: u32| view.iter().position(|v| *v == r).map(|i| i as u32).unwrap_or(r);
        let (top, bottom) = (find(a.top), find(a.bottom));
        drop(view);
        self.anchor.set((top, a.left));
        self.cursor.set((bottom, a.right));
        self.sync();
        self.area.queue_draw();
    }

    fn fill_down_or_right(&self, down: bool) {
        let a = self.selection_src();
        let (src, to) = if down {
            if a.rows() < 2 {
                return;
            }
            (Area::new(a.top, a.left, a.top, a.right), (a.bottom, a.left))
        } else {
            if a.cols() < 2 {
                return;
            }
            (Area::new(a.top, a.left, a.bottom, a.left), (a.top, a.right))
        };
        self.change(|wb| {
            wb.fill(src, to);
        });
    }

    // ---------- Status ----------

    fn selection_stats(&self) -> String {
        let a = self.selection();
        let wb = self.wb.borrow();
        let calc = self.calc.borrow();
        let s = wb.sheet();
        if self.editing.get() {
            return format!("Editing {} · Enter to keep, Esc to cancel", formula::cell_name(self.src_row(a.top), a.left));
        }
        let name = if a.is_cell() { formula::cell_name(self.src_row(a.top), a.left) } else { self.selection_src().name() };
        let (mut sum, mut count) = (0.0, 0usize);
        // Only cells that hold something are looked at, so whole columns are quick.
        for r in a.top..=a.bottom {
            let src = self.src_row(r);
            for ((_, c), cell) in s.cells.range((src, a.left)..=(src, a.right)) {
                if cell.raw.is_empty() {
                    continue;
                }
                if let formula::Value::Num(n) = calc.value(&wb, wb.active, src, *c) {
                    sum += n;
                    count += 1;
                }
            }
        }
        let paper = crate::print::Paper::preferred().name();
        if count >= 2 || (count == 1 && !a.is_cell()) {
            let avg = sum / count as f64;
            format!(
                "{name} · Sum {} · Average {} · Count {} · {paper}",
                formula::fixed(sum, if sum.fract() == 0.0 { 0 } else { 2 }, true),
                formula::fixed(avg, if avg.fract() == 0.0 { 0 } else { 2 }, true),
                count
            )
        } else {
            format!("{name} · {} · {paper}", wb.sheet().name)
        }
    }

    /// Set the toolbar and formula bar from the active cell.
    fn sync(&self) {
        self.syncing.set(true);
        let (r, c) = self.cursor.get();
        let src = self.src_row(r);
        let wb = self.wb.borrow();
        let sheet = wb.sheet();
        let f = sheet.format(src, c);
        let a = self.selection();
        self.name.set_text(&if a.is_cell() { formula::cell_name(src, c) } else { self.selection_src().name() });
        if !self.fx.has_focus() {
            self.fx.set_text(sheet.raw(src, c));
        }
        if let Some(ctl) = self.controls.borrow().as_ref() {
            ctl.bold.set_active(f.bold);
            ctl.wrap.set_active(f.wrap);
            ctl.checkbox.set_active(f.checkbox);
            ctl.italic.set_active(f.italic);
            for (n, b) in &ctl.numbers {
                b.set_active(*n == f.number && f.pattern.is_none());
            }
            if f.pattern.is_some() {
                ctl.formats.add_css_class("active-filter");
                ctl.formats.set_tooltip_text(Some(&format!("Number format: {}", f.pattern.as_deref().unwrap_or(""))));
            } else {
                ctl.formats.remove_css_class("active-filter");
                ctl.formats.set_tooltip_text(Some("More number formats"));
            }
            for (al, b) in &ctl.align {
                b.set_active(*al == f.align);
            }
            ctl.color.set_current(f.color.as_deref().or(Some("#202329")));
            ctl.fill.set_current(f.fill.as_deref());
            ctl.header.set_active(sheet.header);
            if sheet.freeze_rows + sheet.freeze_cols > 0 {
                ctl.freeze.add_css_class("active-filter");
            } else {
                ctl.freeze.remove_css_class("active-filter");
            }
            ctl.chart.set_active(self.chart.pane.is_visible());
            if sheet.filter.is_some() {
                ctl.filter.add_css_class("active-filter");
            } else {
                ctl.filter.remove_css_class("active-filter");
            }
        }
        match &sheet.chart {
            Some(ch) => {
                self.chart.range.set_text(&ch.range);
                self.chart.kind.set_selected(ChartKind::ALL.iter().position(|k| *k == ch.kind).unwrap_or(0) as u32);
                for (e, t) in self.chart.titles.iter().zip([&ch.text.title, &ch.text.x_title, &ch.text.y_title]) {
                    if !e.has_focus() && e.text() != t.as_str() {
                        e.set_text(t);
                    }
                }
            }
            None => self.chart.range.set_text(""),
        }
        drop(wb);
        self.syncing.set(false);
        doc::status_changed();
    }

    // ---------- Files ----------

    fn load(&self, wb: Workbook) {
        self.end_edit(false);
        *self.wb.borrow_mut() = wb;
        self.undo.borrow_mut().clear();
        self.redo.borrow_mut().clear();
        *self.clip.borrow_mut() = None;
        self.found.borrow_mut().clear();
        self.cursor.set((0, 0));
        self.anchor.set((0, 0));
        self.hadj.set_value(0.0);
        self.vadj.set_value(0.0);
        self.after_change(false);
    }

    fn native_bytes(&self) -> Result<Vec<u8>> {
        container::write(FORMAT, &self.wb.borrow().to_json(), &Default::default())
    }

    fn read_native(bytes: &[u8]) -> Result<Workbook> {
        if container::is_zip(bytes) {
            let c = container::read(bytes, FORMAT)?;
            return Workbook::from_json(&c.content);
        }
        // Files from the Qt version are plain JSON.
        Workbook::from_legacy_json(bytes)
    }
}

fn gio_none() -> Option<&'static gtk::gio::Cancellable> {
    None
}

impl Find for Sheets {
    fn find(&self, query: &str, case: bool, forward: bool) -> bool {
        let q = if case { query.to_string() } else { query.to_lowercase() };
        let wb = self.wb.borrow();
        let calc = self.calc.borrow();
        let s = wb.sheet();
        let view = self.view.borrow();
        // On-screen order: rows, then columns.
        let mut hits: Vec<(u32, u32)> = Vec::new();
        for (vi, src) in view.iter().enumerate() {
            for ((_, c), cell) in s.cells.range((*src, 0)..(*src + 1, 0)) {
                let shown = display(&calc.value(&wb, wb.active, *src, *c), &cell.raw, &cell.format).0;
                let hay = if case { shown } else { shown.to_lowercase() };
                let raw = if case { cell.raw.clone() } else { cell.raw.to_lowercase() };
                if hay.contains(&q) || raw.contains(&q) {
                    hits.push((vi as u32, *c));
                }
            }
            if s.used().is_some_and(|(lr, _)| *src > lr) && s.filter.is_none() {
                break;
            }
        }
        drop((wb, calc, view));
        if hits.is_empty() {
            return false;
        }
        let cur = self.cursor.get();
        let next = if forward {
            hits.iter().find(|h| **h > cur).or(hits.first())
        } else {
            hits.iter().rev().find(|h| **h < cur).or(hits.last())
        };
        if let Some((r, c)) = next.copied() {
            self.anchor.set((r, c));
            self.cursor.set((r, c));
            self.scroll_to(r, c);
            self.sync();
            self.area.queue_draw();
        }
        true
    }

    fn replace(&self, query: &str, with: &str, case: bool) -> bool {
        let (r, c) = self.cursor.get();
        let src = self.src_row(r);
        let raw = self.wb.borrow().sheet().raw(src, c).to_string();
        let replaced = replace_in(&raw, query, with, case);
        if replaced != raw {
            self.change(|wb| wb.set(src, c, &replaced));
        }
        Find::find(self, query, case, true)
    }

    fn replace_all(&self, query: &str, with: &str, case: bool) -> usize {
        let mut n = 0;
        self.change(|wb| {
            let keys: Vec<(u32, u32)> = wb.sheet().cells.keys().copied().collect();
            for (r, c) in keys {
                let raw = wb.sheet().raw(r, c).to_string();
                let replaced = replace_in(&raw, query, with, case);
                if replaced != raw {
                    n += 1;
                    wb.set(r, c, &replaced);
                }
            }
        });
        n
    }

    fn count(&self, query: &str, case: bool) -> usize {
        let q = if case { query.to_string() } else { query.to_lowercase() };
        let wb = self.wb.borrow();
        let calc = self.calc.borrow();
        let s = wb.sheet();
        let mut found = Vec::new();
        for ((r, c), cell) in &s.cells {
            let shown = display(&calc.value(&wb, wb.active, *r, *c), &cell.raw, &cell.format).0;
            let (hay, raw) = if case { (shown, cell.raw.clone()) } else { (shown.to_lowercase(), cell.raw.to_lowercase()) };
            if !q.is_empty() && (hay.contains(&q) || raw.contains(&q)) {
                found.push((*r, *c));
            }
        }
        let n = found.len();
        *self.found.borrow_mut() = found;
        self.area.queue_draw();
        n
    }

    fn done(&self) {
        self.found.borrow_mut().clear();
        self.area.queue_draw();
    }
}

fn replace_in(text: &str, query: &str, with: &str, case: bool) -> String {
    if query.is_empty() {
        return text.to_string();
    }
    if case {
        return text.replace(query, with);
    }
    let lower = text.to_lowercase();
    if lower.len() != text.len() {
        return text.to_string();
    }
    let q = query.to_lowercase();
    let mut out = String::new();
    let mut last = 0;
    for (i, _) in lower.match_indices(&q) {
        out.push_str(&text[last..i]);
        out.push_str(with);
        last = i + q.len();
    }
    out.push_str(&text[last..]);
    out
}

impl doc::Editor for Sheets {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    fn templates(&self) -> &'static [Template] {
        templates::TEMPLATES
    }

    fn new_document(&self, template: Option<&str>) {
        let wb = template.and_then(templates::build).unwrap_or_default();
        let chart = wb.sheet().chart.is_some();
        self.load(wb);
        self.show_chart(chart);
    }

    fn open(&self, path: &Path) -> Result<()> {
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let bytes = std::fs::read(path)?;
        if bytes.len() > 64 * 1024 * 1024 {
            bail!("it's larger than 64 MB");
        }
        let wb = match ext.as_str() {
            "nsheets" => Sheets::read_native(&bytes)?,
            "csv" => Workbook::from_csv(&String::from_utf8_lossy(&bytes))?,
            "xlsx" => {
                let opened = xlsx::read(&bytes)?;
                if opened.truncated {
                    window::toast(&format!(
                        "Only the first {} rows and {COLS} columns of each sheet were opened.",
                        crate::fmt::thousands(ROWS as usize)
                    ));
                } else if opened.dropped_rules {
                    window::toast("Some conditional formats use rules Sheets doesn't have; they were left out.");
                }
                opened.workbook
            }
            _ => bail!("Nexus Sheets opens .nsheets, .xlsx and .csv files"),
        };
        let chart = wb.sheet().chart.is_some();
        self.load(wb);
        self.chart.pane.set_visible(chart);
        self.sync();
        Ok(())
    }

    fn save(&self, path: &Path) -> Result<Vec<String>> {
        self.end_edit(true);
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        match ext.as_str() {
            "nsheets" => {
                crate::cmd::atomic_write(path, self.native_bytes()?)?;
                Ok(Vec::new())
            }
            "csv" => {
                let wb = self.wb.borrow();
                crate::cmd::atomic_write(path, wb.to_csv())?;
                let mut lost = Vec::new();
                if wb.sheets.len() > 1 {
                    lost.push(format!("the other {}", crate::fmt::count(wb.sheets.len() - 1, "sheet", "sheets")));
                }
                if wb.sheet().has_formatting() {
                    lost.push("formatting, sizes, merged cells, comments, highlight rules or the chart".into());
                }
                Ok(lost)
            }
            "xlsx" => {
                let (bytes, lost) = xlsx::write(&self.wb.borrow(), &self.calc.borrow())?;
                crate::cmd::atomic_write(path, bytes)?;
                Ok(lost)
            }
            _ => bail!("Nexus Sheets saves .nsheets, .xlsx and .csv files"),
        }
    }

    fn snapshot(&self) -> Result<Vec<u8>> {
        self.native_bytes()
    }

    fn restore(&self, bytes: &[u8]) -> Result<()> {
        self.load(Sheets::read_native(bytes)?);
        Ok(())
    }

    fn undo(&self) {
        self.end_edit(false);
        let Some(prev) = self.undo.borrow_mut().pop() else { return };
        let now = self.wb.replace(prev);
        self.redo.borrow_mut().push(now);
        self.clamp_cursor();
        self.after_change(true);
    }

    fn redo(&self) {
        self.end_edit(false);
        let Some(next) = self.redo.borrow_mut().pop() else { return };
        let now = self.wb.replace(next);
        self.undo.borrow_mut().push(now);
        self.clamp_cursor();
        self.after_change(true);
    }

    fn status(&self) -> String {
        self.selection_stats()
    }

    fn find(&self) -> Option<Rc<dyn Find>> {
        Some(self.rc())
    }

    fn pages(&self) -> Rc<dyn Pages> {
        self.end_edit(true);
        Rc::new(pages::SheetPages::new(&self.wb.borrow(), &self.calc.borrow()))
    }

    fn set_zoom(&self, zoom: f64) {
        if let Some(b) = self.zoomed.borrow().as_ref() {
            crate::widgets::set_zoom(b, zoom);
        }
    }

    fn focus(&self) {
        if !self.editing.get() {
            self.area.grab_focus();
        }
    }

    fn shortcuts(&self) -> &'static [doc::Shortcut] {
        &[
            (&["Enter"], "Keep the entry and move down (Shift: up)"),
            (&["Tab"], "Keep the entry and move right (Shift: left)"),
            (&["F2"], "Edit the cell (or double-click it)"),
            (&["Esc"], "Cancel the entry"),
            (&["Shift", "Arrows"], "Select cells"),
            (&["Ctrl", "Arrows"], "Jump to the edge of the data"),
            (&["Ctrl", "A"], "Select the whole sheet"),
            (&["Delete"], "Clear the selected cells"),
            (&["Ctrl", "C"], "Copy (Ctrl+X cuts, then paste moves)"),
            (&["Drag", "Edge"], "Drag the selection by its edge to move it (hold Ctrl to copy)"),
            (&["Ctrl", "V"], "Paste; a larger selection repeats the copy"),
            (&["Ctrl", "Shift", "V"], "Paste values only (right-click for formatting only)"),
            (&["Ctrl", "+"], "Insert rows above (columns, when whole columns are selected)"),
            (&["Ctrl", "−"], "Delete the selected rows (or whole columns)"),
            (&["Right-click"], "Cut, copy, paste, insert, delete, hide or show rows and columns"),
            (&["Ctrl", "9"], "Hide the selected rows (Ctrl+Shift+9 shows them)"),
            (&["Ctrl", "0"], "Hide the selected columns (Ctrl+Shift+0 shows them)"),
            (&["Ctrl", "D"], "Fill down"),
            (&["Ctrl", "R"], "Fill right"),
            (&["Ctrl", "B"], "Bold"),
            (&["Ctrl", "I"], "Italic"),
            (&["Ctrl", "PgDn"], "Next sheet (PgUp: the one before)"),
            (&["Ctrl", "Alt", "M"], "Comment on the cell"),
        ]
    }

    fn key(&self, key: gdk::Key, mods: gdk::ModifierType) -> bool {
        if !self.area.has_focus() {
            return false;
        }
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
        if !ctrl {
            return false;
        }
        if mods.contains(gdk::ModifierType::ALT_MASK) {
            if key.to_lower() == gdk::Key::m {
                self.edit_comment();
                return true;
            }
            return false;
        }
        match key.to_lower() {
            gdk::Key::b => {
                let on = !self.wb.borrow().sheet().format(self.src_row(self.cursor.get().0), self.cursor.get().1).bold;
                self.format_selection(move |f| f.bold = on);
            }
            gdk::Key::i => {
                let on = !self.wb.borrow().sheet().format(self.src_row(self.cursor.get().0), self.cursor.get().1).italic;
                self.format_selection(move |f| f.italic = on);
            }
            // Whole columns selected: columns; otherwise rows.
            gdk::Key::plus | gdk::Key::equal | gdk::Key::KP_Add => {
                if self.whole_cols(self.selection()) { self.insert_cols(false) } else { self.insert_rows(false) }
            }
            gdk::Key::minus | gdk::Key::KP_Subtract => {
                if self.whole_cols(self.selection()) { self.delete_cols() } else { self.delete_rows() }
            }
            // As in other spreadsheets: Ctrl+9 hides rows, Ctrl+0 columns; with Shift, shows them.
            gdk::Key::_9 | gdk::Key::parenleft => self.hide_rows(!mods.contains(gdk::ModifierType::SHIFT_MASK)),
            gdk::Key::_0 | gdk::Key::parenright => self.hide_cols(!mods.contains(gdk::ModifierType::SHIFT_MASK)),
            gdk::Key::Page_Down | gdk::Key::Page_Up => {
                let (n, i) = (self.wb.borrow().sheets.len(), self.wb.borrow().active);
                let j = if key == gdk::Key::Page_Down { (i + 1) % n } else { (i + n - 1) % n };
                self.select_sheet(j);
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
pub fn write_sample(path: &Path) {
    let mut wb = templates::build("monthly-budget").expect("a template");
    wb.set(1, 1, "1200");
    wb.set(1, 2, "1150");
    wb.set(2, 1, "200");
    let calc = Calc::new(&wb, 0.0);
    let (bytes, _) = xlsx::write(&wb, &calc).expect("an xlsx");
    std::fs::write(path, bytes).expect("written");
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::doc::Editor;

    /// Run from `crate::gtk_tests`: GTK lives on one thread, so the editors' checks
    /// run one after another in a single test.
    pub fn editor_behaves() {
        let dir = std::env::temp_dir().join(format!("nxo-sheets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let s = Sheets::build();

        // Typing into a cell and keeping it.
        s.move_to(0, 0, false);
        s.begin_edit(Some("5"), false);
        s.end_edit(true);
        s.move_to(1, 0, false);
        s.begin_edit(Some("=A1*2"), false);
        s.end_edit(true);
        assert_eq!(s.wb.borrow().sheet().raw(1, 0), "=A1*2");
        let shown = |s: &Sheets, r: u32, c: u32| {
            let wb = s.wb.borrow();
            display(&s.calc.borrow().value(&wb, 0, r, c), wb.sheet().raw(r, c), &wb.sheet().format(r, c)).0
        };
        assert_eq!(shown(&s, 1, 0), "10");

        // Escape throws the entry away.
        s.begin_edit(Some("nope"), false);
        s.end_edit(false);
        assert_eq!(s.wb.borrow().sheet().raw(1, 0), "=A1*2");

        // Format a selection, then undo and redo it.
        s.anchor.set((0, 0));
        s.cursor.set((1, 0));
        s.format_selection(|f| f.bold = true);
        assert!(s.wb.borrow().sheet().format(1, 0).bold);
        Editor::undo(&*s);
        assert!(!s.wb.borrow().sheet().format(1, 0).bold);
        Editor::redo(&*s);
        assert!(s.wb.borrow().sheet().format(0, 0).bold);

        // Fill down with Ctrl+D's command.
        s.anchor.set((1, 0));
        s.cursor.set((3, 0));
        s.fill_down_or_right(true);
        assert_eq!(s.wb.borrow().sheet().raw(3, 0), "=A3*2");
        assert_eq!(shown(&s, 3, 0), "40");

        // Insert a row above row 2: the formula below follows A1, then undo.
        s.anchor.set((1, 0));
        s.cursor.set((1, 0));
        s.insert_rows(false);
        assert_eq!(s.wb.borrow().sheet().raw(2, 0), "=A1*2");
        assert_eq!(s.wb.borrow().sheet().raw(1, 0), "");
        assert_eq!(s.selection(), Area::new(1, 0, 1, 0));
        s.delete_rows();
        assert_eq!(s.wb.borrow().sheet().raw(1, 0), "=A1*2");
        s.anchor.set((0, 0));
        s.cursor.set((0, 0));
        s.insert_cols(false);
        assert_eq!(s.wb.borrow().sheet().raw(1, 1), "=B1*2");
        Editor::undo(&*s);
        assert_eq!(s.wb.borrow().sheet().raw(1, 0), "=A1*2");

        // A dropdown list takes only its choices, written as the list writes them.
        s.change(|wb| {
            wb.set_list(Area::cell(20, 0), Some(lists::ListRule { range: String::new(), items: vec!["Paid".into(), "Unpaid".into()], source: None }))
        });
        s.commit_raw(20, 0, "maybe");
        assert_eq!(s.wb.borrow().sheet().raw(20, 0), "");
        s.commit_raw(20, 0, " paid ");
        assert_eq!(s.wb.borrow().sheet().raw(20, 0), "Paid");

        // Frozen panes: the first row and column stay put while the rest scrolls.
        s.area.set_size_request(800, 600);
        s.change(|wb| {
            wb.sheet_mut().freeze_rows = 1;
            wb.sheet_mut().freeze_cols = 1;
        });
        s.update_adjustments();
        s.vadj.set_value(300.0);
        s.hadj.set_value(500.0);
        let (y0, _) = s.row_rect(0);
        let (x0, _) = s.col_rect(0);
        assert_eq!((y0, x0), (grid::HEAD_H, grid::HEAD_W));
        assert_eq!(s.cell_at(grid::HEAD_W + 5.0, grid::HEAD_H + 5.0), Some((0, 0)));
        // Just past the kept row and column: what's scrolled into view there.
        let (r, c) = s.cell_at(grid::HEAD_W + s.frozen_w() + 1.0, grid::HEAD_H + s.frozen_h() + 1.0).unwrap();
        assert!(r > 5 && c > 2, "{r} {c}");
        s.change(|wb| {
            wb.sheet_mut().freeze_rows = 0;
            wb.sheet_mut().freeze_cols = 0;
        });
        s.vadj.set_value(0.0);
        s.hadj.set_value(0.0);

        // Hide row 2: stepping down from row 1 lands on row 3; showing it again.
        s.anchor.set((1, 0));
        s.cursor.set((1, COLS - 1));
        s.hide_rows(true);
        assert!(s.wb.borrow().sheet().hidden_rows.contains(&1));
        assert_eq!(s.cursor.get().0, 2);
        s.move_to(0, 0, false);
        assert_eq!(s.step(1, 0), (2, 0));
        assert_eq!(shown(&s, 1, 0), "10", "hidden cells still calculate");
        s.anchor.set((0, 0));
        s.cursor.set((2, COLS - 1));
        s.hide_rows(false);
        assert!(s.wb.borrow().sheet().hidden_rows.is_empty());
        // Hiding every column is refused.
        s.anchor.set((0, 0));
        s.cursor.set((0, COLS - 1));
        s.hide_cols(true);
        assert!(s.wb.borrow().sheet().hidden_cols.is_empty());

        // A tab per sheet, the active one marked; adding a sheet adds a tab.
        let tabs = |s: &Sheets| {
            let mut out = Vec::new();
            let mut c = s.sheet_list.first_child();
            while let Some(w) = c {
                out.push(w.has_css_class("active"));
                c = w.next_sibling();
            }
            out
        };
        assert_eq!(tabs(&s), [true]);
        s.add_sheet();
        assert_eq!(tabs(&s), [false, true]);
        s.select_sheet(0);
        assert_eq!(tabs(&s), [true, false]);
        Editor::undo(&*s);
        assert_eq!(tabs(&s).len(), 1);

        // Dragging cells by the selection's edge moves them; with Ctrl, copies them.
        s.change(|wb| wb.set(0, 4, "moved"));
        // (No window here: keep the grid unscrolled so cells sit where they're drawn.)
        s.hadj.set_value(0.0);
        s.vadj.set_value(0.0);
        s.anchor.set((0, 4));
        s.cursor.set((0, 4));
        let (x, y, w, _) = s.area_rect(Area::cell(0, 4));
        assert!(matches!(s.hit(x + w / 2.0, y), grid::Hit::Edge));
        s.press(x + w / 2.0, y + 1.0, false);
        let (tx, ty, _, _) = s.area_rect(Area::cell(2, 5));
        s.drag_to(tx + 5.0, ty + 5.0, 0.0, 0.0);
        s.release();
        assert_eq!(s.wb.borrow().sheet().raw(0, 4), "");
        assert_eq!(s.wb.borrow().sheet().raw(2, 5), "moved");
        assert_eq!(s.selection_src(), Area::cell(2, 5));
        s.hadj.set_value(0.0);
        s.vadj.set_value(0.0);
        let (x, y, w, _) = s.area_rect(Area::cell(2, 5));
        s.press(x + w / 2.0, y + 1.0, false);
        s.drag_copy.set(true);
        let (tx, ty, _, _) = s.area_rect(Area::cell(6, 5));
        s.drag_to(tx + 5.0, ty + 5.0, 0.0, 0.0);
        s.release();
        s.drag_copy.set(false);
        s.hadj.set_value(0.0);
        s.vadj.set_value(0.0);
        assert_eq!((s.wb.borrow().sheet().raw(2, 5).to_string(), s.wb.borrow().sheet().raw(6, 5).to_string()), ("moved".into(), "moved".into()));

        // A chart placed on the sheet: beside the selection, picked, removable.
        s.anchor.set((0, 0));
        s.cursor.set((3, 0));
        s.place_chart();
        assert_eq!(s.wb.borrow().sheet().charts.len(), 1);
        let (x, y, _, _) = s.chart_rect(&s.wb.borrow().sheet().charts[0].clone());
        assert_eq!(s.chart_hit(x + 10.0, y + 10.0), Some((0, false)));
        s.delete_chart();
        assert!(s.wb.borrow().sheet().charts.is_empty());

        // Merging: the block acts as one cell for the cursor and the selection.
        s.anchor.set((5, 0));
        s.cursor.set((6, 1));
        s.merge_cells();
        assert_eq!(s.wb.borrow().sheet().merges, [Area::new(5, 0, 6, 1)]);
        s.move_to(6, 1, false);
        assert_eq!(s.cursor.get(), (5, 0));
        assert_eq!(s.selection(), Area::new(5, 0, 6, 1));
        assert_eq!(s.step(1, 0), (7, 0));
        assert_eq!(s.step(0, 1), (5, 2));
        s.merge_cells();
        assert!(s.wb.borrow().sheet().merges.is_empty());

        // The status bar adds up the selection.
        s.anchor.set((0, 0));
        s.cursor.set((3, 0));
        assert!(s.selection_stats().contains("Sum 75"), "{}", s.selection_stats());

        // Save, open, and the CSV note.
        let path = dir.join("book.nsheets");
        Editor::save(&*s, &path).unwrap();
        let before = s.wb.borrow().clone();
        Editor::new_document(&*s, None);
        Editor::open(&*s, &path).unwrap();
        assert_eq!(*s.wb.borrow(), before);
        let lost = Editor::save(&*s, &dir.join("book.csv")).unwrap();
        assert!(!lost.is_empty(), "bold can't go in a CSV");

        // Find, replace, and the PDF.
        assert_eq!(Find::count(&*s, "a1", false), 1);
        assert_eq!(Find::replace_all(&*s, "A1", "A2", true), 1);
        assert_eq!(s.wb.borrow().sheet().raw(1, 0), "=A2*2");
        let pdf = dir.join("book.pdf");
        crate::print::export_pdf(s.pages().as_ref(), &pdf).unwrap();
        assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));

        // A template with a chart lays out a chart page too.
        Editor::new_document(&*s, Some("monthly-budget"));
        s.wb.borrow_mut().set(1, 1, "1200");
        s.after_change(false);
        assert_eq!(s.pages().count(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
