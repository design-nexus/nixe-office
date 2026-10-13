//! More ways out of a deck: each slide as a picture, handouts (six slides to a
//! page) and speaker-notes pages (a slide over its notes), on the paper chosen
//! in Settings.

use super::Present;
use super::model::*;
use super::render::{self, Hints, Pictures};
use crate::print::{Pages, Paper};
use crate::{doc, window};
use gtk::pango;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

/// Pictures are this many pixels wide (1920 × 1080).
const PICTURE_W: f64 = 1920.0;
const MARGIN: f64 = 42.0;

/// What a page needs to draw slides.
struct Slides {
    deck: Deck,
    media: BTreeMap<String, Vec<u8>>,
    pictures: Pictures,
    ctx: pango::Context,
    title: String,
}

impl Slides {
    /// Slide `i` scaled into the box (x, y, w), with a hairline frame.
    fn slide(&self, cr: &cairo::Context, i: usize, (x, y, w): (f64, f64, f64)) {
        let Some(s) = self.deck.composed(i) else { return };
        let scale = w / W;
        cr.save().ok();
        cr.translate(x, y);
        cr.scale(scale, scale);
        cr.rectangle(0.0, 0.0, W, H);
        cr.clip();
        render::draw(cr, &self.ctx, &s, &self.media, &self.pictures, (W, H), Hints::default());
        cr.restore().ok();
        cr.set_source_rgb(0.75, 0.77, 0.8);
        cr.set_line_width(0.5);
        cr.rectangle(x, y, w, w * H / W);
        cr.stroke().ok();
    }

    /// A line of small grey text: the deck's name on the left, the page number on the right.
    fn footer(&self, cr: &cairo::Context, (pw, ph): (f64, f64), page: usize) {
        let layout = pango::Layout::new(&self.ctx);
        layout.set_font_description(Some(&pango::FontDescription::from_string("Noto Sans 8")));
        cr.set_source_rgb(0.45, 0.47, 0.5);
        layout.set_text(&self.title);
        cr.move_to(MARGIN, ph - MARGIN * 0.75);
        pangocairo::functions::show_layout(cr, &layout);
        layout.set_text(&format!("{}", page + 1));
        let (tw, _) = layout.pixel_size();
        cr.move_to(pw - MARGIN - tw as f64, ph - MARGIN * 0.75);
        pangocairo::functions::show_layout(cr, &layout);
    }
}

/// Six slides to a portrait page, two across and three down.
struct Handouts(Slides);

impl Pages for Handouts {
    fn count(&self) -> usize {
        self.0.deck.slides.len().div_ceil(6).max(1)
    }
    fn size(&self) -> (f64, f64) {
        let p = Paper::preferred();
        (p.width, p.height)
    }
    fn draw(&self, cr: &cairo::Context, index: usize) {
        let (pw, ph) = self.size();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().ok();
        let gap = 24.0;
        let w = ((pw - 2.0 * MARGIN - gap) / 2.0).min((ph - 2.0 * MARGIN - 2.0 * gap - 20.0) / 3.0 * W / H);
        let h = w * H / W;
        let left = (pw - 2.0 * w - gap) / 2.0;
        let top = MARGIN + ((ph - 2.0 * MARGIN - 20.0) - (3.0 * h + 2.0 * gap)) / 2.0;
        for k in 0..6 {
            let i = index * 6 + k;
            if i >= self.0.deck.slides.len() {
                break;
            }
            let (col, row) = ((k % 2) as f64, (k / 2) as f64);
            self.0.slide(cr, i, (left + col * (w + gap), top + row * (h + gap), w));
        }
        self.0.footer(cr, (pw, ph), index);
    }
}

/// A slide on the top half of a portrait page, its speaker notes under it.
struct NotesPages(Slides);

impl Pages for NotesPages {
    fn count(&self) -> usize {
        self.0.deck.slides.len().max(1)
    }
    fn size(&self) -> (f64, f64) {
        let p = Paper::preferred();
        (p.width, p.height)
    }
    fn draw(&self, cr: &cairo::Context, index: usize) {
        let (pw, ph) = self.size();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().ok();
        let w = pw - 2.0 * MARGIN;
        self.0.slide(cr, index, (MARGIN, MARGIN, w));
        let notes = self.0.deck.slides.get(index).map(|s| s.notes.trim().to_string()).unwrap_or_default();
        let layout = pango::Layout::new(&self.0.ctx);
        layout.set_font_description(Some(&pango::FontDescription::from_string("Noto Sans 11")));
        layout.set_width((w * pango::SCALE as f64) as i32);
        layout.set_wrap(pango::WrapMode::WordChar);
        layout.set_height(((ph - MARGIN * 2.5 - w * H / W - 24.0) * pango::SCALE as f64) as i32);
        layout.set_ellipsize(pango::EllipsizeMode::End);
        let (text, colour) = if notes.is_empty() { ("No notes for this slide.", 0.55) } else { (notes.as_str(), 0.13) };
        layout.set_text(text);
        cr.set_source_rgb(colour, colour, colour + 0.03);
        cr.move_to(MARGIN, MARGIN + w * H / W + 24.0);
        pangocairo::functions::show_layout(cr, &layout);
        self.0.footer(cr, (pw, ph), index);
    }
}

