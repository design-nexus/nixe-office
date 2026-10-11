//! Characters that aren't on the keyboard: accented letters, currency, maths,
//! punctuation and arrows, from a menu on the toolbar.

use super::Writer;
use crate::widgets;
use gtk::prelude::*;
use std::rc::Rc;

/// (group, characters).
const GROUPS: [(&str, &str); 6] = [
    ("Accented letters", "áàâäãåæçéèêëíìîïñóòôöõøœúùûüýÿßÁÀÂÄÃÅÆÇÉÈÊËÍÌÎÏÑÓÒÔÖÕØŒÚÙÛÜÝ"),
    ("Punctuation", "–—…‘’“”«»‹›¡¿•·§¶†‡′″"),
    ("Money", "€£¥¢₹₽₩₪₺₿¤"),
    ("Maths", "±×÷≠≈≤≥∞√∑∏∫∂∆πµΩ°‰½⅓⅔¼¾¹²³"),
    ("Symbols", "©®™℠№✓✗★☆♥♦♣♠☐☑☎✉☀☁☂❄♪"),
    ("Arrows", "←→↑↓↔↕⇐⇒⇑⇓⇔↩↪"),
];

impl Writer {
    pub(super) fn symbols_button(self: &Rc<Self>) -> gtk::MenuButton {
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("nxo-omega-symbolic");
        menu.add_css_class("bar-button");
        menu.set_tooltip_text(Some("Insert a special character"));
        menu.set_valign(gtk::Align::Center);
        let pop = gtk::Popover::new();
        let content = widgets::vbox(6);
        content.add_css_class("prompt-box");
        for (name, chars) in GROUPS {
            content.append(&widgets::label(name, "dim"));
            let grid = gtk::FlowBox::new();
            grid.set_selection_mode(gtk::SelectionMode::None);
            grid.set_max_children_per_line(12);
            grid.set_min_children_per_line(12);
            grid.set_homogeneous(true);
            for ch in chars.chars() {
                let b = gtk::Button::with_label(&ch.to_string());
                b.add_css_class("flat");
                b.add_css_class("symbol-key");
                b.set_tooltip_text(Some(&format!("U+{:04X}", ch as u32)));
                let (me, p) = (Rc::downgrade(self), pop.clone());
                b.connect_clicked(move |_| {
                    p.popdown();
                    if let Some(w) = me.upgrade() {
                        w.insert_symbol(ch);
                    }
                });
                grid.append(&b);
            }
            content.append(&grid);
        }
        pop.set_child(Some(&content));
        menu.set_popover(Some(&pop));
        menu
    }

    /// Type `ch` at the cursor, in the formatting there, over any selection.
    fn insert_symbol(&self, ch: char) {
        let b = self.tb();
        b.delete_selection(true, true);
        b.insert_interactive_at_cursor(&ch.to_string(), true);
        self.view.grab_focus();
    }
}
