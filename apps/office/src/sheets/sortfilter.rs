//! Sorting by several columns, and filtering by picking the values to show.

use super::model::*;
use super::{Sheets, formula};
use crate::{widgets, window};
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// The most different values a filter lists to pick from.
pub const MAX_VALUES: usize = 500;

impl Filter {
    /// Whether it hides anything.
    pub fn active(&self) -> bool {
        !self.query.trim().is_empty() || self.values.is_some()
    }

    /// Whether a row whose cell shows `shown` stays in view.
    pub fn keeps(&self, shown: &str) -> bool {
        let q = self.query.trim().to_lowercase();
        (q.is_empty() || shown.to_lowercase().contains(&q))
            && self.values.as_ref().is_none_or(|vs| vs.iter().any(|v| v == shown))
    }
}

impl Workbook {
    /// A cell as it shows on sheet `si`.
    pub fn shown(&self, si: usize, r: u32, c: u32, calc: &Calc) -> String {
        let s = &self.sheets[si];
        display(&calc.value(self, si, r, c), s.raw(r, c), &s.format(r, c)).0
    }

    /// The different values column `col` shows under the header, in order, and
    /// whether the list was cut short.
    pub fn column_values(&self, si: usize, col: u32, calc: &Calc) -> (Vec<String>, bool) {
        let s = &self.sheets[si];
        let Some((last, _)) = s.used() else { return (Vec::new(), false) };
        let first = if s.header { 1 } else { 0 };
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for r in first..=last {
            let v = self.shown(si, r, col, calc);
            if seen.insert(v.clone()) {
                if out.len() == MAX_VALUES {
                    return (out, true);
                }
                out.push(v);
            }
        }
        // Numbers in number order, words alphabetically, empty last.
        out.sort_by(|a, b| match (a.is_empty(), b.is_empty()) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => match (formula::parse_number(a), formula::parse_number(b)) {
                (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                _ => a.to_lowercase().cmp(&b.to_lowercase()),
            },
        });
        (out, false)
    }

    /// Names for the columns of sheet `si`: "B · Cost" when the header names them.
    pub fn column_names(&self, si: usize, calc: &Calc) -> Vec<(u32, String)> {
        let s = &self.sheets[si];
        let last = s.used().map(|(_, c)| c).unwrap_or(0);
        (0..=last)
            .map(|c| {
                let letter = formula::col_name(c);
                let head = if s.header { self.shown(si, 0, c, calc) } else { String::new() };
                (c, if head.trim().is_empty() { format!("Column {letter}") } else { format!("{letter} · {}", head.trim()) })
            })
            .collect()
    }
}

// ---------- The dialog and the filter menu ----------

