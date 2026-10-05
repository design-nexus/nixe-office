//! Line and paragraph spacing: the toolbar's spacing menu.

use super::Writer;
use super::buffer::{self, ParaProps};
use super::model::LINE_SPACINGS;
use crate::widgets;
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

impl Writer {
    pub(super) fn spacing_button(self: &Rc<Self>) -> gtk::MenuButton {
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("nxo-unfold-vertical-symbolic");
        menu.add_css_class("bar-button");
        menu.set_tooltip_text(Some("Line and paragraph spacing"));
        menu.set_valign(gtk::Align::Center);
        let pop = gtk::Popover::new();
        let content = widgets::vbox(10);
        content.add_css_class("prompt-box");
        content.append(&widgets::label("Spacing", "popover-title"));

        // Line spacing: one of the usual multiples.
        content.append(&widgets::label("Line spacing", "dim"));
        let lines = widgets::hbox(4);
        let mut buttons: Vec<(f64, gtk::ToggleButton)> = Vec::new();
        for l in LINE_SPACINGS {
            let b = gtk::ToggleButton::with_label(&format!("{l}"));
            if let Some((_, first)) = buttons.first() {
                b.set_group(Some(first));
            }
            lines.append(&b);
            buttons.push((l, b));
        }
        content.append(&lines);

        // Space above and below the paragraph, in points.
        let before = gtk::SpinButton::with_range(0.0, 96.0, 1.0);
        let after = gtk::SpinButton::with_range(0.0, 96.0, 1.0);
        for (label, spin) in [("Space before (pt)", &before), ("Space after (pt)", &after)] {
            let row = widgets::hbox(8);
            let l = widgets::label(label, "dim");
            l.set_hexpand(true);
            l.set_xalign(0.0);
            row.append(&l);
            row.append(spin);
            content.append(&row);
        }
        let reset = gtk::Button::with_label("Use the style's spacing");
        reset.set_halign(gtk::Align::End);
        content.append(&reset);
        pop.set_child(Some(&content));
        menu.set_popover(Some(&pop));

        // Show the paragraph's spacing when the menu opens; changes apply as they're made.
        let syncing = Rc::new(Cell::new(false));
        let me = Rc::downgrade(self);
        let (bs, b, a, sy) = (buttons.clone(), before.clone(), after.clone(), syncing.clone());
        pop.connect_show(move |_| {
            let Some(w) = me.upgrade() else { return };
            let p = buffer::para_props(w.tb(), &w.cursor_iter());
            let (above, below) = p.style.spacing();
            sy.set(true);
            let line = p.line.unwrap_or(1.0);
            for (l, btn) in &bs {
                btn.set_active((l - line).abs() < 0.001);
            }
            b.set_value(p.before.unwrap_or(above));
            a.set_value(p.after.unwrap_or(below));
            sy.set(false);
        });
        for (l, btn) in &buttons {
            let (me, sy, l) = (Rc::downgrade(self), syncing.clone(), *l);
            btn.connect_toggled(move |btn| {
                if sy.get() || !btn.is_active() {
                    return;
                }
                if let Some(w) = me.upgrade() {
                    w.change_spacing(move |p| p.line = ((l - 1.0).abs() > 0.001).then_some(l));
                }
            });
        }
        for (spin, is_before) in [(&before, true), (&after, false)] {
            let (me, sy) = (Rc::downgrade(self), syncing.clone());
            spin.connect_value_changed(move |s| {
                if sy.get() {
                    return;
                }
                let v = s.value();
                if let Some(w) = me.upgrade() {
                    w.change_spacing(move |p| {
                        // The style's own spacing is stored as none.
                        let (above, below) = p.style.spacing();
                        let own = if is_before { above } else { below };
                        let set = ((v - own).abs() > 0.01).then_some(v);
                        if is_before { p.before = set } else { p.after = set }
                    });
                }
            });
        }
        let me = Rc::downgrade(self);
        let p = pop.clone();
        reset.connect_clicked(move |_| {
            p.popdown();
            if let Some(w) = me.upgrade() {
                w.change_spacing(|p| {
                    p.line = None;
                    p.before = None;
                    p.after = None;
                });
            }
        });
        menu
    }

    /// Change the spacing of the selected paragraphs, keeping the menu open.
    fn change_spacing(&self, f: impl Fn(&mut ParaProps) + 'static) {
        self.checkpoint();
        let (first, last) = self.selected_lines();
        buffer::update_para(self.tb(), first, last, f);
        self.edited();
    }
}
