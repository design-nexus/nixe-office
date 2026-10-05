//! Editing a slide table's cells or a chart's numbers in a dialog, and the
//! toolbar menus for slide transitions and object builds.

use super::Present;
use super::model::*;
use crate::widgets;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// The most rows and columns a slide table or chart table takes.
const MAX_ROWS: usize = 20;
const MAX_COLS: usize = 8;

/// A grid of entries over `rows`, with buttons to add and remove rows and columns.
struct GridEditor {
    rows: Rc<RefCell<Vec<Vec<String>>>>,
    grid: gtk::Grid,
    entries: Rc<RefCell<Vec<Vec<gtk::Entry>>>>,
}

impl GridEditor {
    fn new(rows: Vec<Vec<String>>) -> Rc<GridEditor> {
        let g = Rc::new(GridEditor {
            rows: Rc::new(RefCell::new(rows)),
            grid: gtk::Grid::new(),
            entries: Rc::new(RefCell::new(Vec::new())),
        });
        g.grid.set_row_spacing(4);
        g.grid.set_column_spacing(4);
        g.grid.add_css_class("data-grid");
        g.rebuild();
        g
    }

    /// The text in the entries, read back into `rows`.
    fn read(&self) -> Vec<Vec<String>> {
        let rows: Vec<Vec<String>> =
            self.entries.borrow().iter().map(|r| r.iter().map(|e| e.text().to_string()).collect()).collect();
        *self.rows.borrow_mut() = rows.clone();
        rows
    }

    fn rebuild(&self) {
        while let Some(c) = self.grid.first_child() {
            self.grid.remove(&c);
        }
        let rows = self.rows.borrow();
        let mut entries = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let mut line = Vec::new();
            for (j, cell) in row.iter().enumerate() {
                let e = gtk::Entry::new();
                e.set_text(cell);
                e.set_width_chars(9);
                self.grid.attach(&e, j as i32, i as i32, 1, 1);
                line.push(e);
            }
            entries.push(line);
        }
        *self.entries.borrow_mut() = entries;
    }

    fn resize(&self, rows: i64, cols: i64) {
        let mut r = self.read();
        let n_cols = r.first().map(Vec::len).unwrap_or(1) as i64;
        let new_rows = (r.len() as i64 + rows).clamp(1, MAX_ROWS as i64) as usize;
        let new_cols = (n_cols + cols).clamp(1, MAX_COLS as i64) as usize;
        r.resize(new_rows, vec![String::new(); new_cols]);
        for row in &mut r {
            row.resize(new_cols, String::new());
        }
        *self.rows.borrow_mut() = r;
        self.rebuild();
    }

    /// The +/− row and column buttons.
    fn buttons(self: &Rc<Self>) -> gtk::Box {
        let bar = widgets::hbox(6);
        for (label, dr, dc) in [("+ Row", 1, 0), ("− Row", -1, 0), ("+ Column", 0, 1), ("− Column", 0, -1)] {
            let b = gtk::Button::with_label(label);
            let me = self.clone();
            b.connect_clicked(move |_| me.resize(dr, dc));
            bar.append(&b);
        }
        bar
    }
}

/// A dialog around a grid editor: title, extra controls, Cancel and Save.
fn grid_dialog(
    title: &str,
    hint: &str,
    editor: &Rc<GridEditor>,
    extra: Option<&gtk::Widget>,
    save: impl Fn(Vec<Vec<String>>) -> Result<(), String> + 'static,
) {
    let (dialog, card) = widgets::dialog(title, 560);
    if !hint.is_empty() {
        let h = widgets::label(hint, "dim");
        h.set_wrap(true);
        h.set_xalign(0.0);
        card.append(&h);
    }
    if let Some(w) = extra {
        card.append(w);
    }
    let scroll = gtk::ScrolledWindow::builder()
        .child(&editor.grid)
        .min_content_height(160)
        .max_content_height(380)
        .propagate_natural_height(true)
        .propagate_natural_width(true)
        .build();
    card.append(&scroll);
    card.append(&editor.buttons());
    let err = widgets::label("", "danger-text");
    err.set_visible(false);
    card.append(&err);
    let row = widgets::hbox(8);
    row.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    let ok = gtk::Button::with_label("Save");
    ok.add_css_class("suggested-action");
    row.append(&cancel);
    row.append(&ok);
    card.append(&row);
    let d = dialog.clone();
    cancel.connect_clicked(move |_| d.close());
    let (d, ed) = (dialog.clone(), editor.clone());
    ok.connect_clicked(move |_| match save(ed.read()) {
        Ok(()) => d.close(),
        Err(m) => {
            err.set_text(&m);
            err.set_visible(true);
        }
    });
    dialog.present();
}

