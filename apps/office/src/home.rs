//! Home: start something new (blank or from a template), pick up a recent file,
//! or bring back work left unsaved by a crash.

use crate::widgets::{self, Page};
use crate::{app, doc, paths, recent, window};
use gtk::prelude::*;
use std::cell::RefCell;

struct Home {
    banners: gtk::Box,
    recent: gtk::Box,
    query: String,
}

thread_local! {
    static HOME: RefCell<Option<Home>> = const { RefCell::new(None) };
}

pub fn build(page: &Page) {
    let banners = widgets::vbox(0);
    page.body.append(&banners);

    let g = page.group("New");
    g.list.remove_css_class("group-list");
    let flow = gtk::FlowBox::new();
    flow.add_css_class("template-grid");
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_column_spacing(10);
    flow.set_row_spacing(10);
    flow.set_homogeneous(true);
    flow.set_min_children_per_line(1);
    flow.set_max_children_per_line(4);
    flow.set_valign(gtk::Align::Start);
    let kind = app::kind();
    let blank = card(kind.icon(), &format!("Blank {}", kind.noun()), "Start from an empty page.");
    // Only from Home itself: a second click as the page fades out isn't another new file.
    blank.connect_clicked(|_| {
        if window::current() == "home" {
            doc::new_document(None);
        }
    });
    flow.append(&blank);
    if let Some(ed) = doc::editor() {
        for t in ed.templates() {
            let c = card(t.icon, t.title, t.summary);
            let id = t.id;
            c.connect_clicked(move |_| {
                if window::current() == "home" {
                    doc::new_document(Some(id));
                }
            });
            flow.append(&c);
        }
    }
    g.add(&flow);

    let g = page.group("Recent");
    let recent_box = widgets::vbox(0);
    g.add(&recent_box);
    let (open_row, _) = widgets::button_row(
        "Open a file",
        &format!("Opens {}.", kind.opens().iter().map(|e| format!(".{e}")).collect::<Vec<_>>().join(", ")),
        "Open…",
        |_| doc::open_dialog(),
    );
    g.add(&open_row);

    HOME.with(|h| *h.borrow_mut() = Some(Home { banners, recent: recent_box, query: String::new() }));
    refresh();
    // Saving and opening change the list.
    doc::subscribe(&page.body, |c| {
        if matches!(c, doc::Change::Opened | doc::Change::Dirty) {
            refresh();
        }
    });
}

fn card(icon: &str, title: &str, summary: &str) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("template-card");
    let content = widgets::hbox(12);
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(20);
    image.set_valign(gtk::Align::Start);
    content.append(&image);
    let text = widgets::vbox(2);
    let t = widgets::label(title, "template-title");
    t.set_wrap(true);
    t.set_max_width_chars(24);
    text.append(&t);
    let s = widgets::label(summary, "template-summary");
    s.set_wrap(true);
    // Narrow labels let several cards share a line.
    s.set_max_width_chars(24);
    s.set_width_chars(18);
    text.append(&s);
    content.append(&text);
    b.set_child(Some(&content));
    b.set_tooltip_text(Some(summary));
    b
}

/// The top bar's search filters the recent files.
pub fn set_query(q: &str) {
    HOME.with(|h| {
        if let Some(h) = h.borrow_mut().as_mut() {
            h.query = q.to_lowercase();
        }
    });
    refresh();
}

pub fn recent_count() -> usize {
    recent::list().len()
}

pub fn refresh() {
    let Some((banners, list, query)) =
        HOME.with(|h| h.borrow().as_ref().map(|h| (h.banners.clone(), h.recent.clone(), h.query.clone())))
    else {
        return;
    };

    while let Some(c) = banners.first_child() {
        banners.remove(&c);
    }
    for r in doc::leftovers().into_iter().take(3) {
        let when = recent::ago(r.saved);
        let name = glib_escape(&r.title);
        let b = widgets::banner(&format!("Unsaved work on <b>{name}</b> from {when} can be brought back."), false);
        let restore = gtk::Button::with_label("Restore");
        restore.set_valign(gtk::Align::Center);
        let discard = gtk::Button::with_label("Discard");
        discard.add_css_class("flat");
        discard.set_valign(gtk::Align::Center);
        let snapshot = r.snapshot.clone();
        discard.connect_clicked(move |_| {
            if let Some(r) = doc::leftovers().into_iter().find(|r| r.snapshot == snapshot) {
                doc::discard(&r);
            }
            refresh();
        });
        let cell = std::rc::Rc::new(RefCell::new(Some(r)));
        restore.connect_clicked(move |_| {
            if let Some(r) = cell.borrow_mut().take() {
                doc::restore(r);
            }
        });
        b.append(&discard);
        b.append(&restore);
        banners.append(&b);
    }

    while let Some(c) = list.first_child() {
        list.remove(&c);
    }
    let items: Vec<recent::Recent> = recent::list()
        .into_iter()
        .filter(|r| query.is_empty() || r.path.to_string_lossy().to_lowercase().contains(&query))
        .take(if query.is_empty() { 12 } else { 60 })
        .collect();
    if items.is_empty() {
        let text = if query.is_empty() {
            "Files you open or save show up here.".to_string()
        } else {
            format!("No recent files match “{query}”.")
        };
        list.append(&widgets::row(&text, "", None));
        return;
    }
    for r in items {
        list.append(&recent_row(&r));
    }
}

fn glib_escape(s: &str) -> String {
    gtk::glib::markup_escape_text(s).to_string()
}

fn recent_row(r: &recent::Recent) -> gtk::Box {
    let name = r.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let folder = r.path.parent().map(paths::pretty).unwrap_or_default();
    let open = gtk::Button::with_label("Open");
    let p = r.path.clone();
    open.connect_clicked(move |_| doc::open(p.clone()));
    let forget = widgets::bar_button("window-close-symbolic", "Remove from this list");
    let p = r.path.clone();
    forget.connect_clicked(move |_| {
        recent::forget(&p);
        refresh();
    });
    let controls = widgets::hbox(6);
    controls.append(&widgets::label(&recent::ago(r.opened), "dim"));
    controls.append(&open);
    controls.append(&forget);
    let row = widgets::row(&name, &glib_escape(&folder), Some(controls.upcast_ref()));
    row.add_css_class("recent-row");
    // A click anywhere on the row opens it.
    let click = gtk::GestureClick::new();
    let p = r.path.clone();
    click.connect_released(move |g, n, _, _| {
        if n == 2 {
            g.set_state(gtk::EventSequenceState::Claimed);
            doc::open(p.clone());
        }
    });
    row.add_controller(click);
    row
}
