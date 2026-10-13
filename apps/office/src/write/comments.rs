//! Comments on spans of text (with their cards beside the page), footnotes,
//! page breaks and the table of contents: the Writer's side of each.

use super::Writer;
use super::buffer::{self, ParaProps};
use super::marks::{self, ContentsWidget, Mark, MarkKind, NoteWidget, SectionWidget};
use super::model::*;
use crate::{doc, widgets, window};
use gtk::glib;
use gtk::prelude::*;
use std::rc::Rc;

/// The look of commented text, and of resolved comments' text (none).
const COMMENT_BG: &str = "#fff1b8";

impl Writer {
    /// Builds the page break, contents and footnote widgets for the loader and inserts.
    pub(super) fn make_mark(&self) -> impl Fn(MarkKind) -> (Mark, gtk::Widget) + use<> {
        let me = self.me.borrow().clone();
        move |kind| match kind {
            MarkKind::PageBreak => (Mark::PageBreak, marks::page_break()),
            MarkKind::Contents => {
                let me = me.clone();
                let c = ContentsWidget::new(Rc::new(move |line| {
                    if let Some(w) = me.upgrade() {
                        w.go_to_line(line);
                    }
                }));
                let widget = c.root.clone().upcast();
                (Mark::Contents(c), widget)
            }
            MarkKind::Section(section) => {
                let m = me.clone();
                let w = SectionWidget::new(
                    section,
                    Rc::new(move || {
                        if let Some(w) = m.upgrade()
                            && !w.loading.get()
                        {
                            w.checkpoint();
                            w.edited();
                        }
                    }),
                );
                let widget = w.button.clone().upcast();
                (Mark::Section(w), widget)
            }
            MarkKind::Note(text, end) => {
                let (m1, m2) = (me.clone(), me.clone());
                let n = NoteWidget::new(
                    text,
                    end,
                    Rc::new(move || {
                        if let Some(w) = m1.upgrade()
                            && !w.loading.get()
                        {
                            w.begin_edit();
                            w.edited();
                        }
                    }),
                    Rc::new(move |n: &Rc<NoteWidget>| {
                        if let Some(w) = m2.upgrade() {
                            w.delete_note(n);
                        }
                    }),
                );
                let widget = n.button.clone().upcast();
                (Mark::Note(n), widget)
            }
        }
    }

    fn go_to_line(&self, line: i32) {
        window::navigate("document");
        if let Some(it) = self.tb().iter_at_line(line) {
            self.tb().place_cursor(&it);
            self.view.scroll_to_mark(&self.tb().get_insert(), 0.0, true, 0.0, 0.1);
            self.view.grab_focus();
        }
    }

    // ---------- Footnotes ----------

    /// Number the footnote marks in the order they appear, and forget deleted ones.
    pub(super) fn renumber_notes(&self) {
        let mut marks = self.marks.borrow_mut();
        marks.retain(|(a, _, _)| !a.is_deleted());
        let b = self.tb();
        let mut notes: Vec<(i32, Rc<NoteWidget>)> = marks
            .iter()
            .filter_map(|(a, m, _)| if let Mark::Note(n) = m { Some((b.iter_at_child_anchor(a).offset(), n.clone())) } else { None })
            .collect();
        notes.sort_by_key(|(o, _)| *o);
        // Footnotes and endnotes are counted apart (1, 2, 3 and i, ii, iii).
        let (mut foot, mut end) = (0, 0);
        for (_, n) in &notes {
            let k = if n.end { &mut end } else { &mut foot };
            *k += 1;
            n.set_number(*k);
        }
    }

