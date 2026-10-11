//! The format painter: copy the look where the cursor is, then select text to
//! give it the same character formatting, or click in a paragraph to give it the
//! paragraph's style, alignment and spacing. A double-click on the brush keeps it
//! painting until Esc.

use super::Writer;
use super::buffer::{self, ParaProps};
use super::model::TextRun;
use gtk::prelude::*;
use gtk::{gdk, glib};
use std::rc::Rc;

#[derive(Clone)]
pub(super) struct Painter {
    run: TextRun,
    para: ParaProps,
    /// Stays on after painting (a double-click on the brush).
    sticky: bool,
}

/// The look of a run, without what isn't formatting (links, comments, changes).
fn look_of(r: &TextRun) -> TextRun {
    TextRun {
        bold: r.bold,
        italic: r.italic,
        underline: r.underline,
        strike: r.strike,
        superscript: r.superscript,
        subscript: r.subscript,
        font: r.font.clone(),
        size: r.size,
        color: r.color.clone(),
        highlight: r.highlight.clone(),
        ..Default::default()
    }
}

impl Writer {
    pub(super) fn painter_button(self: &Rc<Self>) -> gtk::ToggleButton {
        let b = gtk::ToggleButton::new();
        b.set_icon_name("nxo-paintbrush-symbolic");
        b.add_css_class("tool-toggle");
        b.set_tooltip_text(Some(
            "Format painter: copy this formatting, then select text to give it the same look, or click a paragraph to match its style. Double-click to paint more than once; Esc stops.",
        ));
        // A double-click's second press comes before its second click.
        let double = Rc::new(std::cell::Cell::new(false));
        let presses = gtk::GestureClick::new();
        let d = double.clone();
        presses.connect_pressed(move |_, n, _, _| d.set(n == 2));
        b.add_controller(presses);
        let me = Rc::downgrade(self);
        b.connect_clicked(move |b| {
            let Some(w) = me.upgrade() else { return };
            if w.syncing.get() {
                return;
            }
            if double.replace(false) {
                w.start_painting(true);
            } else if b.is_active() {
                w.start_painting(false);
            } else {
                w.stop_painting();
            }
        });
        // Paint when a click or a drag in the text ends (once the selection is final).
        let release = gtk::GestureClick::new();
        release.set_propagation_phase(gtk::PropagationPhase::Capture);
        let me = Rc::downgrade(self);
        release.connect_released(move |_, _, _, _| {
            let me = me.clone();
            glib::idle_add_local_once(move || {
                if let Some(w) = me.upgrade() {
                    w.paint();
                }
            });
        });
        self.view.add_controller(release);
        *self.painter_button.borrow_mut() = Some(b.clone());
        b
    }

    fn start_painting(&self, sticky: bool) {
        let b = self.tb();
        let at = b.selection_bounds().map(|(s, _)| s).unwrap_or_else(|| b.iter_at_mark(&b.get_insert()));
        let run = look_of(&buffer::char_format(&at));
        let para = buffer::para_props(b, &at);
        *self.painter.borrow_mut() = Some(super::painter::Painter { run, para, sticky });
        self.show_painting(true);
        self.view.grab_focus();
    }

    pub(super) fn stop_painting(&self) {
        self.painter.borrow_mut().take();
        self.show_painting(false);
    }

    pub(super) fn painting(&self) -> bool {
        self.painter.borrow().is_some()
    }

    fn show_painting(&self, on: bool) {
        if let Some(b) = self.painter_button.borrow().as_ref() {
            self.syncing.set(true);
            b.set_active(on);
            self.syncing.set(false);
        }
        self.view.set_cursor(gdk::Cursor::from_name(if on { "copy" } else { "text" }, None).as_ref());
    }

    /// Gives the selection, or the paragraph clicked in, the copied look.
    fn paint(&self) {
        let Some(p) = self.painter.borrow().clone() else { return };
        let b = self.tb();
        match b.selection_bounds() {
            Some((s, e)) => {
                let run = p.run.clone();
                self.apply_char_range(s.offset(), e.offset(), &move |r| {
                    let keep = TextRun { link: r.link.take(), comment: r.comment, inserted: r.inserted, deleted: r.deleted, ..run.clone() };
                    *r = keep;
                });
            }
            None => {
                let line = b.iter_at_mark(&b.get_insert()).line();
                self.checkpoint();
                buffer::update_para(b, line, line, |q| {
                    q.style = p.para.style;
                    q.align = p.para.align;
                    q.line = p.para.line;
                    q.before = p.para.before;
                    q.after = p.para.after;
                });
                self.edited();
            }
        }
        if !p.sticky {
            self.stop_painting();
        }
        self.sync_controls();
    }
}

#[cfg(test)]
pub(super) fn paint_now(w: &Writer) {
    w.paint();
}

#[cfg(test)]
pub(super) fn start_now(w: &Writer, sticky: bool) {
    w.start_painting(sticky);
}
