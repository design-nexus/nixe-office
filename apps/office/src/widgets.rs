//! Nexus-style building blocks: pages, groups and option rows.

use crate::{cmd, paths};
use gtk::pango;
use gtk::prelude::*;
use std::path::PathBuf;
use std::rc::Rc;

// ---------- Page / group ----------

pub struct Page {
    pub root: gtk::ScrolledWindow,
    pub body: gtk::Box,
}

/// A page. Its name shows in the top bar (or the settings dialog's header), so
/// the page itself starts straight with its content.
pub fn page(section: &str) -> Page {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.add_css_class("settings-page");
    body.add_css_class(&format!("page-{section}"));

    body.set_hexpand(true);

    let root = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        // Scrolls with wheel, trackpad and keyboard; no visible scrollbar.
        .vscrollbar_policy(gtk::PolicyType::External)
        .child(&body)
        .vexpand(true)
        .build();
    Page { root, body }
}

impl Page {
    pub fn group(&self, title: &str) -> Group {
        let wrapper = gtk::Box::new(gtk::Orientation::Vertical, 0);
        wrapper.add_css_class("settings-group");
        // The settings dialog lists groups by this name.
        wrapper.set_widget_name(title);
        if !title.is_empty() {
            let l = gtk::Label::new(Some(&title.to_uppercase()));
            l.add_css_class("group-title");
            l.set_xalign(0.0);
            wrapper.append(&l);
        }
        // Rows join into one card, split by hairlines.
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        list.add_css_class("group-list");
        wrapper.append(&list);
        self.body.append(&wrapper);
        Group { wrapper, list }
    }
}

pub fn banner(text: &str, warning: bool) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    b.add_css_class("banner");
    if warning {
        b.add_css_class("warning");
    }
    let icon = gtk::Image::from_icon_name(if warning { "dialog-warning-symbolic" } else { "dialog-information-symbolic" });
    icon.set_valign(gtk::Align::Start);
    let l = gtk::Label::new(None);
    l.set_markup(text);
    l.set_wrap(true);
    l.set_xalign(0.0);
    l.set_hexpand(true);
    b.append(&icon);
    b.append(&l);
    b
}

#[derive(Clone)]
pub struct Group {
    pub wrapper: gtk::Box,
    pub list: gtk::Box,
}

impl Group {
    pub fn add(&self, w: &impl IsA<gtk::Widget>) {
        self.list.append(w);
    }

    pub fn note(&self, text: &str) {
        let l = gtk::Label::new(None);
        l.set_markup(text);
        l.add_css_class("group-note");
        l.set_xalign(0.0);
        l.set_wrap(true);
        // Notes sit just under the group title.
        self.wrapper.insert_child_after(&l, self.wrapper.first_child().as_ref());
    }
}

// ---------- Config files and bar buttons ----------

/// Opens a page's config file; a menu picks one when there are several.
pub fn config_button(files: &[PathBuf]) -> gtk::Widget {
    if let [path] = files {
        let path = path.clone();
        let b = bar_button("text-editor-symbolic", &format!("Open {}", paths::pretty(&path)));
        b.connect_clicked(move |_| cmd::open_in_editor(&path));
        return b.upcast();
    }
    let menu = gtk::MenuButton::new();
    menu.set_icon_name("text-editor-symbolic");
    menu.add_css_class("bar-button");
    menu.set_valign(gtk::Align::Center);
    menu.set_tooltip_text(Some("Open a config file"));
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let popover = gtk::Popover::new();
    for path in files {
        let b = gtk::Button::with_label(&paths::pretty(path));
        b.add_css_class("flat");
        if let Some(label) = b.child().and_downcast::<gtk::Label>() {
            label.set_xalign(0.0);
        }
        let p = path.clone();
        let pop = popover.clone();
        b.connect_clicked(move |_| {
            pop.popdown();
            cmd::open_in_editor(&p);
        });
        list.append(&b);
    }
    popover.set_child(Some(&list));
    menu.set_popover(Some(&popover));
    menu.upcast()
}

/// A flat icon button for the top bar and dialog headers.
pub fn bar_button(icon: &str, tooltip: &str) -> gtk::Button {
    let b = gtk::Button::from_icon_name(icon);
    b.add_css_class("bar-button");
    b.set_tooltip_text(Some(tooltip));
    b.set_valign(gtk::Align::Center);
    b
}

// ---------- Rows ----------

