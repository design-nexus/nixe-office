//! Widgets that sit in the text at a child anchor, besides tables: a page break
//! and a table of contents (each on a line of its own), and footnote marks
//! (inside a paragraph).

use crate::widgets;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub enum Mark {
    PageBreak,
    Contents(Rc<ContentsWidget>),
    Note(Rc<NoteWidget>),
}

/// Which mark to make while loading a document.
pub enum MarkKind<'a> {
    PageBreak,
    Contents,
    Note(&'a str),
}

/// The marks in the text: anchor, what it is, and its widget.
pub type Marks = Rc<RefCell<Vec<(gtk::TextChildAnchor, Mark, gtk::Widget)>>>;

/// A dashed rule labelled "Page break".
pub fn page_break() -> gtk::Widget {
    let row = widgets::hbox(10);
    row.add_css_class("page-break");
    let a = gtk::Separator::new(gtk::Orientation::Horizontal);
    a.set_hexpand(true);
    a.set_valign(gtk::Align::Center);
    let b = gtk::Separator::new(gtk::Orientation::Horizontal);
    b.set_hexpand(true);
    b.set_valign(gtk::Align::Center);
    row.append(&a);
    row.append(&widgets::label("Page break", "page-break-label"));
    row.append(&b);
    row.set_tooltip_text(Some("What follows starts on a new page. Select it and press Delete to remove it."));
    row.upcast()
}

/// A table of contents in the text: the headings, each a link to itself.
/// Page numbers appear in print and PDF.
pub struct ContentsWidget {
    pub root: gtk::Box,
    list: gtk::Box,
    /// Jump to the heading on this buffer line.
    go: Rc<dyn Fn(i32)>,
}

impl ContentsWidget {
    pub fn new(go: Rc<dyn Fn(i32)>) -> Rc<ContentsWidget> {
        let root = widgets::vbox(2);
        root.add_css_class("toc-block");
        root.append(&widgets::label("Contents", "toc-title"));
        let list = widgets::vbox(0);
        root.append(&list);
        root.set_tooltip_text(Some("Updates as you add headings; page numbers show in print and PDF"));
        Rc::new(ContentsWidget { root, list, go })
    }

    /// Show `entries`: (level 0–3, text, buffer line).
    pub fn refresh(&self, entries: &[(usize, String, i32)]) {
        while let Some(c) = self.list.first_child() {
            self.list.remove(&c);
        }
        if entries.is_empty() {
            self.list.append(&widgets::label("Headings will be listed here.", "toc-empty"));
        }
        for (level, text, line) in entries {
            let b = gtk::Button::new();
            b.add_css_class("toc-entry");
            b.add_css_class(&format!("toc-level-{level}"));
            let l = widgets::label(text, "");
            l.set_xalign(0.0);
            l.set_ellipsize(gtk::pango::EllipsizeMode::End);
            b.set_child(Some(&l));
            let (go, line) = (self.go.clone(), *line);
            b.connect_clicked(move |_| go(line));
            self.list.append(&b);
        }
    }
}

/// Removes a footnote from the text.
pub type OnDelete = Rc<dyn Fn(&Rc<NoteWidget>)>;

/// A footnote's number in the text. Clicking it opens the note to edit.
pub struct NoteWidget {
    pub button: gtk::Button,
    label: gtk::Label,
    pub text: RefCell<String>,
}

impl NoteWidget {
    /// `changed` runs after the note's words change; `delete` removes the mark.
    pub fn new(text: &str, changed: Rc<dyn Fn()>, delete: OnDelete) -> Rc<NoteWidget> {
        let label = widgets::label("•", "");
        let button = gtk::Button::new();
        button.add_css_class("note-mark");
        button.set_child(Some(&label));
        button.set_valign(gtk::Align::Start);
        button.set_focus_on_click(false);
        let n = Rc::new(NoteWidget { button, label, text: RefCell::new(text.to_string()) });
        let weak = Rc::downgrade(&n);
        n.button.connect_clicked(move |b| {
            let Some(n) = weak.upgrade() else { return };
            n.edit(b, changed.clone(), delete.clone());
        });
        n.update_tip();
        n
    }

    pub fn set_number(&self, k: usize) {
        self.label.set_text(&k.to_string());
    }

    fn update_tip(&self) {
        let t = self.text.borrow();
        self.button.set_tooltip_text(Some(if t.trim().is_empty() { "Footnote (click to write it)" } else { t.as_str() }));
    }

    fn edit(self: &Rc<Self>, at: &gtk::Button, changed: Rc<dyn Fn()>, delete: OnDelete) {
        let pop = gtk::Popover::new();
        pop.set_parent(at);
        let content = widgets::vbox(8);
        content.add_css_class("prompt-box");
        content.append(&widgets::label(&format!("Footnote {}", self.label.text()), "popover-title"));
        let view = gtk::TextView::new();
        view.add_css_class("note-text");
        view.set_wrap_mode(gtk::WrapMode::WordChar);
        view.buffer().set_text(&self.text.borrow());
        let scroll = gtk::ScrolledWindow::builder()
            .child(&view)
            .min_content_width(320)
            .min_content_height(90)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        scroll.add_css_class("comment-box");
        content.append(&scroll);
        let remove = gtk::Button::with_label("Remove footnote");
        remove.add_css_class("danger-text");
        remove.set_halign(gtk::Align::Start);
        content.append(&remove);
        pop.set_child(Some(&content));
        let me = Rc::downgrade(self);
        view.buffer().connect_changed(move |b| {
            let Some(n) = me.upgrade() else { return };
            let t = b.text(&b.start_iter(), &b.end_iter(), false).to_string();
            if *n.text.borrow() != t {
                *n.text.borrow_mut() = t;
                n.update_tip();
                changed();
            }
        });
        let me = Rc::downgrade(self);
        let p = pop.clone();
        remove.connect_clicked(move |_| {
            p.popdown();
            if let Some(n) = me.upgrade() {
                delete(&n);
            }
        });
        pop.connect_closed(|p| p.unparent());
        pop.popup();
        view.grab_focus();
    }
}
