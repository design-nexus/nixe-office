//! A picture in the text: a texture shown at the width the user chose, carrying
//! its file name under `media/` so the text can be saved back.

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use std::cell::{Cell, RefCell};

/// Screen pixels per point.
pub const PX: f64 = 96.0 / 72.0;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Picture {
        pub texture: RefCell<Option<gdk::Texture>>,
        pub name: RefCell<String>,
        /// Width on the page, in points.
        pub width: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Picture {
        const NAME: &'static str = "NxoPicture";
        type Type = super::Picture;
        type Interfaces = (gdk::Paintable,);
    }

    impl ObjectImpl for Picture {}

    impl PaintableImpl for Picture {
        fn intrinsic_width(&self) -> i32 {
            (self.width.get() * PX).round() as i32
        }
        fn intrinsic_height(&self) -> i32 {
            (self.obj().height_pt() * PX).round() as i32
        }
        fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            if let Some(t) = self.texture.borrow().as_ref() {
                t.snapshot(snapshot, width, height);
            }
        }
    }
}

glib::wrapper! {
    pub struct Picture(ObjectSubclass<imp::Picture>) @implements gdk::Paintable;
}

impl Picture {
    pub fn new(name: &str, bytes: &[u8], width_pt: f64) -> Option<Picture> {
        let texture = gdk::Texture::from_bytes(&glib::Bytes::from(bytes)).ok()?;
        let p: Picture = glib::Object::new();
        p.imp().texture.replace(Some(texture));
        p.imp().name.replace(name.to_string());
        p.imp().width.set(width_pt.max(16.0));
        Some(p)
    }

    pub fn name(&self) -> String {
        self.imp().name.borrow().clone()
    }

    pub fn width_pt(&self) -> f64 {
        self.imp().width.get()
    }

    pub fn height_pt(&self) -> f64 {
        let (w, h) = self.pixel_size();
        if w == 0 { 0.0 } else { self.width_pt() * h as f64 / w as f64 }
    }

    pub fn pixel_size(&self) -> (i32, i32) {
        self.imp().texture.borrow().as_ref().map(|t| (t.width(), t.height())).unwrap_or((0, 0))
    }

    pub fn set_width_pt(&self, w: f64) {
        self.imp().width.set(w.max(16.0));
        self.invalidate_size();
    }
}

/// The picture's natural width in points, shrunk to fit `max` (the text width).
pub fn fitting_width(pixel_width: i32, max: f64) -> f64 {
    (pixel_width as f64 / PX).min(max).max(16.0)
}
