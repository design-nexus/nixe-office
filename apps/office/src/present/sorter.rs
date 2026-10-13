//! The slide sorter: every slide in a grid, to look over and rearrange a long deck.

use super::render::{self, Hints};
use super::{Present, ctx};
use crate::widgets;
use gtk::prelude::*;
use gtk::{gdk, glib};

impl Present {
    /// Sets up the grid's own handlers (once, while building).
    pub(super) fn connect_sorter(&self) {
        let grid = &self.sorter;
        grid.set_selection_mode(gtk::SelectionMode::Single);
        grid.set_activate_on_single_click(false);
        grid.set_homogeneous(true);
        grid.set_max_children_per_line(12);
        grid.set_valign(gtk::Align::Start);
        grid.add_css_class("slide-sorter");
        let me = self.weak();
        grid.connect_selected_children_changed(move |g| {
            if let Some(p) = me.upgrade()
                && !p.syncing.get()
                && let Some(c) = g.selected_children().first()
            {
                p.go_to(c.index() as usize);
            }
        });
        // Double-click or Enter opens the slide.
        let me = self.weak();
        grid.connect_child_activated(move |_, c| {
            if let Some(p) = me.upgrade() {
                p.go_to(c.index() as usize);
                p.show_sorter(false);
            }
        });
        let keys = gtk::EventControllerKey::new();
        let me = self.weak();
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(p) = me.upgrade() else { return glib::Propagation::Proceed };
            if matches!(key, gdk::Key::Delete | gdk::Key::BackSpace) && p.deck.borrow().slides.len() > 1 {
                let i = p.current.get();
                p.change_deck(|d| {
                    d.remove(i);
                });
                p.go_to(i.min(p.deck.borrow().slides.len() - 1));
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        grid.add_controller(keys);
    }

    /// Switches between editing one slide and the sorter.
    pub(super) fn show_sorter(&self, on: bool) {
        self.end_edit();
        if self.sorter_button.is_active() != on {
            self.syncing.set(true);
            self.sorter_button.set_active(on);
            self.syncing.set(false);
        }
        self.views.set_visible_child_name(if on { "sorter" } else { "slide" });
        if on {
            self.refresh_sorter();
            if let Some(c) = self.sorter.child_at_index(self.current.get() as i32) {
                c.grab_focus();
            }
        } else {
            self.canvas.grab_focus();
        }
    }

    fn sorter_showing(&self) -> bool {
        self.views.visible_child_name().as_deref() == Some("sorter")
    }

    /// Brings the grid up to date with the deck (only while it shows).
    pub(super) fn refresh_sorter(&self) {
        if !self.sorter_showing() {
            return;
        }
        let n = self.deck.borrow().slides.len();
        let mut have = 0;
        while self.sorter.child_at_index(have).is_some() {
            have += 1;
        }
        if have as usize != n {
            self.syncing.set(true);
            self.sorter.remove_all();
            for i in 0..n {
                self.sorter.append(&self.sorter_tile(i));
            }
            self.syncing.set(false);
        } else {
            for i in 0..n {
                if let Some(area) = self.sorter.child_at_index(i as i32).and_then(|c| c.child()).and_then(|b| b.first_child()) {
                    area.queue_draw();
                }
            }
        }
        self.mark_sorter();
    }

    /// Selects the current slide in the grid.
    pub(super) fn mark_sorter(&self) {
        if !self.sorter_showing() {
            return;
        }
        if let Some(c) = self.sorter.child_at_index(self.current.get() as i32)
            && !c.is_selected()
        {
            self.syncing.set(true);
            self.sorter.select_child(&c);
            self.syncing.set(false);
        }
    }

    fn sorter_tile(&self, i: usize) -> gtk::FlowBoxChild {
        let tile = widgets::vbox(4);
        tile.add_css_class("sorter-tile");
        let area = gtk::DrawingArea::new();
        area.set_content_width(240);
        area.set_content_height(135);
        let me = self.weak();
        area.set_draw_func(move |_, cr, w, h| {
            if let Some(p) = me.upgrade() {
                let d = p.deck.borrow();
                if let Some(s) = d.composed(i) {
                    render::draw(cr, &ctx(), &s, &p.media.borrow(), &p.pictures, (w as f64, h as f64), Hints::default());
                }
            }
        });
        tile.append(&area);
        tile.append(&widgets::label(&(i + 1).to_string(), "thumb-number"));
        let child = gtk::FlowBoxChild::new();
        child.set_child(Some(&tile));

        // Drag a slide onto another to put it there.
        let source = gtk::DragSource::new();
        source.set_actions(gdk::DragAction::MOVE);
        source.connect_prepare(move |_, _, _| Some(gdk::ContentProvider::for_value(&(i as u32).to_value())));
        let a = area.clone();
        source.connect_drag_begin(move |s, _| {
            let paintable = gtk::WidgetPaintable::new(Some(&a));
            s.set_icon(Some(&paintable), 24, 24);
        });
        child.add_controller(source);
        let target = gtk::DropTarget::new(u32::static_type(), gdk::DragAction::MOVE);
        let me = self.weak();
        target.connect_drop(move |_, value, _, _| {
            let (Some(p), Ok(from)) = (me.upgrade(), value.get::<u32>()) else { return false };
            let mut to = None;
            p.change_deck(|d| to = d.move_to(from as usize, i));
            if let Some(to) = to {
                p.go_to(to);
            }
            to.is_some()
        });
        child.add_controller(target);

        let click = gtk::GestureClick::new();
        click.set_button(3);
        let me = self.weak();
        let c = child.clone();
        click.connect_pressed(move |_, _, _, _| {
            if let Some(p) = me.upgrade() {
                p.go_to(i);
                p.slide_menu(i, c.upcast_ref());
            }
        });
        child.add_controller(click);
        child
    }
}