impl Sheets {
    /// Sort by up to three columns, each A to Z or Z to A.
    pub(super) fn sort_dialog(self: &Rc<Self>) {
        self.end_edit(true);
        let (si, names) = {
            let wb = self.wb.borrow();
            (wb.active, wb.column_names(wb.active, &self.calc.borrow()))
        };
        if names.is_empty() {
            window::toast("There's nothing to sort yet.");
            return;
        }
        let (dialog, card) = widgets::dialog("Sort", 440);
        let header = self.wb.borrow().sheet().header;
        let note = widgets::label(
            if header { "The first row stays on top as the header." } else { "Every row is sorted (the first row isn't a header)." },
            "dim",
        );
        note.set_xalign(0.0);
        card.append(&note);
        let cursor_col = self.cursor.get().1;
        let mut levels: Vec<(gtk::DropDown, gtk::DropDown)> = Vec::new();
        for i in 0..3 {
            let row = widgets::hbox(8);
            row.append(&widgets::label(if i == 0 { "Sort by" } else { "then by" }, "dim"));
            let mut labels: Vec<String> = if i == 0 { Vec::new() } else { vec!["(nothing)".into()] };
            labels.extend(names.iter().map(|(_, n)| n.clone()));
            let col = gtk::DropDown::from_strings(&labels.iter().map(String::as_str).collect::<Vec<_>>());
            col.set_hexpand(true);
            if i == 0 {
                col.set_selected(names.iter().position(|(c, _)| *c == cursor_col).unwrap_or(0) as u32);
            }
            let dir = gtk::DropDown::from_strings(&["A to Z, 1 to 9", "Z to A, 9 to 1"]);
            row.append(&col);
            row.append(&dir);
            card.append(&row);
            levels.push((col, dir));
        }
        let buttons = widgets::hbox(8);
        buttons.set_halign(gtk::Align::End);
        buttons.set_margin_top(6);
        let cancel = gtk::Button::with_label("Cancel");
        let ok = gtk::Button::with_label("Sort");
        ok.add_css_class("suggested-action");
        buttons.append(&cancel);
        buttons.append(&ok);
        card.append(&buttons);
        let d = dialog.clone();
        cancel.connect_clicked(move |_| d.close());
        let (d, me) = (dialog.clone(), self.weak());
        ok.connect_clicked(move |_| {
            d.close();
            let Some(s) = me.upgrade() else { return };
            let mut keys: Vec<(u32, bool)> = Vec::new();
            for (i, (col, dir)) in levels.iter().enumerate() {
                let pick = col.selected() as usize;
                // Below the first, item 0 is "(nothing)".
                let idx = if i == 0 { Some(pick) } else { pick.checked_sub(1) };
                if let Some((c, _)) = idx.and_then(|k| names.get(k))
                    && !keys.iter().any(|(k, _)| k == c)
                {
                    keys.push((*c, dir.selected() == 0));
                }
            }
            let calc = Calc::new(&s.wb.borrow(), super::today());
            let mut sorted = true;
            s.change(|wb| sorted = wb.sort_by(si, &keys, &calc));
            if !sorted {
                window::toast("Unmerge the merged cells in the data to sort it.");
                return;
            }
            let by: Vec<String> = keys.iter().map(|(c, _)| formula::col_name(*c)).collect();
            window::toast(&format!("Sorted by {}.", by.join(", then ")));
        });
        dialog.present();
    }