impl Present {
    /// Double-click or Enter on a table or chart: edit its contents.
    pub(super) fn edit_data(&self, id: u32) {
        let Some(o) = self.slide().objects.into_iter().find(|o| o.id == id) else { return };
        match o.kind {
            Kind::Table { rows, header } => {
                let editor = GridEditor::new(rows);
                let head = gtk::CheckButton::with_label("First row is a heading");
                head.set_active(header);
                let me = self.weak();
                let h = head.clone();
                grid_dialog("Table", "", &editor, Some(head.upcast_ref()), move |rows| {
                    if let Some(p) = me.upgrade() {
                        let header = h.is_active();
                        p.change_slide(|s| {
                            if let Some(o) = s.objects.iter_mut().find(|o| o.id == id) {
                                o.kind = Kind::Table { rows, header };
                            }
                        });
                    }
                    Ok(())
                });
            }
            Kind::Chart { chart, names, labels, series } => {
                let me = self.weak();
                edit_chart(chart, &names, &labels, &series, move |new| {
                    if let Some(p) = me.upgrade() {
                        p.change_slide(|s| {
                            if let Some(o) = s.objects.iter_mut().find(|o| o.id == id) {
                                o.kind = new;
                            }
                        });
                    }
                });
            }
            _ => {}
        }
    }

    /// The slide's transition: a menu of effects and speeds.
    pub(super) fn transition_button(self: &Rc<Self>) -> gtk::MenuButton {
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("nxo-sparkles-symbolic");
        menu.add_css_class("bar-button");
        menu.set_tooltip_text(Some("Transition: how this slide comes in during the slideshow"));
        menu.set_valign(gtk::Align::Center);
        let pop = gtk::Popover::new();
        let content = widgets::vbox(10);
        content.add_css_class("prompt-box");
        content.append(&widgets::label("Transition", "popover-title"));
        let mut names = vec!["None"];
        names.extend(TransitionKind::ALL.iter().map(|k| k.name()));
        let kind = gtk::DropDown::from_strings(&names);
        let speeds = [("Fast", 0.5), ("Medium", 0.75), ("Slow", 1.5)];
        let speed = gtk::DropDown::from_strings(&speeds.iter().map(|s| s.0).collect::<Vec<_>>());
        let row = widgets::hbox(8);
        row.append(&kind);
        row.append(&speed);
        content.append(&row);
        let all = gtk::Button::with_label("Use on every slide");
        content.append(&all);
        pop.set_child(Some(&content));
        menu.set_popover(Some(&pop));
        let read = {
            let (kind, speed) = (kind.clone(), speed.clone());
            move || {
                let k = kind.selected() as usize;
                (k > 0).then(|| Transition {
                    kind: TransitionKind::ALL[(k - 1).min(TransitionKind::ALL.len() - 1)],
                    secs: speeds[(speed.selected() as usize).min(2)].1,
                })
            }
        };
        // Show the slide's setting when the menu opens.
        let me = Rc::downgrade(self);
        let (k2, s2) = (kind.clone(), speed.clone());
        pop.connect_show(move |_| {
            let Some(p) = me.upgrade() else { return };
            let t = p.slide().transition;
            p.syncing.set(true);
            k2.set_selected(t.map(|t| TransitionKind::ALL.iter().position(|k| *k == t.kind).unwrap_or(0) + 1).unwrap_or(0) as u32);
            s2.set_selected(t.map(|t| if t.secs < 0.6 { 0 } else if t.secs < 1.2 { 1 } else { 2 }).unwrap_or(1));
            p.syncing.set(false);
        });
        for d in [&kind, &speed] {
            let me = Rc::downgrade(self);
            let read = read.clone();
            d.connect_selected_notify(move |_| {
                if let Some(p) = me.upgrade()
                    && !p.syncing.get()
                {
                    let t = read();
                    p.change_slide(|s| s.transition = t);
                }
            });
        }
        let me = Rc::downgrade(self);
        let p2 = pop.clone();
        all.connect_clicked(move |_| {
            p2.popdown();
            if let Some(p) = me.upgrade() {
                let t = read();
                p.change_deck(|d| {
                    for s in &mut d.slides {
                        s.transition = t;
                    }
                });
                crate::window::toast(if t.is_some() { "Every slide uses this transition." } else { "Transitions are off on every slide." });
            }
        });
        menu
    }

