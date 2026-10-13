//! Tracked changes: while tracking, typed text is marked as added and deleted
//! text stays, struck through, until the change is accepted or rejected.

use super::Writer;
use super::buffer;
use super::model::*;
use crate::{doc, widgets, window};
use gtk::glib;
use gtk::prelude::*;
use std::rc::Rc;

/// Who is editing, for new changes and comments.
pub fn author() -> String {
    glib::real_name()
        .into_string()
        .ok()
        .filter(|n| !n.trim().is_empty() && n != "Unknown")
        .or_else(|| glib::user_name().into_string().ok())
        .unwrap_or_default()
}

pub fn now() -> String {
    glib::DateTime::now_utc().ok().and_then(|d| d.format("%Y-%m-%dT%H:%M:%SZ").ok()).map(|s| s.to_string()).unwrap_or_default()
}

/// Every span carrying a tag whose name starts with `prefix`: (id, start, end).
fn spans(b: &gtk::TextBuffer, prefix: &str) -> Vec<(u32, i32, i32)> {
    let mut tags = Vec::new();
    b.tag_table().foreach(|t| {
        if let Some(n) = t.name()
            && let Some(id) = n.strip_prefix(prefix).and_then(|v| v.parse::<u32>().ok())
        {
            tags.push((id, t.clone()));
        }
    });
    let mut out = Vec::new();
    for (id, tag) in tags {
        let mut it = b.start_iter();
        loop {
            // To the next start of the tag, then to where it ends.
            if !it.has_tag(&tag) && (!it.forward_to_tag_toggle(Some(&tag)) || !it.has_tag(&tag)) {
                break;
            }
            let start = it.offset();
            let more = it.forward_to_tag_toggle(Some(&tag));
            out.push((id, start, it.offset()));
            if !more {
                break;
            }
        }
    }
    out.sort_by_key(|s| s.1);
    out
}

impl Writer {
    /// A revision for new edits now: the one next to `at` if it's ours and of
    /// the same kind, otherwise a new one.
    fn revision_near(&self, at: &gtk::TextIter, deleted: bool) -> u32 {
        let near = |it: &gtk::TextIter| {
            let f = buffer::char_format(it);
            if deleted { f.deleted } else { f.inserted }
        };
        let mut before = *at;
        let candidate = if before.backward_char() { near(&before) } else { None }.or_else(|| near(at));
        let me = author();
        if let Some(id) = candidate
            && self.revisions.borrow().iter().any(|r| r.id == id && r.author == me)
        {
            return id;
        }
        // The newest revision, if nothing in the text uses it yet (the cursor
        // moved without typing), rather than one per cursor move.
        let table = self.tb().tag_table();
        let unused = self.revisions.borrow().last().filter(|r| {
            r.author == me && table.lookup(&format!("ins:{}", r.id)).is_none() && table.lookup(&format!("del:{}", r.id)).is_none()
        }).map(|r| r.id);
        if let Some(id) = unused {
            if let Some(r) = self.revisions.borrow_mut().last_mut() {
                r.date = now();
            }
            return id;
        }
        let id = self.revisions.borrow().iter().map(|r| r.id).max().unwrap_or(0) + 1;
        self.revisions.borrow_mut().push(Revision { id, author: me, date: now() });
        id
    }

    /// The formatting typed text gets while tracking: marked as added.
    pub(super) fn tracked_typing(&self, typing: &mut TextRun, at: &gtk::TextIter) {
        typing.deleted = None;
        typing.inserted = if self.tracking.get() { Some(self.revision_near(at, false)) } else { None };
    }

    /// Delete `start..end` the tracked way: text added while tracking really
    /// goes; the rest is marked deleted. Line breaks are left alone.
    pub(super) fn track_delete(&self, start: i32, end: i32) {
        if start >= end {
            return;
        }
        self.checkpoint();
        let b = self.tb();
        self.loading.set(true);
        // Work from the end, so earlier offsets stay put.
        let mut pos = end;
        while pos > start {
            let it = b.iter_at_offset(pos - 1);
            let f = buffer::char_format(&it);
            // A run of characters alike in how they're deleted.
            let mut run_start = pos - 1;
            while run_start > start {
                let prev = b.iter_at_offset(run_start - 1);
                let pf = buffer::char_format(&prev);
                if (pf.inserted.is_some(), pf.deleted.is_some(), prev.char() == '\n')
                    != (f.inserted.is_some(), f.deleted.is_some(), it.char() == '\n')
                {
                    break;
                }
                run_start -= 1;
            }
            let (mut s, mut e) = (b.iter_at_offset(run_start), b.iter_at_offset(pos));
            if it.char() == '\n' || f.deleted.is_some() {
                // Paragraph breaks aren't tracked; deleted text is already marked.
            } else if f.inserted.is_some() {
                b.delete(&mut s, &mut e);
            } else {
                let id = self.revision_near(&e, true);
                b.apply_tag(&buffer::tag(b, &format!("del:{id}")), &s, &e);
            }
            pos = run_start;
        }
        self.loading.set(false);
        self.edited();
        self.refresh_changes();
    }