/// Write each slide as a PNG in `dir`: "<name> 01.png" and on. Returns how many.
fn write_pictures(slides: &Slides, dir: &Path, stem: &str) -> anyhow::Result<usize> {
    let scale = PICTURE_W / W;
    let n = slides.deck.slides.len();
    let digits = n.to_string().len().max(2);
    for i in 0..n {
        let surface = cairo::ImageSurface::create(cairo::Format::Rgb24, PICTURE_W as i32, (H * scale) as i32)?;
        let cr = cairo::Context::new(&surface)?;
        cr.scale(scale, scale);
        if let Some(s) = slides.deck.composed(i) {
            render::draw(&cr, &slides.ctx, &s, &slides.media, &slides.pictures, (W, H), Hints::default());
        }
        drop(cr);
        let mut bytes = Vec::new();
        surface.write_to_png(&mut bytes)?;
        crate::cmd::atomic_write(&dir.join(format!("{stem} {:0digits$}.png", i + 1)), bytes)?;
    }
    Ok(n)
}

impl Present {
    fn slides_now(&self) -> Slides {
        self.end_edit();
        Slides {
            deck: self.deck.borrow().clone(),
            media: self.media.borrow().clone(),
            pictures: Pictures::default(),
            ctx: crate::print::text_context(),
            title: doc::stem(),
        }
    }

    /// The file menu's extra ways out.
    pub(super) fn export_items(self: &Rc<Self>) -> Vec<doc::MenuItem> {
        let me = Rc::downgrade(self);
        let pictures: doc::Action = Rc::new(move || {
            if let Some(p) = me.upgrade() {
                p.export_pictures();
            }
        });
        let me = Rc::downgrade(self);
        let handouts: doc::Action = Rc::new(move || {
            let me = me.clone();
            doc::export_pages_dialog("Export handouts", " handouts", move || match me.upgrade() {
                Some(p) => Rc::new(Handouts(p.slides_now())),
                None => Rc::new(Handouts(empty())),
            });
        });
        let me = Rc::downgrade(self);
        let notes: doc::Action = Rc::new(move || {
            let me = me.clone();
            doc::export_pages_dialog("Export with speaker notes", " notes", move || match me.upgrade() {
                Some(p) => Rc::new(NotesPages(p.slides_now())),
                None => Rc::new(NotesPages(empty())),
            });
        });
        vec![
            ("Export slides as pictures…", pictures),
            ("Export handouts (PDF, six to a page)…", handouts),
            ("Export with speaker notes (PDF)…", notes),
        ]
    }

    /// Ask for a folder, then put a picture of each slide in it.
    fn export_pictures(self: &Rc<Self>) {
        let dialog = gtk::FileDialog::builder().title("Export slides as pictures: choose a folder").modal(true).build();
        let me = Rc::downgrade(self);
        dialog.select_folder(window::window().as_ref(), gio::Cancellable::NONE, move |res| {
            let Some(dir) = res.ok().and_then(|f| f.path()) else { return };
            let Some(p) = me.upgrade() else { return };
            let slides = p.slides_now();
            let stem = slides.title.clone();
            // A moment's work for a big deck: let the click finish drawing first.
            glib::idle_add_local_once(move || match write_pictures(&slides, &dir, &stem) {
                Ok(n) => window::toast(&format!("Saved {} in {}.", crate::fmt::count(n, "picture", "pictures"), crate::paths::pretty(&dir))),
                Err(e) => window::toast(&format!("Couldn't save the pictures: {e:#}")),
            });
        });
    }
}

fn empty() -> Slides {
    Slides { deck: Deck::default(), media: BTreeMap::new(), pictures: Pictures::default(), ctx: crate::print::text_context(), title: String::new() }
}

/// Handouts and notes pages of a template, for checking by eye.
#[cfg(test)]
pub fn write_samples(dir: &Path) {
    let deck = super::templates::build("trip-plan").expect("a template");
    let s = || Slides { deck: deck.clone(), title: "Trip plan".into(), ..empty() };
    crate::print::export_pdf(&Handouts(s()), &dir.join("handouts.pdf")).expect("handouts");
    crate::print::export_pdf(&NotesPages(s()), &dir.join("notes.pdf")).expect("notes");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slides(n: usize) -> Slides {
        let mut deck = Deck::default();
        while deck.slides.len() < n {
            deck.slides.push(Slide::default());
        }
        deck.slides[0].notes = "Say hello.".into();
        Slides { deck, title: "Talk".into(), ..empty() }
    }

    #[test]
    fn pages_and_pictures() {
        assert_eq!(Handouts(slides(7)).count(), 2);
        assert_eq!(NotesPages(slides(7)).count(), 7);
        let dir = std::env::temp_dir().join(format!("nxo-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        crate::print::export_pdf(&Handouts(slides(7)), &dir.join("h.pdf")).unwrap();
        crate::print::export_pdf(&NotesPages(slides(2)), &dir.join("n.pdf")).unwrap();
        assert_eq!(write_pictures(&slides(3), &dir, "Talk").unwrap(), 3);
        let png = std::fs::read(dir.join("Talk 03.png")).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        // 1920 × 1080.
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 1920);
        assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 1080);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