/// An option card: title and description on the left, control on the right.
pub fn row(title: &str, desc: &str, control: Option<&gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    row.add_css_class("settings-option");
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_valign(gtk::Align::Center);
    text.set_hexpand(true);
    let t = gtk::Label::new(Some(title));
    t.add_css_class("settings-option-title");
    t.set_xalign(0.0);
    t.set_wrap(true);
    text.append(&t);
    if !desc.is_empty() {
        let d = gtk::Label::new(None);
        d.set_markup(desc);
        d.add_css_class("settings-option-description");
        d.set_xalign(0.0);
        d.set_wrap(true);
        d.set_wrap_mode(pango::WrapMode::WordChar);
        text.append(&d);
    }
    row.append(&text);
    if let Some(c) = control {
        c.set_valign(gtk::Align::Center);
        row.append(c);
    }
    row
}

pub fn switch_row(title: &str, desc: &str, active: bool, on_change: impl Fn(bool) + 'static) -> (gtk::Box, gtk::Switch) {
    let sw = gtk::Switch::new();
    sw.set_active(active);
    sw.connect_active_notify(move |s| on_change(s.is_active()));
    let r = row(title, desc, Some(sw.upcast_ref()));
    (r, sw)
}

pub fn dropdown(options: &[(String, String)], current: &str) -> gtk::DropDown {
    let labels: Vec<&str> = options.iter().map(|(_, l)| l.as_str()).collect();
    let dd = gtk::DropDown::from_strings(&labels);
    if let Some(i) = options.iter().position(|(id, _)| id == current) {
        dd.set_selected(i as u32);
    } else {
        dd.set_selected(gtk::INVALID_LIST_POSITION);
    }
    dd
}

pub fn opts(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

pub fn choice_row(
    title: &str,
    desc: &str,
    options: Vec<(String, String)>,
    current: &str,
    on_change: impl Fn(String) + 'static,
) -> (gtk::Box, gtk::DropDown) {
    let dd = dropdown(&options, current);
    dd.connect_selected_notify(move |d| {
        if let Some((id, _)) = options.get(d.selected() as usize) {
            on_change(id.clone());
        }
    });
    let r = row(title, desc, Some(dd.upcast_ref()));
    (r, dd)
}

pub fn button_row(title: &str, desc: &str, label: &str, on_click: impl Fn(&gtk::Button) + 'static) -> (gtk::Box, gtk::Button) {
    let b = gtk::Button::with_label(label);
    b.connect_clicked(on_click);
    let r = row(title, desc, Some(b.upcast_ref()));
    (r, b)
}

pub fn hbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Horizontal, spacing)
}

pub fn vbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, spacing)
}

pub fn label(text: &str, class: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    if !class.is_empty() {
        l.add_css_class(class);
    }
    l.set_xalign(0.0);
    l
}