    /// Backspace, Delete, typing over or cutting a selection, while tracking.
    /// Returns true when the key was dealt with here.
    pub(super) fn tracked_key(&self, key: gtk::gdk::Key, ctrl: bool) -> bool {
        use gtk::gdk::Key;
        if !self.tracking.get() {
            return false;
        }
        let b = self.tb();
        if let Some((s, e)) = b.selection_bounds() {
            let (so, eo) = (s.offset(), e.offset());
            let typed = !ctrl && key.to_unicode().is_some_and(|c| !c.is_control());
            match key {
                Key::BackSpace | Key::Delete | Key::KP_Delete => {
                    self.track_delete(so, eo);
                    b.place_cursor(&b.iter_at_offset(so));
                    return true;
                }
                Key::x | Key::X if ctrl => {
                    if let Some(d) = gtk::gdk::Display::default() {
                        d.clipboard().set_text(&b.text(&s, &e, false));
                    }
                    self.track_delete(so, eo);
                    b.place_cursor(&b.iter_at_offset(so));
                    return true;
                }
                // Typing or pasting over a selection: mark it deleted, then add after it.
                Key::v | Key::V if ctrl => {
                    self.track_delete(so, eo);
                    b.place_cursor(&b.iter_at_offset(eo.min(b.char_count())));
                    return false;
                }
                _ if typed => {
                    self.track_delete(so, eo);
                    b.place_cursor(&b.iter_at_offset(eo.min(b.char_count())));
                    return false;
                }
                _ => return false,
            }
        }
        let at = self.cursor_iter().offset();
        match key {
            Key::BackSpace if at > 0 => {
                // Step back over text already marked deleted.
                let mut p = at;
                while p > 0 && buffer::char_format(&b.iter_at_offset(p - 1)).deleted.is_some() {
                    p -= 1;
                }
                if p == 0 {
                    return true;
                }
                if b.iter_at_offset(p - 1).char() == '\n' {
                    window::toast("Joining paragraphs isn't tracked: turn off Track changes to do it.");
                    return true;
                }
                self.track_delete(p - 1, p);
                // Before the character: struck through now, or gone.
                b.place_cursor(&b.iter_at_offset(p - 1));
                true
            }
            Key::Delete | Key::KP_Delete => {
                let mut p = at;
                let last = b.char_count() - 1;
                while p < last && buffer::char_format(&b.iter_at_offset(p)).deleted.is_some() {
                    p += 1;
                }
                if p >= last || b.iter_at_offset(p).char() == '\n' {
                    return true;
                }
                let inserted = buffer::char_format(&b.iter_at_offset(p)).inserted.is_some();
                self.track_delete(p, p + 1);
                b.place_cursor(&b.iter_at_offset(if inserted { p } else { p + 1 }));
                true
            }
            _ => false,
        }
    }

    /// Accept or reject changes: all of them, or (`only`) one revision.
    pub(super) fn settle(&self, accept: bool, only: Option<u32>) {
        let b = self.tb();
        let ins = spans(b, "ins:");
        let del = spans(b, "del:");
        let pick = |id: u32| only.is_none_or(|o| o == id);
        if !ins.iter().chain(&del).any(|(id, _, _)| pick(*id)) {
            window::toast(if only.is_some() { "The cursor isn't in a change." } else { "There are no changes." });
            return;
        }
        self.checkpoint();
        self.loading.set(true);
        // Text that goes: rejected additions, accepted deletions. From the end.
        let mut gone: Vec<(i32, i32)> = (if accept { &del } else { &ins }).iter().filter(|s| pick(s.0)).map(|s| (s.1, s.2)).collect();
        gone.sort_by_key(|g| std::cmp::Reverse(g.0));
        for (s, e) in gone {
            let (mut s, mut e) = (b.iter_at_offset(s), b.iter_at_offset(e));
            b.delete(&mut s, &mut e);
        }
        // What stays is plain text again.
        for (prefix, list) in [("ins:", &ins), ("del:", &del)] {
            for (id, _, _) in list.iter().filter(|s| pick(s.0)) {
                b.remove_tag_by_name(&format!("{prefix}{id}"), &b.start_iter(), &b.end_iter());
            }
        }
        if let Some(id) = only {
            self.revisions.borrow_mut().retain(|r| r.id != id);
        }
        self.loading.set(false);
        self.edited();
        self.refresh_changes();
    }

