//! Links in the text (`link:<address>` tags), and superscript and subscript.
//! Ctrl+click opens a link; resting the pointer on one shows where it goes.

use super::Writer;
use super::buffer;
use super::model::TextRun;
use crate::{widgets, window};
use gtk::prelude::*;
use gtk::{gdk, gio};
use std::rc::Rc;

/// What was typed as an address, made into one: `example.com` → `https://example.com`,
/// `me@example.com` → `mailto:me@example.com`. None when it's empty.
pub fn normalize(address: &str) -> Option<String> {
    let a = address.trim();
    if a.is_empty() {
        return None;
    }
    let lower = a.to_ascii_lowercase();
    let has_scheme = a.contains("://") || ["mailto:", "tel:", "file:"].iter().any(|s| lower.starts_with(s));
    Some(if has_scheme {
        a.to_string()
    } else if a.starts_with('/') {
        format!("file://{a}")
    } else if a.contains('@') && !a.contains('/') {
        format!("mailto:{a}")
    } else {
        format!("https://{a}")
    })
}

impl Writer {
    /// The link the character after `it` is part of: its whole stretch and address.
    pub(super) fn link_at(&self, it: &gtk::TextIter) -> Option<(gtk::TextIter, gtk::TextIter, String)> {
        let uri = buffer::char_format(it).link?;
        let tag = self.tb().tag_table().lookup(&format!("link:{uri}"))?;
        let (mut s, mut e) = (*it, *it);
        if !s.starts_tag(Some(&tag)) {
            s.backward_to_tag_toggle(Some(&tag));
        }
        e.forward_to_tag_toggle(Some(&tag));
        Some((s, e, uri))
    }

    /// The link under the cursor, or just before it.
    fn link_near_cursor(&self) -> Option<(gtk::TextIter, gtk::TextIter, String)> {
        let it = self.cursor_iter();
        self.link_at(&it).or_else(|| {
            let mut before = it;
            (before.backward_char()).then(|| self.link_at(&before)).flatten()
        })
    }

    /// Set (or with None, remove) the link on characters `so..eo`.
    pub(super) fn set_link(&self, so: i32, eo: i32, uri: Option<String>) {
        self.apply_char_range(so, eo, &move |r: &mut TextRun| r.link = uri.clone());
    }

    /// Ctrl+K: add a link to the selection, change or remove the one at the
    /// cursor, or type new linked text.
    pub(super) fn link_dialog(&self) {
        let b = self.tb();
        let existing = self.link_near_cursor();
        let range = match (&existing, b.selection_bounds()) {
            (_, Some((s, e))) => Some((s.offset(), e.offset())),
            (Some((s, e, _)), None) => Some((s.offset(), e.offset())),
            (None, None) => None,
        };
        let current = existing.as_ref().map(|x| x.2.clone()).unwrap_or_default();
        let title = if existing.is_some() { "Edit link" } else { "Add a link" };
        let (dialog, card) = widgets::dialog(title, 440);

        let text = gtk::Entry::new();
        text.set_placeholder_text(Some("Text to show"));
        if range.is_none() {
            card.append(&widgets::label("Text", "dim"));
            card.append(&text);
        }
        card.append(&widgets::label("Address", "dim"));
        let address = gtk::Entry::new();
        address.set_placeholder_text(Some("https://example.com, or an email address"));
        address.set_text(&current);
        card.append(&address);

        let buttons = widgets::hbox(8);
        buttons.set_margin_top(6);
        let remove = gtk::Button::with_label("Remove link");
        remove.add_css_class("destructive-action");
        remove.set_visible(existing.is_some());
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        let cancel = gtk::Button::with_label("Cancel");
        let ok = gtk::Button::with_label(if existing.is_some() { "Change" } else { "Add" });
        ok.add_css_class("suggested-action");
        buttons.append(&remove);
        buttons.append(&spacer);
        buttons.append(&cancel);
        buttons.append(&ok);
        card.append(&buttons);

        let d = dialog.clone();
        cancel.connect_clicked(move |_| d.close());
        let (d, me) = (dialog.clone(), self.rc());
        remove.connect_clicked(move |_| {
            d.close();
            if let Some((so, eo)) = range {
                me.set_link(so, eo, None);
            }
        });
        let (d, me, t, a) = (dialog.clone(), self.rc(), text.clone(), address.clone());
        let go = Rc::new(move || {
            let Some(uri) = normalize(&a.text()) else {
                a.grab_focus();
                return;
            };
            d.close();
            match range {
                Some((so, eo)) => me.set_link(so, eo, Some(uri)),
                None => {
                    let shown = t.text();
                    let shown = if shown.trim().is_empty() { uri.clone() } else { shown.to_string() };
                    me.insert_linked(&shown, uri);
                }
            }
        });
        let g = go.clone();
        ok.connect_clicked(move |_| g());
        let g = go.clone();
        address.connect_activate(move |_| g());
        text.connect_activate(move |_| go());
        dialog.present();
        if range.is_none() {
            text.grab_focus();
        } else {
            address.grab_focus();
            address.select_region(0, -1);
        }
    }

