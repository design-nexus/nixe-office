//! Dropdown lists in cells: a cell (or block of cells) whose value is picked
//! from a list, typed in the dialog or taken from cells. Picking shows on the
//! selected cell as an arrow; typing something else is refused.

use super::Sheets;
use super::model::*;
use crate::{widgets, window};
use gtk::gdk;
use gtk::prelude::*;
use serde::{Deserialize, Serialize};
use std::rc::Rc;

/// Choices for the cells in `range`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListRule {
    /// "B2:B40".
    pub range: String,
    /// The choices, as typed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
    /// Or the cells holding them: "D1:D5", "'Sheet 2'!A1:A9".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// The most choices shown, from cells.
const MAX_CHOICES: usize = 500;

/// "'Sheet 2'!A1:A9" → (Some("Sheet 2"), "A1:A9").
pub fn split_source(s: &str) -> (Option<String>, &str) {
    let s = s.trim().trim_start_matches('=');
    match s.rsplit_once('!') {
        Some((sheet, range)) => {
            let name = sheet.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')).map_or(sheet.to_string(), |x| x.replace("''", "'"));
            (Some(name), range)
        }
        None => (None, s),
    }
}

impl Workbook {
    /// The list rule for a cell of sheet `si`.
    pub fn list_at(&self, si: usize, r: u32, c: u32) -> Option<&ListRule> {
        self.sheets.get(si)?.lists.iter().rev().find(|l| Area::parse(&l.range).is_some_and(|a| a.contains(r, c)))
    }

    /// A rule's choices, from cells worked out by `calc` when it takes them from cells.
    pub fn choices(&self, si: usize, rule: &ListRule, calc: &Calc) -> Vec<String> {
        let Some(src) = &rule.source else { return rule.items.clone() };
        let (sheet, range) = split_source(src);
        let sj = match sheet {
            Some(name) => match self.sheets.iter().position(|s| s.name.eq_ignore_ascii_case(&name)) {
                Some(j) => j,
                None => return Vec::new(),
            },
            None => si,
        };
        let Some(a) = Area::parse(range) else { return Vec::new() };
        let mut out = Vec::new();
        for (r, c) in a.iter() {
            let v = calc.value(self, sj, r, c);
            let t = v.text();
            if !t.trim().is_empty() && !out.contains(&t) {
                out.push(t);
            }
            if out.len() >= MAX_CHOICES {
                break;
            }
        }
        out
    }

    /// Give the cells in `area` of the active sheet a list (None takes it away).
    pub fn set_list(&mut self, area: Area, rule: Option<ListRule>) {
        let s = self.sheet_mut();
        s.lists.retain(|l| !Area::parse(&l.range).is_some_and(|a| a.overlaps(&area)));
        if let Some(r) = rule {
            s.lists.push(ListRule { range: area.name(), ..r });
        }
    }
}

impl Sheets {
    /// Where the arrow is drawn on a cell: x, y, size.
    fn arrow_rect(&self, r: u32, c: u32) -> (f64, f64, f64) {
        let (x, y, w, h) = self.area_rect(Area::cell(r, c));
        let size = h.clamp(16.0, 22.0);
        (x + w - size - 2.0, y + (h - size) / 2.0, size)
    }

    /// The selected cell's list, if it has one (and no filter turns rows around).
    fn cursor_list(&self) -> Option<ListRule> {
        let (r, c) = self.cursor.get();
        let wb = self.wb.borrow();
        wb.list_at(wb.active, self.src_row(r), c).cloned()
    }

    /// Draw the arrow on the selected cell when it has a list.
    pub(super) fn draw_list_arrow(&self, cr: &cairo::Context, accent: (f64, f64, f64)) {
        if self.editing.get() || self.cursor_list().is_none() {
            return;
        }
        let (r, c) = self.cursor.get();
        let (x, y, size) = self.arrow_rect(r, c);
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.rectangle(x, y, size, size);
        cr.fill().ok();
        cr.set_source_rgb(accent.0, accent.1, accent.2);
        cr.set_line_width(1.0);
        cr.rectangle(x + 0.5, y + 0.5, size - 1.0, size - 1.0);
        cr.stroke().ok();
        let (cx, cy) = (x + size / 2.0, y + size / 2.0 + 1.0);
        cr.move_to(cx - 4.0, cy - 2.0);
        cr.line_to(cx + 4.0, cy - 2.0);
        cr.line_to(cx, cy + 3.0);
        cr.close_path();
        cr.fill().ok();
    }