    /// The selected object's builds: whether it comes in, or goes out, on a
    /// click; how; and when.
    pub(super) fn build_button(self: &Rc<Self>) -> gtk::MenuButton {
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("nxo-wand-sparkles-symbolic");
        menu.add_css_class("bar-button");
        menu.set_tooltip_text(Some("Builds: bring the object in, or take it away, on a click during the slideshow"));
        menu.set_valign(gtk::Align::Center);
        let pop = gtk::Popover::new();
        let content = widgets::vbox(10);
        content.add_css_class("prompt-box");
        content.append(&widgets::label("Build in", "popover-title"));
        let mut names = vec!["None (shown from the start)"];
        names.extend(Effect::ALL.iter().map(|e| e.name()));
        let effect = gtk::DropDown::from_strings(&names);
        content.append(&effect);
        content.append(&widgets::label("Build out", "popover-title"));
        let out_names = ["None (stays to the end)", "Disappear", "Fade out", "Fly out"];
        let effect_out = gtk::DropDown::from_strings(&out_names);
        content.append(&effect_out);
        let when = widgets::label("", "dim");
        when.set_wrap(true);
        when.set_xalign(0.0);
        content.append(&when);
        let order = widgets::hbox(6);
        let earlier = gtk::Button::with_label("Earlier");
        let later = gtk::Button::with_label("Later");
        order.append(&earlier);
        order.append(&later);
        content.append(&order);
        pop.set_child(Some(&content));
        menu.set_popover(Some(&pop));

        let refresh = {
            let me = Rc::downgrade(self);
            let (effect, effect_out, when, order) = (effect.clone(), effect_out.clone(), when.clone(), order.clone());
            move || {
                let Some(p) = me.upgrade() else { return };
                let s = p.slide();
                let Some(id) = p.selected.get() else { return };
                let o = s.objects.iter().find(|o| o.id == id);
                let (b, bo) = (o.and_then(|o| o.build), o.and_then(|o| o.build_out));
                let pos = |e: Option<Build>| e.map(|b| Effect::ALL.iter().position(|x| *x == b.effect).unwrap_or(0) + 1).unwrap_or(0) as u32;
                p.syncing.set(true);
                effect.set_selected(pos(b));
                effect_out.set_selected(pos(bo));
                p.syncing.set(false);
                let steps = s.steps();
                let at = |out: bool| steps.iter().position(|x| x.id == id && x.out == out).map(|i| i + 1);
                let mut parts = Vec::new();
                match at(false) {
                    Some(k) => parts.push(format!("Comes in on click {k}")),
                    None => parts.push("Shown from the start".to_string()),
                }
                if let Some(k) = at(true) {
                    parts.push(format!("goes out on click {k}"));
                }
                when.set_text(&format!("{} of {}.", parts.join(", "), steps.len().max(1)));
                order.set_visible(b.is_some() && steps.len() > 1);
            }
        };
        let r = refresh.clone();
        pop.connect_show(move |_| r());
        for (dd, out) in [(&effect, false), (&effect_out, true)] {
            let me = Rc::downgrade(self);
            let r = refresh.clone();
            dd.connect_selected_notify(move |d| {
                let Some(p) = me.upgrade() else { return };
                if p.syncing.get() {
                    return;
                }
                let chosen = p.chosen();
                let i = d.selected() as usize;
                p.change_slide(|s| {
                    let mut next = s.objects.iter().flat_map(|o| [o.build, o.build_out]).flatten().map(|b| b.order).max().unwrap_or(0) + 1;
                    for o in s.objects.iter_mut().filter(|o| chosen.contains(&o.id)) {
                        let slot = if out { &mut o.build_out } else { &mut o.build };
                        *slot = if i == 0 {
                            None
                        } else {
                            let effect = Effect::ALL[(i - 1).min(2)];
                            // A new build goes last; a changed one keeps its turn.
                            let order = slot.map(|b| b.order).unwrap_or_else(|| {
                                next += 1;
                                next - 1
                            });
                            Some(Build { order, effect })
                        };
                    }
                });
                r();
            });
        }
        for (b, by) in [(&earlier, -1i64), (&later, 1)] {
            let me = Rc::downgrade(self);
            let r = refresh.clone();
            b.connect_clicked(move |_| {
                let Some(p) = me.upgrade() else { return };
                let Some(id) = p.selected.get() else { return };
                p.change_slide(|s| {
                    let mut steps = s.steps();
                    let Some(i) = steps.iter().position(|x| x.id == id && !x.out) else { return };
                    let j = i as i64 + by;
                    if j < 0 || j as usize >= steps.len() {
                        return;
                    }
                    // Number every step afresh with the two swapped.
                    steps.swap(i, j as usize);
                    for (n, st) in steps.iter().enumerate() {
                        if let Some(o) = s.objects.iter_mut().find(|o| o.id == st.id)
                            && let Some(b) = if st.out { &mut o.build_out } else { &mut o.build }
                        {
                            b.order = n as u32 + 1;
                        }
                    }
                });
                r();
            });
        }
        menu
    }
}