    /// Type `shown` at the cursor as a link to `uri`; what's typed next isn't linked.
    fn insert_linked(&self, shown: &str, uri: String) {
        let b = self.tb();
        self.checkpoint();
        let so = self.cursor_iter().offset();
        b.insert_at_cursor(shown);
        let eo = self.cursor_iter().offset();
        self.set_link(so, eo, Some(uri));
        self.view.grab_focus();
    }

    /// Superscript or subscript on or off; turning one on turns the other off.
    pub(super) fn toggle_script(&self, sup: bool) {
        let on = !self.all_have(&|r| if sup { r.superscript } else { r.subscript });
        self.apply_char(move |r| {
            if sup {
                r.superscript = on;
            } else {
                r.subscript = on;
            }
            if on {
                r.superscript = sup;
                r.subscript = !sup;
            }
        });
    }

    /// Open the link under the pointer on Ctrl+click; show its address on hover.
    pub(super) fn connect_links(self: &Rc<Self>) {
        let click = gtk::GestureClick::new();
        click.set_button(gdk::BUTTON_PRIMARY);
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let me = Rc::downgrade(self);
        click.connect_pressed(move |g, _, x, y| {
            let Some(w) = me.upgrade() else { return };
            if !g.current_event_state().contains(gdk::ModifierType::CONTROL_MASK) {
                return;
            }
            if let Some(uri) = w.link_under(x, y) {
                g.set_state(gtk::EventSequenceState::Claimed);
                open(&uri);
            }
        });
        self.view.add_controller(click);

        self.view.set_has_tooltip(true);
        let me = Rc::downgrade(self);
        self.view.connect_query_tooltip(move |_, x, y, keyboard, tip| {
            let Some(w) = me.upgrade() else { return false };
            if keyboard {
                return false;
            }
            let Some(uri) = w.link_under(x as f64, y as f64) else { return false };
            tip.set_text(Some(&format!("{uri}\nCtrl+click to open")));
            true
        });
    }

    fn link_under(&self, x: f64, y: f64) -> Option<String> {
        let (bx, by) = self.view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
        let it = self.view.iter_at_location(bx, by)?;
        buffer::char_format(&it).link
    }
}

/// Open an address in its default app (the browser, the mail app, a file's app).
pub fn open(uri: &str) {
    let launcher = gtk::UriLauncher::new(uri);
    let uri = uri.to_string();
    launcher.launch(window::window().as_ref(), gio::Cancellable::NONE, move |res| {
        if res.is_err() {
            window::toast(&format!("Couldn't open {uri}."));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn addresses_are_completed() {
        assert_eq!(normalize("example.com").as_deref(), Some("https://example.com"));
        assert_eq!(normalize(" https://a.b/c ").as_deref(), Some("https://a.b/c"));
        assert_eq!(normalize("me@example.com").as_deref(), Some("mailto:me@example.com"));
        assert_eq!(normalize("mailto:me@example.com").as_deref(), Some("mailto:me@example.com"));
        assert_eq!(normalize("/home/me/notes.pdf").as_deref(), Some("file:///home/me/notes.pdf"));
        assert_eq!(normalize("localhost:8080").as_deref(), Some("https://localhost:8080"));
        assert_eq!(normalize("  "), None);
    }
}