    /// True when (x, y) is on the selected cell's arrow.
    pub(super) fn on_list_arrow(&self, x: f64, y: f64) -> bool {
        if self.editing.get() || self.cursor_list().is_none() {
            return false;
        }
        let (r, c) = self.cursor.get();
        let (ax, ay, size) = self.arrow_rect(r, c);
        x >= ax && x <= ax + size && y >= ay && y <= ay + size
    }

    /// The selected cell's choices, under it (Alt+Down, or a click on the arrow).
    pub(super) fn show_choices(self: &Rc<Self>) {
        let Some(rule) = self.cursor_list() else { return };
        let choices = {
            let wb = self.wb.borrow();
            wb.choices(wb.active, &rule, &self.calc.borrow())
        };
        let (r, c) = self.cursor.get();
        let (x, y, w, h) = self.area_rect(Area::cell(r, c));
        let pop = gtk::Popover::new();
        pop.add_css_class("menu-popover");
        pop.set_has_arrow(false);
        pop.set_position(gtk::PositionType::Bottom);
        pop.set_parent(&self.area);
        pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, w as i32, h as i32)));
        let list = widgets::vbox(0);
        let current = self.wb.borrow().sheet().raw(self.src_row(r), c).to_string();
        let mut entries: Vec<(String, String)> = choices.iter().map(|t| (t.clone(), t.clone())).collect();
        entries.push(("Clear".into(), String::new()));
        let mut first: Option<gtk::Button> = None;
        for (label, value) in entries {
            let b = gtk::Button::with_label(&label);
            b.add_css_class("menu-item");
            if value.is_empty() {
                b.add_css_class("dim");
            }
            if !value.is_empty() && value == current {
                b.add_css_class("suggested-action");
            }
            if let Some(l) = b.child().and_downcast::<gtk::Label>() {
                l.set_xalign(0.0);
            }
            let (me, p) = (self.weak(), pop.clone());
            b.connect_clicked(move |_| {
                p.popdown();
                if let Some(s) = me.upgrade() {
                    s.commit_raw(r, c, &value);
                    s.area.grab_focus();
                }
            });
            first.get_or_insert(b.clone());
            list.append(&b);
        }
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(320)
            .child(&list)
            .build();
        pop.set_child(Some(&scroll));
        pop.connect_closed(|p| p.unparent());
        pop.popup();
        if let Some(b) = first {
            b.grab_focus();
        }
    }

    /// What may go in a cell with a list: the matching choice as written, or
    /// None (with a note) when it isn't one. Cells without a list take anything.
    pub(super) fn checked_entry(&self, src: u32, c: u32, raw: &str) -> Option<String> {
        let wb = self.wb.borrow();
        let Some(rule) = wb.list_at(wb.active, src, c) else { return Some(raw.to_string()) };
        if raw.trim().is_empty() {
            return Some(String::new());
        }
        let choices = wb.choices(wb.active, rule, &self.calc.borrow());
        match choices.iter().find(|x| x.trim().eq_ignore_ascii_case(raw.trim())) {
            Some(x) => Some(x.clone()),
            None => {
                window::toast("That isn't one of the cell's choices. Pick one from its list (Alt+Down).");
                None
            }
        }
    }

    /// Set up the selected cells' list, or take it away.
    pub(super) fn list_dialog(self: &Rc<Self>) {
        self.end_edit(true);
        let area = self.selection_src();
        let existing = {
            let wb = self.wb.borrow();
            wb.list_at(wb.active, area.top, area.left).cloned()
        };
        let (dialog, card) = widgets::dialog(&format!("Dropdown list · {}", area.name()), 420);
        let note = widgets::label("Cells with a list take one of its choices: pick from the arrow, or type one.", "dim");
        note.set_wrap(true);
        note.set_xalign(0.0);
        card.append(&note);

        card.append(&widgets::label("Choices, one to a line", "dim"));
        let text = gtk::TextView::new();
        text.add_css_class("list-choices");
        text.set_wrap_mode(gtk::WrapMode::WordChar);
        text.set_top_margin(6);
        text.set_left_margin(8);
        text.buffer().set_text(&existing.as_ref().map(|l| l.items.join("\n")).unwrap_or_default());
        let scroll = gtk::ScrolledWindow::builder().min_content_height(120).child(&text).build();
        scroll.add_css_class("frame");
        card.append(&scroll);

        card.append(&widgets::label("Or take them from cells", "dim"));
        let source = gtk::Entry::new();
        source.set_placeholder_text(Some("D1:D8, or 'Sheet 2'!A1:A20"));
        source.set_text(existing.as_ref().and_then(|l| l.source.as_deref()).unwrap_or(""));
        card.append(&source);

        let err = widgets::label("", "danger-text");
        err.set_visible(false);
        card.append(&err);
        let buttons = widgets::hbox(8);
        buttons.set_margin_top(6);
        let remove = gtk::Button::with_label("Remove list");
        remove.add_css_class("destructive-action");
        remove.set_visible(existing.is_some());
        let spacer = widgets::hbox(0);
        spacer.set_hexpand(true);
        let cancel = gtk::Button::with_label("Cancel");
        let ok = gtk::Button::with_label("Apply");
        ok.add_css_class("suggested-action");
        buttons.append(&remove);
        buttons.append(&spacer);
        buttons.append(&cancel);
        buttons.append(&ok);
        card.append(&buttons);

        let d = dialog.clone();
        cancel.connect_clicked(move |_| d.close());
        let (d, me) = (dialog.clone(), self.weak());
        remove.connect_clicked(move |_| {
            d.close();
            if let Some(s) = me.upgrade() {
                s.change(|wb| wb.set_list(area, None));
            }
        });
        let (d, me) = (dialog.clone(), self.weak());
        ok.connect_clicked(move |_| {
            let Some(s) = me.upgrade() else { return };
            let b = text.buffer();
            let typed = b.text(&b.start_iter(), &b.end_iter(), false);
            let items: Vec<String> = typed.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
            let src = source.text().trim().to_string();
            let rule = if !src.is_empty() {
                if Area::parse(split_source(&src).1).is_none() {
                    err.set_text("That isn't a block of cells, like D1:D8.");
                    err.set_visible(true);
                    return;
                }
                ListRule { range: String::new(), items: Vec::new(), source: Some(src.trim_start_matches('=').to_string()) }
            } else if !items.is_empty() {
                ListRule { range: String::new(), items, source: None }
            } else {
                err.set_text("Type some choices, or the cells that hold them.");
                err.set_visible(true);
                return;
            };
            d.close();
            s.change(|wb| wb.set_list(area, Some(rule)));
            s.area.grab_focus();
        });
        dialog.present();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_from_items_and_cells() {
        let mut wb = Workbook::default();
        wb.set(0, 3, "Paid");
        wb.set(1, 3, "Unpaid");
        wb.set(2, 3, "Paid");
        wb.set_list(Area::new(1, 1, 9, 1), Some(ListRule { range: String::new(), items: Vec::new(), source: Some("D1:D5".into()) }));
        wb.set_list(Area::cell(0, 0), Some(ListRule { range: String::new(), items: vec!["Yes".into(), "No".into()], source: None }));
        let calc = Calc::new(&wb, 0.0);
        let rule = wb.list_at(0, 4, 1).cloned().expect("a list");
        assert_eq!(wb.choices(0, &rule, &calc), ["Paid", "Unpaid"]);
        assert_eq!(wb.list_at(0, 0, 0).map(|l| l.items.len()), Some(2));
        assert!(wb.list_at(0, 10, 1).is_none());
        // A new list over part of an old one replaces it.
        wb.set_list(Area::cell(3, 1), None);
        assert!(wb.list_at(0, 4, 1).is_none());
        assert_eq!(split_source("'My sheet'!A1:A3"), (Some("My sheet".into()), "A1:A3"));
    }
}
