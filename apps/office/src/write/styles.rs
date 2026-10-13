//! Changing how a paragraph style looks: make it look like the text at the
//! cursor, or put it back as it was. Every paragraph of that style follows.

use super::Writer;
use super::buffer;
use super::model::*;
use crate::widgets;
use gtk::prelude::*;
use std::rc::Rc;

impl Writer {
    pub(super) fn style_menu(self: &Rc<Self>) -> gtk::MenuButton {
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("nxo-pen-line-symbolic");
        menu.add_css_class("bar-button");
        menu.set_tooltip_text(Some("Change how this paragraph style looks"));
        menu.set_valign(gtk::Align::Center);
        let pop = gtk::Popover::new();
        pop.add_css_class("menu-popover");
        let list = widgets::vbox(0);
        pop.set_child(Some(&list));
        let me = Rc::downgrade(self);
        // Built as it opens, naming the style the cursor is in.
        pop.connect_show(move |pop| {
            let Some(w) = me.upgrade() else { return };
            while let Some(c) = list.first_child() {
                list.remove(&c);
            }
            let style = buffer::para_props(w.tb(), &w.tb().iter_at_mark(&w.tb().get_insert())).style;
            if style == Style::Normal {
                let note = widgets::label("Put the cursor in a title, heading or quote to change how that style looks.", "dim");
                note.set_wrap(true);
                note.set_max_width_chars(36);
                note.set_margin_start(10);
                note.set_margin_end(10);
                note.set_margin_top(8);
                note.set_margin_bottom(8);
                list.append(&note);
                return;
            }
            let changed = w.styles.borrow().contains_key(style.id());
            let item = |label: String, f: Box<dyn Fn(&Writer)>| {
                let b = gtk::Button::with_label(&label);
                b.add_css_class("menu-item");
                if let Some(l) = b.child().and_downcast::<gtk::Label>() {
                    l.set_xalign(0.0);
                }
                let (me, p) = (w.me.borrow().clone(), pop.clone());
                b.connect_clicked(move |_| {
                    p.popdown();
                    if let Some(w) = me.upgrade() {
                        f(&w);
                    }
                });
                list.append(&b);
            };
            item(format!("Make every {} look like this text", style.name()), Box::new(move |w| w.match_style(style)));
            if changed {
                item(format!("Put {} back as it was", style.name()), Box::new(move |w| w.set_style_look(style, None)));
            }
        });
        menu.set_popover(Some(&pop));
        menu
    }

    /// The style takes the look of the text at the cursor: its font, size, colour,
    /// bold and italic (what the text doesn't set keeps the style's).
    pub(super) fn match_style(&self, style: Style) {
        let b = self.tb();
        let at = b.selection_bounds().map(|(s, _)| s).unwrap_or_else(|| b.iter_at_mark(&b.get_insert()));
        let r = buffer::char_format(&at);
        let now = look_of(&self.styles.borrow(), style);
        let look = StyleLook {
            font: Some(r.font.unwrap_or(now.font)),
            size: Some(r.size.unwrap_or(now.size)),
            color: r.color.or(now.color),
            bold: Some(r.bold || now.bold),
            italic: Some(r.italic || now.italic),
        };
        self.set_style_look(style, Some(look));
    }

    /// Change (or with None, undo the changes to) a style's look, as one undo step.
    pub(super) fn set_style_look(&self, style: Style, look: Option<StyleLook>) {
        self.checkpoint();
        {
            let mut styles = self.styles.borrow_mut();
            match look {
                Some(l) => styles.insert(style.id().to_string(), l),
                None => styles.remove(style.id()),
            };
        }
        buffer::apply_looks(self.tb(), &self.styles.borrow());
        self.edited();
        self.sync_controls();
        self.view.grab_focus();
    }
}