    /// The change at the cursor, if any.
    fn change_here(&self) -> Option<u32> {
        let it = self.cursor_iter();
        let f = buffer::char_format(&it);
        let mut before = it;
        let g = if before.backward_char() { buffer::char_format(&before) } else { TextRun::default() };
        f.inserted.or(f.deleted).or(g.inserted).or(g.deleted)
    }

    /// Move to the next change after the cursor (around to the first).
    fn next_change(&self) {
        let b = self.tb();
        let mut all = spans(b, "ins:");
        all.extend(spans(b, "del:"));
        all.sort_by_key(|s| s.1);
        let at = self.cursor_iter().offset();
        let next = all.iter().find(|s| s.1 > at).or(all.first());
        match next {
            Some((_, s, e)) => {
                b.select_range(&b.iter_at_offset(*s), &b.iter_at_offset(*e));
                self.view.scroll_to_iter(&mut b.iter_at_offset(*s), 0.1, false, 0.0, 0.0);
                self.view.grab_focus();
            }
            None => window::toast("There are no changes."),
        }
    }

    /// Show the toolbar's tracking state.
    pub(super) fn refresh_changes(&self) {
        if let Some(c) = self.controls.borrow().as_ref() {
            self.syncing.set(true);
            c.track.set_active(self.tracking.get());
            self.syncing.set(false);
        }
        doc::status_changed();
    }

    /// The Track changes switch and the menu to go through changes.
    pub(super) fn changes_buttons(self: &Rc<Self>) -> (gtk::ToggleButton, gtk::MenuButton) {
        let track = gtk::ToggleButton::new();
        track.set_icon_name("nxo-file-pen-line-symbolic");
        track.add_css_class("tool-toggle");
        track.set_tooltip_text(Some("Track changes (Ctrl+Shift+T): new text is marked, deleted text stays struck through"));
        let me = Rc::downgrade(self);
        track.connect_toggled(move |b| {
            if let Some(w) = me.upgrade()
                && !w.syncing.get()
            {
                w.set_tracking(b.is_active());
            }
        });
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("nxo-check-check-symbolic");
        menu.add_css_class("bar-button");
        menu.set_tooltip_text(Some("Review changes"));
        menu.set_valign(gtk::Align::Center);
        let pop = gtk::Popover::new();
        pop.add_css_class("menu-popover");
        let list = widgets::vbox(0);
        type Act = fn(&Writer);
        let items: [(&str, Act); 5] = [
            ("Next change", |w| w.next_change()),
            ("Accept this change", |w| w.settle(true, w.change_here().or(Some(0)))),
            ("Reject this change", |w| w.settle(false, w.change_here().or(Some(0)))),
            ("Accept all changes", |w| w.settle(true, None)),
            ("Reject all changes", |w| w.settle(false, None)),
        ];
        for (i, (label, f)) in items.into_iter().enumerate() {
            let b = gtk::Button::with_label(label);
            b.add_css_class("menu-item");
            if let Some(l) = b.child().and_downcast::<gtk::Label>() {
                l.set_xalign(0.0);
            }
            let me = Rc::downgrade(self);
            let p = pop.clone();
            b.connect_clicked(move |_| {
                p.popdown();
                if let Some(w) = me.upgrade() {
                    f(&w);
                }
            });
            list.append(&b);
            if i == 0 || i == 2 {
                list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            }
        }
        pop.set_child(Some(&list));
        menu.set_popover(Some(&pop));
        (track, menu)
    }

    pub(super) fn set_tracking(&self, on: bool) {
        if self.tracking.get() == on {
            return;
        }
        self.checkpoint();
        self.tracking.set(on);
        self.pick_up_format();
        self.edited();
        self.refresh_changes();
        window::toast(if on { "Tracking changes." } else { "Changes are no longer tracked." });
    }
}