/// Edit a chart's kind and numbers in a table; `save` gets the new chart.
pub(crate) fn edit_chart(chart: ChartKind, names: &[String], labels: &[String], series: &[Vec<f64>], save: impl Fn(Kind) + 'static) {
    // As a table: series names across the top, a label and its values per row.
    let mut rows = vec![std::iter::once(String::new()).chain(names.iter().cloned()).collect::<Vec<_>>()];
    for (i, l) in labels.iter().enumerate() {
        let mut r = vec![l.clone()];
        r.extend(series.iter().map(|s| s.get(i).map(|v| crate::sheets::formula::general(*v)).unwrap_or_default()));
        rows.push(r);
    }
    let editor = GridEditor::new(rows);
    let names_list: Vec<&str> = ChartKind::ALL.iter().map(|k| k.name()).collect();
    let kind = gtk::DropDown::from_strings(&names_list);
    kind.set_selected(ChartKind::ALL.iter().position(|k| *k == chart).unwrap_or(0) as u32);
    let kind_row = widgets::hbox(8);
    kind_row.append(&widgets::label("Kind", "dim"));
    kind_row.append(&kind);
    let k = kind.clone();
    grid_dialog(
        "Chart data",
        "Series names across the top row; a label and its numbers on each row below. Pie charts show the first series; scatter charts take the labels as x values.",
        &editor,
        Some(kind_row.upcast_ref()),
        move |rows| {
            save(chart_from_rows(&rows, ChartKind::ALL[(k.selected() as usize).min(ChartKind::ALL.len() - 1)])?);
            Ok(())
        },
    );
}

/// Chart numbers from the editing table. Blank cells count as 0.
pub fn chart_from_rows(rows: &[Vec<String>], chart: ChartKind) -> Result<Kind, String> {
    let head = rows.first().ok_or("Add a row of numbers.")?;
    let names: Vec<String> =
        head.iter().skip(1).enumerate().map(|(i, n)| if n.trim().is_empty() { format!("Series {}", i + 1) } else { n.trim().to_string() }).collect();
    if names.is_empty() {
        return Err("Add a column of numbers.".into());
    }
    let mut labels = Vec::new();
    let mut series = vec![Vec::new(); names.len()];
    for (r, row) in rows.iter().enumerate().skip(1) {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        labels.push(row.first().map(|l| l.trim().to_string()).unwrap_or_default());
        for (k, s) in series.iter_mut().enumerate() {
            let cell = row.get(k + 1).map(|c| c.trim()).unwrap_or("");
            let v = if cell.is_empty() {
                0.0
            } else {
                crate::sheets::formula::parse_number(cell)
                    .ok_or_else(|| format!("“{cell}” in row {} isn't a number.", r + 1))?
            };
            s.push(v);
        }
    }
    if labels.is_empty() {
        return Err("Add a row with a label and numbers.".into());
    }
    Ok(Kind::Chart { chart, names, labels, series })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_table_reads_back() {
        let rows = vec![
            vec!["".to_string(), "Sales".into(), "".into()],
            vec!["Jan".to_string(), "1,200".into(), "".into()],
            vec!["".to_string(), "".into(), "".into()],
            vec!["Feb".to_string(), "900".into(), "5".into()],
        ];
        let Kind::Chart { names, labels, series, .. } = chart_from_rows(&rows, ChartKind::Line).unwrap() else { panic!() };
        assert_eq!(names, ["Sales", "Series 2"]);
        assert_eq!(labels, ["Jan", "Feb"]);
        assert_eq!(series, [vec![1200.0, 900.0], vec![0.0, 5.0]]);
        let bad = vec![vec!["".to_string(), "A".into()], vec!["x".to_string(), "lots".into()]];
        assert!(chart_from_rows(&bad, ChartKind::Bar).unwrap_err().contains("lots"));
    }
}
