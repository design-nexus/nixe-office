//! Pictures with text wrapped beside them, on screen: the page view draws the
//! picture at the side of its paragraph, and the paragraph gets a margin (and,
//! if it's short, room below) so its text stays clear of it.

use super::Writer;
use super::buffer;
use super::model::Side;
use super::picture::PX;
use super::view;
use crate::doc;
use gtk::prelude::*;

/// Room between a wrapped picture and the text beside it, in points.
const GAP: f64 = 12.0;
const WRAP_TAGS: [&str; 3] = ["wl:", "wr:", "wb:"];

impl Writer {
    /// Give each paragraph with a wrapped picture room for it.
    pub(super) fn apply_wraps(&self) {
        let b = self.tb();
        let pics = view::wrapped_pictures(b);
        // What each line had, before it's worked out again.
        let old_below = |line: i32| -> Option<i32> {
            let it = b.iter_at_line(line)?;
            it.tags().iter().filter_map(|t| t.name()?.strip_prefix("wb:")?.parse().ok()).next()
        };
        let olds: Vec<Option<i32>> = pics.iter().map(|(l, _)| old_below(*l)).collect();
        let was = self.loading.replace(true);
        let has_any = { let mut any = false; b.tag_table().foreach(|t| any |= t.name().is_some_and(|n| WRAP_TAGS.iter().any(|p| n.starts_with(p)))); any };
        if has_any {
            buffer::remove_char_tags(b, &b.start_iter(), &b.end_iter(), &WRAP_TAGS);
        }
        for ((line, pic), old) in pics.into_iter().zip(olds) {
            let (s, e) = buffer::line_range(b, line, line);
            let props = buffer::para_props(b, &s);
            let w = (pic.width_pt() * PX).round() as i32;
            let gap = (GAP * PX).round() as i32;
            let name = match pic.wrap() {
                Some(Side::Right) => format!("wr:{}", w + gap),
                _ => format!("wl:{}", props.margin_px() + w + gap),
            };
            b.apply_tag(&buffer::tag(b, &name), &s, &e);
            // A short paragraph still ends below the picture.
            let normal = (props.style.look().2 * PX).round() as i32;
            let normal = normal.max(self.view.pixels_below_lines());
            let (_, h) = self.view.line_yrange(&s);
            let text_h = h - old.unwrap_or(normal);
            let need = (pic.height_pt() * PX).round() as i32 + 8 - text_h;
            if need > normal {
                b.apply_tag(&buffer::tag(b, &format!("wb:{need}")), &s, &e);
            }
        }
        self.loading.set(was);
    }

    /// In line, or wrapped at the left or right of its paragraph.
    pub(super) fn set_picture_wrap(&self, wrap: Option<Side>) {
        let Some(pic) = self.picture_at_cursor() else { return };
        if pic.wrap() == wrap {
            return;
        }
        self.checkpoint();
        pic.set_wrap(wrap);
        self.view.queue_resize();
        self.apply_wraps();
        // Once laid out, the room below can be worked out properly.
        let me = self.me.borrow().clone();
        gtk::glib::idle_add_local_once(move || {
            if let Some(w) = me.upgrade() {
                w.apply_wraps();
            }
        });
        doc::edited();
        self.schedule_pages();
        self.view.queue_draw();
    }
}
