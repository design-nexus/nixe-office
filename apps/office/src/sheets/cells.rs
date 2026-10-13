//! Merging cells, cell comments and the highlight-rules dialog.

use super::Sheets;
use super::model::*;
use super::rules::{self, Rule, Test};
use crate::{widgets, window};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Rebuilds the rule list; set once the list's closures can refer back to it.
type Refresh = Rc<RefCell<Option<Box<dyn Fn()>>>>;

impl Sheets {
    /// Merge the selection, or split the merged cells in it.
    pub(super) fn merge_cells(&self) {
        if !self.merges_shown() {
            window::toast("Clear the filter to merge or unmerge cells.");
            return;
        }
        let a = self.selection();
        if self.wb.borrow().sheet().merges.iter().any(|m| m.overlaps(&a)) {
            self.change(|wb| {
                wb.unmerge(a);
            });
            self.area.grab_focus();
            return;
        }
        if a.is_cell() {
            window::toast("Select two or more cells to merge.");
            return;
        }
        let mut lost = false;
        self.change(|wb| lost = wb.merge(a));
        self.anchor.set((a.top, a.left));
        self.cursor.set((a.top, a.left));
        self.area.queue_draw();
        self.area.grab_focus();
        if lost {
            window::toast("Merged. Only the top-left cell's content was kept (Ctrl+Z brings the rest back).");
        }
    }

    /// Add or change the comment on the active cell.
    pub(super) fn edit_comment(&self) {
        self.end_edit(true);
        let (r, c) = self.cursor.get();
        let src = self.src_row(r);
        let current = self.wb.borrow().sheet().comment(src, c).unwrap_or("").to_string();
        let (dialog, card) = widgets::dialog(&format!("Comment on {}", super::formula::cell_name(src, c)), 420);
        let text = gtk::TextView::new();
        text.add_css_class("comment-text");
        text.set_wrap_mode(gtk::WrapMode::WordChar);
        text.buffer().set_text(&current);
        let scroll = gtk::ScrolledWindow::builder()
            .child(&text)
            .min_content_height(110)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        scroll.add_css_class("comment-box");
        card.append(&scroll);
        let buttons = widgets::hbox(8);
        let remove = gtk::Button::with_label("Remove");
        remove.add_css_class("danger-text");
        remove.set_visible(!current.is_empty());
        let spacer = widgets::hbox(0);
        spacer.set_hexpand(true);
        let cancel = gtk::Button::with_label("Cancel");
        let ok = gtk::Button::with_label("Save");
        ok.add_css_class("suggested-action");
        buttons.append(&remove);
        buttons.append(&spacer);
        buttons.append(&cancel);
        buttons.append(&ok);
        card.append(&buttons);
        let d = dialog.clone();
        cancel.connect_clicked(move |_| d.close());
        let save = {
            let (me, d) = (self.weak(), dialog.clone());
            move |t: String| {
                d.close();
                if let Some(s) = me.upgrade() {
                    s.change(|wb| wb.set_comment(src, c, &t));
                    s.area.grab_focus();
                }
            }
        };
        let s2 = save.clone();
        remove.connect_clicked(move |_| s2(String::new()));
        let b = text.buffer();
        ok.connect_clicked(move |_| save(b.text(&b.start_iter(), &b.end_iter(), false).to_string()));
        dialog.present();
        text.grab_focus();
    }

    /// The comment under the pointer, for the grid's tooltip.
    pub(super) fn comment_at(&self, x: f64, y: f64) -> Option<String> {
        let (r, c) = self.cell_at(x, y)?;
        let src = self.src_row(r);
        let wb = self.wb.borrow();
        let s = wb.sheet();
        let (r, c) = if self.merges_shown() { s.merge_at(src, c).map(|m| (m.top, m.left)).unwrap_or((src, c)) } else { (src, c) };
        s.comment(r, c).map(str::to_string)
    }