    /// Put a footnote (or, `end`, an endnote) mark at the cursor and open it for writing.
    pub(super) fn insert_note(&self, end: bool) {
        let mut at = self.cursor_iter();
        if at.child_anchor().is_some() && at.starts_line() && at.ends_line() {
            return;
        }
        self.checkpoint();
        let b = self.tb();
        // Not inside a selection: after it.
        if let Some((_, e)) = b.selection_bounds() {
            at = e;
        }
        self.loading.set(true);
        let anchor = b.create_child_anchor(&mut at);
        let (mark, widget) = (self.make_mark())(MarkKind::Note("", end));
        self.view.add_child_at_anchor(&widget, &anchor);
        self.marks.borrow_mut().push((anchor, mark, widget.clone()));
        self.loading.set(false);
        self.edited();
        if let Some(btn) = widget.downcast_ref::<gtk::Button>() {
            let btn = btn.clone();
            // Once it's on screen, so the note's popover has somewhere to point.
            glib::idle_add_local_once(move || btn.emit_clicked());
        }
    }

    fn delete_note(&self, n: &Rc<NoteWidget>) {
        let anchor = self
            .marks
            .borrow()
            .iter()
            .find(|(_, m, _)| matches!(m, Mark::Note(x) if Rc::ptr_eq(x, n)))
            .map(|(a, _, _)| a.clone());
        let Some(anchor) = anchor else { return };
        self.checkpoint();
        let b = self.tb();
        let mut s = b.iter_at_child_anchor(&anchor);
        let mut e = s;
        e.forward_char();
        b.delete(&mut s, &mut e);
        self.edited();
        self.view.grab_focus();
    }

    // ---------- Page breaks and contents ----------

    /// A page break or a table of contents, on a line of its own at the cursor.
    /// A page break, table of contents or section break, on a line of its own at the cursor.
    pub(super) fn insert_mark_block(&self, kind: MarkKind<'static>) {
        let contents = matches!(kind, MarkKind::Contents);
        let section = matches!(kind, MarkKind::Section(_));
        if contents && self.marks.borrow().iter().any(|(a, m, _)| matches!(m, Mark::Contents(_)) && !a.is_deleted()) {
            window::toast("The document already has a table of contents.");
            return;
        }
        self.checkpoint();
        let b = self.tb();
        self.loading.set(true);
        let mut at = self.cursor_iter();
        if !at.starts_line() {
            b.insert(&mut at, "\n");
        }
        if !at.ends_line() {
            b.insert(&mut at, "\n");
            at.backward_char();
        }
        let line = at.line();
        buffer::set_para(b, line, line, ParaProps::default());
        let anchor = b.create_child_anchor(&mut at);
        let (mark, widget) = (self.make_mark())(kind);
        widget.set_size_request(self.view.width_request() - 4, -1);
        self.view.add_child_at_anchor(&widget, &anchor);
        self.marks.borrow_mut().push((anchor, mark, widget));
        // The cursor goes to the line after.
        let mut after = b.iter_at_line(line + 1).unwrap_or_else(|| b.end_iter());
        if after.line() == line {
            b.insert(&mut after, "\n");
        }
        b.place_cursor(&b.iter_at_line(line + 1).unwrap_or_else(|| b.end_iter()));
        self.loading.set(false);
        self.edited();
        self.refresh_contents();
        self.view.grab_focus();
        // A new section break opens to choose its columns.
        if section && let Some(btn) = self.marks.borrow().last().and_then(|(_, _, w)| w.clone().downcast::<gtk::Button>().ok()) {
            glib::idle_add_local_once(move || btn.emit_clicked());
        }
    }

    /// Show the current headings in every table of contents.
    pub(super) fn refresh_contents(&self) {
        let tocs: Vec<Rc<ContentsWidget>> = self
            .marks
            .borrow()
            .iter()
            .filter_map(|(a, m, _)| if let (false, Mark::Contents(c)) = (a.is_deleted(), m) { Some(c.clone()) } else { None })
            .collect();
        if tocs.is_empty() {
            return;
        }
        let b = self.tb();
        let mut entries = Vec::new();
        for line in 0..b.line_count() - 1 {
            let Some(start) = b.iter_at_line(line) else { continue };
            let p = buffer::para_props(b, &start);
            let level = match p.style {
                Style::Title => 0,
                Style::H1 => 1,
                Style::H2 => 2,
                Style::H3 => 3,
                _ => continue,
            };
            let mut end = start;
            end.forward_to_line_end();
            let text = b.text(&start, &end, false).replace('\u{FFFC}', "").trim().to_string();
            if !text.is_empty() {
                entries.push((level, text, line));
            }
        }
        for t in tocs {
            t.refresh(&entries);
        }
    }