/// A modal card dialog in the app's style. Returns the window and its content box.
pub fn dialog(title: &str, width: i32) -> (gtk::Window, gtk::Box) {
    let dialog = gtk::Window::builder().modal(true).title(title).default_width(width).build();
    if let Some(parent) = crate::window::window() {
        dialog.set_transient_for(Some(&parent));
    }
    dialog.add_css_class("office-window");
    dialog.set_titlebar(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    let card = vbox(12);
    card.add_css_class("dialog-card");
    card.append(&label(title, "section-title"));
    dialog.set_child(Some(&card));
    let keys = gtk::EventControllerKey::new();
    let d = dialog.clone();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape {
            d.close();
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    dialog.add_controller(keys);
    (dialog, card)
}

/// Buttons joined into one control; exactly one is selected.
pub fn segmented(options: &[(String, String)], current: &str, on_change: impl Fn(String) + 'static) -> gtk::Box {
    let bx = hbox(0);
    bx.add_css_class("segmented");
    bx.set_valign(gtk::Align::Center);
    let mut first: Option<gtk::ToggleButton> = None;
    let on_change = Rc::new(on_change);
    for (id, label) in options {
        let b = gtk::ToggleButton::with_label(label);
        b.add_css_class("segment");
        if let Some(f) = &first {
            b.set_group(Some(f));
        } else {
            first = Some(b.clone());
        }
        b.set_active(id == current);
        let id = id.clone();
        let cb = on_change.clone();
        b.connect_toggled(move |b| {
            if b.is_active() {
                cb(id.clone());
            }
        });
        bx.append(&b);
    }
    bx
}

pub fn segmented_row(
    title: &str,
    desc: &str,
    options: Vec<(String, String)>,
    current: &str,
    on_change: impl Fn(String) + 'static,
) -> gtk::Box {
    let seg = segmented(&options, current, on_change);
    row(title, desc, Some(seg.upcast_ref()))
}

/// Rows of key caps for the keyboard help.
pub fn key_caps(keys: &[&str]) -> gtk::Box {
    let caps = hbox(4);
    for (i, k) in keys.iter().enumerate() {
        if i > 0 {
            caps.append(&label("+", "dim"));
        }
        caps.append(&label(k, "key-cap"));
    }
    caps
}

/// A row whose children wrap onto more lines when it's too narrow, like words
/// in a paragraph (toolbar groups at half-screen width).
pub fn wrap_row(spacing: i32, line_spacing: i32) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.set_layout_manager(Some(wrap::WrapLayout::new(spacing, line_spacing)));
    row
}

mod wrap {
    use gtk::glib;
    use gtk::prelude::*;
    use gtk::subclass::prelude::*;
    use std::cell::Cell;

    mod imp {
        use super::*;

        #[derive(Default)]
        pub struct WrapLayout {
            pub spacing: Cell<i32>,
            pub line_spacing: Cell<i32>,
        }

        #[glib::object_subclass]
        impl ObjectSubclass for WrapLayout {
            const NAME: &'static str = "NxoWrapLayout";
            type Type = super::WrapLayout;
            type ParentType = gtk::LayoutManager;
        }

        impl ObjectImpl for WrapLayout {}

        fn children(w: &gtk::Widget) -> Vec<gtk::Widget> {
            let mut out = Vec::new();
            let mut c = w.first_child();
            while let Some(x) = c {
                if x.should_layout() {
                    out.push(x.clone());
                }
                c = x.next_sibling();
            }
            out
        }

        impl WrapLayout {
            /// Lines of (child, width) for a given width.
            fn lines(&self, kids: &[gtk::Widget], width: i32) -> Vec<Vec<(gtk::Widget, i32)>> {
                let spacing = self.spacing.get();
                let mut lines: Vec<Vec<(gtk::Widget, i32)>> = vec![Vec::new()];
                let mut x = 0;
                for k in kids {
                    let (_, nat, _, _) = k.measure(gtk::Orientation::Horizontal, -1);
                    let w = nat.min(width.max(1));
                    if !lines.last().is_some_and(|l| l.is_empty()) && x + spacing + w > width {
                        lines.push(Vec::new());
                        x = 0;
                    }
                    let line = lines.last_mut().expect("a line");
                    if !line.is_empty() {
                        x += spacing;
                    }
                    x += w;
                    line.push((k.clone(), w));
                }
                lines
            }

            fn line_height(line: &[(gtk::Widget, i32)]) -> i32 {
                line.iter().map(|(k, w)| k.measure(gtk::Orientation::Vertical, *w).1).max().unwrap_or(0)
            }
        }

        impl LayoutManagerImpl for WrapLayout {
            fn request_mode(&self, _widget: &gtk::Widget) -> gtk::SizeRequestMode {
                gtk::SizeRequestMode::HeightForWidth
            }

            fn measure(&self, widget: &gtk::Widget, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
                let kids = children(widget);
                if orientation == gtk::Orientation::Horizontal {
                    let min = kids.iter().map(|k| k.measure(orientation, -1).0).max().unwrap_or(0);
                    let nat: i32 = kids.iter().map(|k| k.measure(orientation, -1).1).sum::<i32>()
                        + self.spacing.get() * (kids.len() as i32 - 1).max(0);
                    (min, nat, -1, -1)
                } else {
                    let width = if for_size < 0 { i32::MAX / 4 } else { for_size };
                    let ls = self.lines(&kids, width);
                    let h: i32 = ls.iter().map(|l| Self::line_height(l)).sum::<i32>()
                        + self.line_spacing.get() * (ls.len() as i32 - 1).max(0);
                    (h, h, -1, -1)
                }
            }

            fn allocate(&self, widget: &gtk::Widget, width: i32, _height: i32, _baseline: i32) {
                let kids = children(widget);
                let mut y = 0;
                for line in self.lines(&kids, width) {
                    let h = Self::line_height(&line);
                    let mut x = 0;
                    for (k, cw) in line {
                        k.size_allocate(&gtk::Allocation::new(x, y, cw, h), -1);
                        x += cw + self.spacing.get();
                    }
                    y += h + self.line_spacing.get();
                }
            }
        }
    }

    glib::wrapper! {
        pub struct WrapLayout(ObjectSubclass<imp::WrapLayout>) @extends gtk::LayoutManager;
    }

    impl WrapLayout {
        pub fn new(spacing: i32, line_spacing: i32) -> WrapLayout {
            let l: WrapLayout = glib::Object::new();
            l.imp().spacing.set(spacing);
            l.imp().line_spacing.set(line_spacing);
            l
        }
    }
}
