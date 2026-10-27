//! Nyx-style building blocks: pages, groups and option rows.

pub use nyx_ui::widgets::*;

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
