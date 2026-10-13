//! Nixe-style building blocks: pages, groups and option rows.

pub use nixe_ui::widgets::*;

use gtk::prelude::*;

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

// ---------- Rows ----------

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

/// A box that shows its one child larger or smaller: the child is laid out at
/// 1/zoom of the room and drawn scaled up by zoom (clicks are mapped back by
/// GTK), so nothing inside needs to know.
pub fn zoom_box(child: &impl IsA<gtk::Widget>) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    b.set_layout_manager(Some(zoom::ZoomLayout::new()));
    b.append(child);
    b
}

/// Set the zoom of a `zoom_box` (1.0 is actual size).
pub fn set_zoom(b: &gtk::Box, z: f64) {
    if let Some(l) = b.layout_manager().and_downcast::<zoom::ZoomLayout>() {
        l.set_zoom(z);
        b.queue_resize();
    }
}

mod zoom {
    use gtk::glib;
    use gtk::prelude::*;
    use gtk::subclass::prelude::*;
    use std::cell::Cell;

    mod imp {
        use super::*;

        pub struct ZoomLayout {
            pub zoom: Cell<f64>,
        }

        impl Default for ZoomLayout {
            fn default() -> Self {
                ZoomLayout { zoom: Cell::new(1.0) }
            }
        }

        #[glib::object_subclass]
        impl ObjectSubclass for ZoomLayout {
            const NAME: &'static str = "NxoZoomLayout";
            type Type = super::ZoomLayout;
            type ParentType = gtk::LayoutManager;
        }

        impl ObjectImpl for ZoomLayout {}

        fn child(w: &gtk::Widget) -> Option<gtk::Widget> {
            w.first_child().filter(|c| c.should_layout())
        }

        impl LayoutManagerImpl for ZoomLayout {
            fn request_mode(&self, widget: &gtk::Widget) -> gtk::SizeRequestMode {
                child(widget).map_or(gtk::SizeRequestMode::ConstantSize, |c| c.request_mode())
            }

            fn measure(&self, widget: &gtk::Widget, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
                let Some(c) = child(widget) else { return (0, 0, -1, -1) };
                let z = self.zoom.get();
                let for_child = if for_size < 0 { -1 } else { (for_size as f64 / z).floor() as i32 };
                let (min, nat, _, _) = c.measure(orientation, for_child);
                ((min as f64 * z).ceil() as i32, (nat as f64 * z).ceil() as i32, -1, -1)
            }

            fn allocate(&self, widget: &gtk::Widget, width: i32, height: i32, _baseline: i32) {
                let Some(c) = child(widget) else { return };
                let z = self.zoom.get();
                let w = ((width as f64 / z).floor() as i32).max(c.measure(gtk::Orientation::Horizontal, -1).0);
                let h = ((height as f64 / z).floor() as i32).max(c.measure(gtk::Orientation::Vertical, w).0);
                let t = gtk::gsk::Transform::new().scale(z as f32, z as f32);
                c.allocate(w, h, -1, Some(t));
            }
        }
    }

    glib::wrapper! {
        pub struct ZoomLayout(ObjectSubclass<imp::ZoomLayout>) @extends gtk::LayoutManager;
    }

    impl ZoomLayout {
        pub fn new() -> ZoomLayout {
            glib::Object::new()
        }

        pub fn set_zoom(&self, z: f64) {
            self.imp().zoom.set(z.clamp(0.25, 4.0));
        }
    }
}