    /// Highlight rules for this sheet: a list, and a form to add one for the selection.
    pub(super) fn highlight_dialog(&self) {
        self.end_edit(true);
        let (dialog, card) = widgets::dialog("Highlight cells", 460);
        let list = widgets::vbox(6);
        list.add_css_class("rule-list");
        card.append(&list);

        // The form: when, look, range.
        let form = widgets::vbox(8);
        form.add_css_class("rule-form");
        let range = gtk::Entry::new();
        range.set_text(&self.selection_src().name());
        range.set_tooltip_text(Some("The cells the rule colours, like B2:B40"));
        let test = gtk::DropDown::from_strings(&Test::NAMES);
        test.set_hexpand(true);
        let a = gtk::Entry::new();
        let b = gtk::Entry::new();
        a.set_width_chars(8);
        b.set_width_chars(8);
        b.set_placeholder_text(Some("and"));
        let look_names: Vec<&str> = rules::LOOKS.iter().map(|l| l.0).collect();
        let look = gtk::DropDown::from_strings(&look_names);
        let bold = gtk::CheckButton::with_label("Bold");
        let row1 = widgets::hbox(8);
        row1.append(&widgets::label("Cells", "dim"));
        range.set_hexpand(true);
        row1.append(&range);
        let row2 = widgets::hbox(8);
        row2.append(&test);
        row2.append(&a);
        row2.append(&b);
        let row3 = widgets::hbox(8);
        row3.append(&widgets::label("Look", "dim"));
        look.set_hexpand(true);
        row3.append(&look);
        row3.append(&bold);
        form.append(&row1);
        form.append(&row2);
        form.append(&row3);
        let err = widgets::label("", "danger-text");
        err.set_visible(false);
        form.append(&err);
        card.append(&form);
        let show_inputs = {
            let (a, b) = (a.clone(), b.clone());
            move |i: usize| {
                let n = Test::inputs_needed(i);
                a.set_visible(n >= 1);
                b.set_visible(n == 2);
            }
        };
        show_inputs(0);
        let si = show_inputs.clone();
        test.connect_selected_notify(move |d| si(d.selected() as usize));

        let buttons = widgets::hbox(8);
        let spacer = widgets::hbox(0);
        spacer.set_hexpand(true);
        let done = gtk::Button::with_label("Done");
        let add = gtk::Button::with_label("Add rule");
        add.add_css_class("suggested-action");
        buttons.append(&spacer);
        buttons.append(&done);
        buttons.append(&add);
        card.append(&buttons);
        let d = dialog.clone();
        done.connect_clicked(move |_| d.close());

        // Which rule the form is changing, if it was picked from the list.
        let editing: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
        let refresh: Refresh = Rc::new(RefCell::new(None));
        {
            let (me, list, refresh2, editing) = (self.weak(), list.clone(), refresh.clone(), editing.clone());
            let (range, test, a, b, look, bold, add) =
                (range.clone(), test.clone(), a.clone(), b.clone(), look.clone(), bold.clone(), add.clone());
            let show_inputs = show_inputs.clone();
            *refresh.borrow_mut() = Some(Box::new(move || {
                let Some(s) = me.upgrade() else { return };
                while let Some(c) = list.first_child() {
                    list.remove(&c);
                }
                let rules = s.wb.borrow().sheet().rules.clone();
                if rules.is_empty() {
                    list.append(&widgets::label("No rules on this sheet yet.", "dim"));
                }
                for (i, rule) in rules.iter().enumerate() {
                    let row = widgets::hbox(8);
                    row.add_css_class("rule-row");
                    let swatch = gtk::Label::new(Some("Aa"));
                    swatch.add_css_class("rule-swatch");
                    let css = gtk::CssProvider::new();
                    css.load_from_string(&format!(
                        "label {{ background: {}; color: {}; {} }}",
                        rule.fill.as_deref().unwrap_or("#ffffff"),
                        rule.color.as_deref().unwrap_or("#202329"),
                        if rule.bold { "font-weight: bold;" } else { "" }
                    ));
                    #[allow(deprecated)]
                    swatch.style_context().add_provider(&css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
                    row.append(&swatch);
                    let text = widgets::label(&format!("{}: {}", rule.range, rule.test.describe()), "");
                    text.set_hexpand(true);
                    text.set_xalign(0.0);
                    text.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    let pick = gtk::Button::new();
                    pick.add_css_class("flat");
                    pick.set_child(Some(&text));
                    pick.set_hexpand(true);
                    pick.set_tooltip_text(Some("Change this rule"));
                    let (rule2, editing2) = (rule.clone(), editing.clone());
                    let (range, test, a, b, look, bold, add, si) =
                        (range.clone(), test.clone(), a.clone(), b.clone(), look.clone(), bold.clone(), add.clone(), show_inputs.clone());
                    pick.connect_clicked(move |_| {
                        editing2.set(Some(i));
                        range.set_text(&rule2.range);
                        test.set_selected(rule2.test.index() as u32);
                        si(rule2.test.index());
                        let (x, y) = rule2.test.inputs();
                        a.set_text(&x);
                        b.set_text(&y);
                        let li = rules::LOOKS.iter().position(|l| Some(l.1) == rule2.fill.as_deref()).unwrap_or(0);
                        look.set_selected(li as u32);
                        bold.set_active(rule2.bold);
                        add.set_label("Save rule");
                    });
                    row.append(&pick);
                    let del = widgets::bar_button("nxo-trash-2-symbolic", "Delete this rule");
                    let (me, r2, ed) = (s.weak(), refresh2.clone(), editing.clone());
                    del.connect_clicked(move |_| {
                        if let Some(s) = me.upgrade() {
                            s.change(|wb| {
                                if i < wb.sheet().rules.len() {
                                    wb.sheet_mut().rules.remove(i);
                                }
                            });
                        }
                        ed.set(None);
                        if let Some(f) = r2.borrow().as_ref() {
                            f();
                        }
                    });
                    row.append(&del);
                    list.append(&row);
                }
            }));
        }
        if let Some(f) = refresh.borrow().as_ref() {
            f();
        }

        let me = self.weak();
        add.connect_clicked(move |btn| {
            let Some(s) = me.upgrade() else { return };
            let area = Area::parse(&range.text());
            let parsed = Test::from_input(test.selected() as usize, &a.text(), &b.text());
            let (area, t) = match (area, parsed) {
                (None, _) => {
                    err.set_text("Type the cells as a range, like B2:B40.");
                    err.set_visible(true);
                    return;
                }
                (_, Err(m)) => {
                    err.set_text(m);
                    err.set_visible(true);
                    return;
                }
                (Some(area), Ok(t)) => (area, t),
            };
            err.set_visible(false);
            let l = rules::LOOKS[(look.selected() as usize).min(rules::LOOKS.len() - 1)];
            let rule = Rule {
                range: area.name(),
                test: t,
                fill: Some(l.1.to_string()),
                color: Some(l.2.to_string()),
                bold: bold.is_active(),
            };
            let at = editing.take();
            s.change(|wb| {
                let rules = &mut wb.sheet_mut().rules;
                match at {
                    Some(i) if i < rules.len() => rules[i] = rule,
                    _ => rules.push(rule),
                }
            });
            btn.set_label("Add rule");
            if let Some(f) = refresh.borrow().as_ref() {
                f();
            }
        });
        dialog.present();
    }
}
