//! The text view on the page. It draws each list item's bullet or number in the
//! paragraph's left margin, so the markers are never part of the text.

use super::buffer::{ParaProps, para_props};
use super::model::{INDENT_STEP, List, list_label, numbering};
use super::picture::PX;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct PageView;

    #[glib::object_subclass]
    impl ObjectSubclass for PageView {
        const NAME: &'static str = "NxoPageView";
        type Type = super::PageView;
        type ParentType = gtk::TextView;
    }

    impl ObjectImpl for PageView {}
    impl WidgetImpl for PageView {}

    impl TextViewImpl for PageView {
        fn snapshot_layer(&self, layer: gtk::TextViewLayer, snapshot: gtk::Snapshot) {
            self.parent_snapshot_layer(layer, snapshot.clone());
            if layer == gtk::TextViewLayer::BelowText {
                self.obj().draw_markers(&snapshot);
            }
        }
    }
}

glib::wrapper! {
    pub struct PageView(ObjectSubclass<imp::PageView>)
        @extends gtk::TextView, gtk::Widget,
        @implements gtk::Accessible, gtk::AccessibleText, gtk::Buildable, gtk::ConstraintTarget, gtk::Scrollable;
}

impl PageView {
    pub fn new(buffer: &impl IsA<gtk::TextBuffer>) -> PageView {
        glib::Object::builder().property("buffer", buffer).build()
    }

    fn draw_markers(&self, snapshot: &gtk::Snapshot) {
        let buffer = self.buffer();
        let lines = buffer.line_count();
        // Numbers depend on every list item before, so walk the whole text once;
        // a home document is short enough for that.
        let props: Vec<ParaProps> =
            (0..lines).map(|l| para_props(&buffer, &buffer.iter_at_line(l).unwrap_or_else(|| buffer.start_iter()))).collect();
        if !props.iter().any(|p| p.list.is_some()) {
            return;
        }
        let numbers = numbering(&props.iter().map(|p| (p.list, p.indent)).collect::<Vec<_>>());
        let visible = self.visible_rect();
        let ink = gtk::gdk::RGBA::parse("#202329").unwrap_or(gtk::gdk::RGBA::BLACK);
        for (line, p) in props.iter().enumerate() {
            let Some(list) = p.list else { continue };
            // The empty line after the final newline isn't a paragraph.
            if line as i32 == lines - 1 {
                continue;
            }
            let Some(start) = buffer.iter_at_line(line as i32) else { continue };
            let loc = self.iter_location(&start);
            if loc.y() + loc.height() < visible.y() || loc.y() > visible.y() + visible.height() {
                continue;
            }
            let label = list_label(list, p.indent, numbers[line]);
            let layout = self.create_pango_layout(Some(&label));
            // The marker takes the size of the text it introduces.
            let size = super::buffer::char_format(&start).size.unwrap_or_else(|| p.style.look().0);
            let mut font = gtk::pango::FontDescription::from_string(super::model::DEFAULT_FONT);
            font.set_size((size * gtk::pango::SCALE as f64) as i32);
            layout.set_font_description(Some(&font));
            let (w, h) = layout.pixel_size();
            let text_x = ((p.indent as f64 + 1.0) * INDENT_STEP * PX) as i32;
            let x = text_x - w - (0.35 * INDENT_STEP * PX) as i32;
            let x = if list == List::Bullet { text_x - (0.62 * INDENT_STEP * PX) as i32 } else { x };
            let y = loc.y() + (loc.height() - h).max(0);
            snapshot.save();
            snapshot.translate(&gtk::graphene::Point::new(x as f32, y as f32));
            snapshot.append_layout(&layout, &ink);
            snapshot.restore();
        }
    }
}