    /// Show only some rows: those whose cell in this column holds some text, or
    /// shows one of the values ticked.
    pub(super) fn filter_button(self: &Rc<Self>) -> gtk::MenuButton {
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("nxo-filter-symbolic");
        menu.add_css_class("bar-button");
        menu.set_tooltip_text(Some("Filter: show only some rows, by what this column holds"));
        menu.set_valign(gtk::Align::Center);
        let pop = gtk::Popover::new();
        let content = widgets::vbox(8);
        content.add_css_class("prompt-box");
        let title = widgets::label("Filter", "popover-title");
        content.append(&title);
        let query = gtk::Entry::new();
        query.set_placeholder_text(Some("Containing this text"));
        content.append(&query);
        // The values to pick from, filled in when the menu opens.
        let all_none = widgets::hbox(6);
        let all = gtk::Button::with_label("All");
        let none = gtk::Button::with_label("None");
        all.add_css_class("flat");
        none.add_css_class("flat");
        all_none.append(&widgets::label("Show these:", "dim"));
        all_none.append(&all);
        all_none.append(&none);
        content.append(&all_none);
        let list = widgets::vbox(0);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(260)
            .min_content_width(240)
            .child(&list)
            .build();
        content.append(&scroll);
        let buttons = widgets::hbox(8);
        buttons.set_halign(gtk::Align::End);
        let clear = gtk::Button::with_label("Show all rows");
        let apply = gtk::Button::with_label("Filter");
        apply.add_css_class("suggested-action");
        buttons.append(&clear);
        buttons.append(&apply);
        content.append(&buttons);
        pop.set_child(Some(&content));
        menu.set_popover(Some(&pop));

        let checks: Rc<RefCell<Vec<(String, gtk::CheckButton)>>> = Rc::default();
        let (me, t, q, l, ch) = (self.weak(), title.clone(), query.clone(), list.clone(), checks.clone());
        pop.connect_show(move |_| {
            let Some(s) = me.upgrade() else { return };
            let col = s.cursor.get().1;
            t.set_text(&format!("Filter column {}", formula::col_name(col)));
            let wb = s.wb.borrow();
            let current = wb.sheet().filter.clone().filter(|f| f.col == col);
            q.set_text(current.as_ref().map(|f| f.query.as_str()).unwrap_or(""));
            while let Some(c) = l.first_child() {
                l.remove(&c);
            }
            let (values, cut) = wb.column_values(wb.active, col, &s.calc.borrow());
            let mut made = Vec::new();
            for v in values {
                let label = if v.is_empty() { "(empty)".to_string() } else { v.clone() };
                let b = gtk::CheckButton::with_label(&label);
                b.set_active(current.as_ref().and_then(|f| f.values.as_ref()).is_none_or(|vs| vs.contains(&v)));
                l.append(&b);
                made.push((v, b));
            }
            if cut {
                l.append(&widgets::label(&format!("Only the first {MAX_VALUES} values are listed."), "dim"));
            }
            *ch.borrow_mut() = made;
        });
        for (button, on) in [(&all, true), (&none, false)] {
            let ch = checks.clone();
            button.connect_clicked(move |_| {
                for (_, b) in ch.borrow().iter() {
                    b.set_active(on);
                }
            });
        }
        let (me, p, q, ch) = (self.weak(), pop.clone(), query.clone(), checks.clone());
        let go = move || {
            p.popdown();
            let Some(s) = me.upgrade() else { return };
            let col = s.cursor.get().1;
            let text = q.text().trim().to_string();
            let ticked: Vec<String> = ch.borrow().iter().filter(|(_, b)| b.is_active()).map(|(v, _)| v.clone()).collect();
            // Every value ticked: no need to list them.
            let values = (ticked.len() < ch.borrow().len()).then_some(ticked);
            let f = Filter { col, query: text, values };
            s.change(|wb| wb.sheet_mut().filter = f.active().then_some(f.clone()));
        };
        let go = Rc::new(go);
        let g = go.clone();
        apply.connect_clicked(move |_| g());
        query.connect_activate(move |_| go());
        let (me, p) = (self.weak(), pop.clone());
        clear.connect_clicked(move |_| {
            p.popdown();
            if let Some(s) = me.upgrade() {
                s.change(|wb| wb.sheet_mut().filter = None);
            }
        });
        menu
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wb() -> Workbook {
        let mut wb = Workbook::default();
        for (r, (city, n)) in [("City", "Sales"), ("Leeds", "3"), ("York", "1"), ("Leeds", "1"), ("Hull", "2"), ("York", "")]
            .iter()
            .enumerate()
        {
            wb.set(r as u32, 0, city);
            wb.set(r as u32, 1, n);
        }
        wb
    }

    #[test]
    fn sorts_by_several_columns() {
        let mut wb = wb();
        wb.sheet_mut().hidden_rows.insert(2);
        let calc = Calc::new(&wb, 0.0);
        assert!(wb.sort_by(0, &[(0, true), (1, false)], &calc));
        let rows: Vec<(String, String)> = (1..=5).map(|r| (wb.sheet().raw(r, 0).to_string(), wb.sheet().raw(r, 1).to_string())).collect();
        assert_eq!(
            rows,
            [("Hull", "2"), ("Leeds", "3"), ("Leeds", "1"), ("York", "1"), ("York", "")].map(|(a, b)| (a.to_string(), b.to_string()))
        );
        // York 1 was hidden at row 3; it's now row 4.
        assert_eq!(wb.sheet().hidden_rows.iter().copied().collect::<Vec<_>>(), [4]);
        assert_eq!(wb.sheet().raw(0, 0), "City");
    }

    #[test]
    fn filters_by_picked_values() {
        let mut wb = wb();
        let calc = Calc::new(&wb, 0.0);
        assert_eq!(wb.column_values(0, 0, &calc).0, ["Hull", "Leeds", "York"]);
        assert_eq!(wb.column_values(0, 1, &calc).0, ["1", "2", "3", ""]);
        wb.sheet_mut().filter = Some(Filter { col: 0, query: String::new(), values: Some(vec!["York".into(), "Hull".into()]) });
        let calc = Calc::new(&wb, 0.0);
        let view = wb.view_rows(0, &calc);
        assert_eq!(&view[..4], [0, 2, 4, 5]);
        assert_eq!(wb.column_names(0, &calc)[1].1, "B · Sales");
    }
}