    // ---------- Comments ----------

    /// Where a comment's text is, if it's still in the document.
    fn comment_range(&self, id: u32) -> Option<(gtk::TextIter, gtk::TextIter)> {
        let b = self.tb();
        let tag = b.tag_table().lookup(&format!("cm:{id}"))?;
        let mut s = b.start_iter();
        if !s.has_tag(&tag) && !s.forward_to_tag_toggle(Some(&tag)) {
            return None;
        }
        if !s.has_tag(&tag) {
            return None;
        }
        let mut e = s;
        e.forward_to_tag_toggle(Some(&tag));
        Some((s, e))
    }

    /// Comment on the selected text.
    pub(super) fn add_comment(&self) {
        let Some((s, e)) = self.tb().selection_bounds() else {
            window::toast("Select the text to comment on.");
            return;
        };
        let (so, eo) = (s.offset(), e.offset());
        let me = self.me.borrow().clone();
        comment_dialog("Add a comment", "", move |text| {
            let Some(w) = me.upgrade() else { return };
            w.checkpoint();
            let id = w.comments.borrow().iter().map(|c| c.id).max().unwrap_or(0) + 1;
            let author = glib::real_name().into_string().ok().filter(|n| !n.trim().is_empty() && n != "Unknown");
            let author = author.or_else(|| glib::user_name().into_string().ok()).unwrap_or_default();
            let date = glib::DateTime::now_utc().ok().and_then(|d| d.format("%Y-%m-%dT%H:%M:%SZ").ok()).map(|s| s.to_string());
            w.comments.borrow_mut().push(Comment { id, author, date: date.unwrap_or_default(), text, resolved: false });
            let b = w.tb();
            let (s, e) = (b.iter_at_offset(so), b.iter_at_offset(eo));
            // A span belongs to one comment: a new one takes over any overlap.
            buffer::remove_char_tags(b, &s, &e, &["cm:"]);
            b.apply_tag(&buffer::tag(b, &format!("cm:{id}")), &s, &e);
            w.edited();
            w.refresh_comments();
        });
    }

    fn edit_comment(&self, id: u32) {
        let Some(c) = self.comments.borrow().iter().find(|c| c.id == id).cloned() else { return };
        let me = self.me.borrow().clone();
        comment_dialog("Edit comment", &c.text, move |text| {
            let Some(w) = me.upgrade() else { return };
            w.checkpoint();
            if let Some(c) = w.comments.borrow_mut().iter_mut().find(|c| c.id == id) {
                c.text = text;
            }
            w.edited();
            w.refresh_comments();
        });
    }

    fn resolve_comment(&self, id: u32) {
        self.checkpoint();
        if let Some(c) = self.comments.borrow_mut().iter_mut().find(|c| c.id == id) {
            c.resolved = !c.resolved;
        }
        self.edited();
        self.refresh_comments();
    }

    fn delete_comment(&self, id: u32) {
        self.checkpoint();
        let b = self.tb();
        b.remove_tag_by_name(&format!("cm:{id}"), &b.start_iter(), &b.end_iter());
        self.comments.borrow_mut().retain(|c| c.id != id);
        self.edited();
        self.refresh_comments();
    }

    /// Rebuild the comment cards beside the page, in the order of their text.
    pub(super) fn refresh_comments(&self) {
        let panel = &self.comment_panel;
        while let Some(c) = panel.first_child() {
            panel.remove(&c);
        }
        let comments = self.comments.borrow().clone();
        let mut placed: Vec<(i32, Comment, String)> = Vec::new();
        for c in comments {
            let tag = buffer::tag(self.tb(), &format!("cm:{}", c.id));
            tag.set_background(if c.resolved { None } else { Some(COMMENT_BG) });
            if let Some((s, e)) = self.comment_range(c.id) {
                let quote = self.tb().text(&s, &e, false).replace('\u{FFFC}', "");
                placed.push((s.offset(), c, quote));
            }
        }
        placed.sort_by_key(|(o, _, _)| *o);
        panel.set_visible(!placed.is_empty());
        if !placed.is_empty() {
            let head = widgets::label("COMMENTS", "group-title");
            // In the Inspector, its section has the heading.
            head.set_visible(!panel.has_css_class("in-inspector"));
            panel.append(&head);
        }
        for (_, c, quote) in placed {
            let card = widgets::vbox(6);
            card.add_css_class("comment-card");
            if c.resolved {
                card.add_css_class("resolved");
            }
            let who = if c.author.is_empty() { "Comment".to_string() } else { c.author.clone() };
            let when = c.date.get(..10).map(|d| format!(" · {d}")).unwrap_or_default();
            card.append(&widgets::label(&format!("{who}{when}"), "comment-author"));
            let q = widgets::label(&format!("“{}”", quote.trim()), "comment-quote");
            q.set_ellipsize(gtk::pango::EllipsizeMode::End);
            q.set_xalign(0.0);
            card.append(&q);
            let t = widgets::label(&c.text, "comment-body");
            t.set_wrap(true);
            t.set_xalign(0.0);
            card.append(&t);
            let row = widgets::hbox(2);
            let id = c.id;
            let me = self.me.borrow().clone();
            type Act = fn(&Writer, u32);
            let actions: [(&str, &str, Act); 3] = [
                ("nxo-text-cursor-symbolic", "Edit", |w, id| w.edit_comment(id)),
                ("nxo-check-symbolic", if c.resolved { "Reopen" } else { "Resolve" }, |w, id| w.resolve_comment(id)),
                ("nxo-trash-2-symbolic", "Delete the comment", |w, id| w.delete_comment(id)),
            ];
            for (icon, tip, f) in actions {
                let b = widgets::bar_button(icon, tip);
                let me = me.clone();
                b.connect_clicked(move |_| {
                    if let Some(w) = me.upgrade() {
                        f(&w, id);
                    }
                });
                row.append(&b);
            }
            card.append(&row);
            // Clicking the card selects its text.
            let click = gtk::GestureClick::new();
            let me = me.clone();
            click.connect_released(move |_, _, _, _| {
                let Some(w) = me.upgrade() else { return };
                if let Some((s, e)) = w.comment_range(id) {
                    w.tb().select_range(&s, &e);
                    w.view.scroll_to_iter(&mut s.clone(), 0.1, false, 0.0, 0.0);
                    w.view.grab_focus();
                }
            });
            card.add_controller(click);
            panel.append(&card);
        }
        doc::status_changed();
    }
}

/// Ask for a comment's words; `save` gets them when they aren't empty.
fn comment_dialog(title: &str, current: &str, save: impl Fn(String) + 'static) {
    let (dialog, card) = widgets::dialog(title, 420);
    let text = gtk::TextView::new();
    text.set_wrap_mode(gtk::WrapMode::WordChar);
    text.buffer().set_text(current);
    let scroll =
        gtk::ScrolledWindow::builder().child(&text).min_content_height(110).hscrollbar_policy(gtk::PolicyType::Never).build();
    scroll.add_css_class("comment-box");
    card.append(&scroll);
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
    let (d, b) = (dialog.clone(), text.buffer());
    ok.connect_clicked(move |_| {
        let t = b.text(&b.start_iter(), &b.end_iter(), false).trim().to_string();
        if t.is_empty() {
            return;
        }
        d.close();
        save(t);
    });
    dialog.present();
    text.grab_focus();
}
