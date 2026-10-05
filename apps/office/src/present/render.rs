//! Drawing a slide. The canvas, the thumbnails, the slideshow, the presenter
//! view and the PDF all draw through here, so they always match.

use super::model::*;
use gtk::pango;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

/// Decoded pictures, by media name.
#[derive(Default)]
pub struct Pictures {
    cache: RefCell<HashMap<String, Option<cairo::ImageSurface>>>,
}

impl Pictures {
    pub fn clear(&self) {
        self.cache.borrow_mut().clear();
    }

    pub fn get(&self, name: &str, media: &BTreeMap<String, Vec<u8>>) -> Option<cairo::ImageSurface> {
        if let Some(s) = self.cache.borrow().get(name) {
            return s.clone();
        }
        let s = media.get(name).and_then(|b| cairo::ImageSurface::create_from_png(&mut std::io::Cursor::new(b)).ok());
        self.cache.borrow_mut().insert(name.to_string(), s.clone());
        s
    }
}

pub fn rgb(hex: &str) -> (f64, f64, f64) {
    let h = hex.trim_start_matches('#');
    let p = |i: usize| h.get(i..i + 2).and_then(|x| u8::from_str_radix(x, 16).ok()).unwrap_or(0) as f64 / 255.0;
    (p(0), p(2), p(4))
}

/// What to draw besides the slide itself.
#[derive(Clone, Copy, Default)]
pub struct Hints {
    /// Show "Click to add a title" in empty boxes (the editor only).
    pub placeholders: bool,
    /// Leave out the title or body (it's being edited on top).
    pub skip_title: bool,
    pub skip_body: bool,
    /// Leave out this text object.
    pub skip_object: Option<u32>,
    /// In the slideshow: how many build steps are showing (None shows everything).
    pub built: Option<usize>,
    /// How far the latest build step has come in, 0 to 1.
    pub anim: Option<f64>,
}

/// How a block of slide text is set.
#[derive(Clone, Copy)]
struct Look {
    size: f64,
    title: bool,
    /// A placeholder hint, drawn faint.
    faint: bool,
    /// From the top of its box (otherwise centred down it).
    top: bool,
}

fn text_box(cr: &cairo::Context, ctx: &pango::Context, s: &Slide, text: &str, frame: Frame, look: Look) {
    let Look { size, title, faint, top } = look;
    let (x, y, w, h, _, centred) = frame;
    let layout = pango::Layout::new(ctx);
    let mut font = pango::FontDescription::from_string(&s.font);
    font.set_size((size * pango::SCALE as f64) as i32);
    if title || s.bold {
        font.set_weight(if s.bold { pango::Weight::Bold } else { pango::Weight::Semibold });
    }
    if s.italic {
        font.set_style(pango::Style::Italic);
    }
    layout.set_font_description(Some(&font));
    layout.set_width((w * pango::SCALE as f64) as i32);
    layout.set_wrap(pango::WrapMode::WordChar);
    layout.set_alignment(if centred { pango::Alignment::Center } else { pango::Alignment::Left });
    layout.set_spacing((size * 0.25 * pango::SCALE as f64) as i32);
    layout.set_text(text);
    let (_, th) = layout.size();
    let th = th as f64 / pango::SCALE as f64;
    let (r, g, b) = rgb(&s.color);
    cr.save().ok();
    cr.rectangle(x - 4.0, y - 4.0, w + 8.0, h + 8.0);
    cr.clip();
    if faint {
        cr.set_source_rgba(r, g, b, 0.35);
    } else {
        cr.set_source_rgb(r, g, b);
    }
    let ty = if top { y } else { y + ((h - th) / 2.0).max(0.0) };
    cr.move_to(x, ty);
    pangocairo::functions::show_layout(cr, &layout);
    cr.restore().ok();
}

/// Draw `slide` filling (0, 0, w, h). `ctx` is a 72-dpi context
/// (`print::text_context`), so text is sized in the slide's points.
pub fn draw(
    cr: &cairo::Context,
    ctx: &pango::Context,
    slide: &Slide,
    media: &BTreeMap<String, Vec<u8>>,
    pictures: &Pictures,
    (w, h): (f64, f64),
    hints: Hints,
) {
    cr.save().ok();
    cr.scale(w / W, h / H);
    let (r, g, b) = rgb(&slide.background);
    cr.set_source_rgb(r, g, b);
    cr.rectangle(0.0, 0.0, W, H);
    cr.fill().ok();

    if let Some(f) = title_frame(slide.layout)
        && !hints.skip_title
    {
        let size = slide.title_size.unwrap_or(f.4);
        if !slide.title.is_empty() {
            text_box(cr, ctx, slide, &slide.title, f, Look { size, title: true, faint: false, top: false });
        } else if hints.placeholders {
            text_box(cr, ctx, slide, "Click to add a title", f, Look { size, title: true, faint: true, top: false });
        }
    }
    if let Some(f) = body_frame(slide.layout)
        && !hints.skip_body
    {
        let size = slide.body_size.unwrap_or(f.4);
        let top = slide.layout == Layout::Content;
        if !slide.body.is_empty() {
            text_box(cr, ctx, slide, &slide.body, f, Look { size, title: false, faint: false, top });
        } else if hints.placeholders {
            let hint = if top { "Click to add points" } else { "Click to add a subtitle" };
            text_box(cr, ctx, slide, hint, f, Look { size, title: false, faint: true, top });
        }
    }
    let steps = slide.build_steps();
    let (hidden, moving): (&[u32], Option<u32>) = match hints.built {
        Some(n) => (&steps[n.min(steps.len())..], n.checked_sub(1).and_then(|i| steps.get(i)).copied()),
        None => (&[], None),
    };
    for o in &slide.objects {
        if hidden.contains(&o.id) {
            continue;
        }
        // The build step coming in: faded or flown in part way.
        let anim = match (hints.anim, moving == Some(o.id), o.build) {
            (Some(t), true, Some(b)) if t < 1.0 && b.effect != Effect::Appear => Some((t, b.effect)),
            _ => None,
        };
        if let Some((t, effect)) = anim {
            cr.save().ok();
            if effect == Effect::Fly {
                let ease = 1.0 - (1.0 - t).powi(3);
                cr.translate(0.0, (1.0 - ease) * (H - o.y + 20.0));
            } else {
                cr.push_group();
            }
        }
        draw_object(cr, ctx, slide, o, media, pictures, hints);
        if let Some((t, effect)) = anim {
            if effect != Effect::Fly {
                cr.pop_group_to_source().ok();
                cr.paint_with_alpha(t).ok();
            }
            cr.restore().ok();
        }
    }
    cr.restore().ok();
}

/// Trace a shape's outline in the box (x, y, w, h).
pub fn shape_path(cr: &cairo::Context, shape: Shape, (x, y, w, h): (f64, f64, f64, f64)) {
    let poly = |pts: &[(f64, f64)]| {
        cr.new_sub_path();
        for (i, (px, py)) in pts.iter().enumerate() {
            if i == 0 {
                cr.move_to(x + px * w, y + py * h);
            } else {
                cr.line_to(x + px * w, y + py * h);
            }
        }
        cr.close_path();
    };
    match shape {
        Shape::Rectangle => cr.rectangle(x, y, w, h),
        Shape::Rounded => rounded(cr, x, y, w, h, w.min(h) * 0.16),
        Shape::Ellipse => {
            cr.save().ok();
            cr.translate(x + w / 2.0, y + h / 2.0);
            cr.scale((w / 2.0).max(0.01), (h / 2.0).max(0.01));
            cr.new_sub_path();
            cr.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
            cr.restore().ok();
        }
        Shape::Triangle => poly(&[(0.5, 0.0), (1.0, 1.0), (0.0, 1.0)]),
        Shape::Diamond => poly(&[(0.5, 0.0), (1.0, 0.5), (0.5, 1.0), (0.0, 0.5)]),
        Shape::Hexagon => poly(&[(0.25, 0.0), (0.75, 0.0), (1.0, 0.5), (0.75, 1.0), (0.25, 1.0), (0.0, 0.5)]),
        Shape::Arrow => {
            // The head is as long as the arrow is tall, up to half its length.
            let head = 1.0 - (h / w.max(1.0)).min(0.5);
            poly(&[(0.0, 0.25), (head, 0.25), (head, 0.0), (1.0, 0.5), (head, 1.0), (head, 0.75), (0.0, 0.75)]);
        }
        Shape::Star => {
            let pts: Vec<(f64, f64)> = (0..10)
                .map(|i| {
                    let r = if i % 2 == 0 { 0.5 } else { 0.5 * 0.4 };
                    let a = (-90.0 + i as f64 * 36.0f64).to_radians();
                    (0.5 + r * a.cos(), 0.5 + r * a.sin() * 1.05 + 0.03)
                })
                .collect();
            poly(&pts);
        }
    }
}

/// The box an object's border and shadow follow: a picture's is the picture as fitted.
fn object_path(cr: &cairo::Context, o: &Object, media: &BTreeMap<String, Vec<u8>>, pictures: &Pictures) {
    match &o.kind {
        Kind::Shape { shape, .. } => shape_path(cr, *shape, (o.x, o.y, o.w, o.h)),
        Kind::Image { image } => match pictures.get(image, media) {
            Some(pic) => {
                let (pw, ph) = (pic.width().max(1) as f64, pic.height().max(1) as f64);
                let scale = (o.w / pw).min(o.h / ph);
                let (dw, dh) = (pw * scale, ph * scale);
                cr.rectangle(o.x + (o.w - dw) / 2.0, o.y + (o.h - dh) / 2.0, dw, dh);
            }
            None => cr.rectangle(o.x, o.y, o.w, o.h),
        },
        Kind::Chart { .. } => rounded(cr, o.x, o.y, o.w, o.h, 8.0),
        _ => cr.rectangle(o.x, o.y, o.w, o.h),
    }
}

fn draw_object(
    cr: &cairo::Context,
    ctx: &pango::Context,
    slide: &Slide,
    o: &Object,
    media: &BTreeMap<String, Vec<u8>>,
    pictures: &Pictures,
    hints: Hints,
) {
    cr.save().ok();
    if o.rotation != 0.0 {
        let (cx, cy) = o.center();
        cr.translate(cx, cy);
        cr.rotate(o.rotation.to_radians());
        cr.translate(-cx, -cy);
    }
    if o.shadow && hints.skip_object != Some(o.id) {
        // A soft drop shadow, down and to the right.
        cr.save().ok();
        if matches!(o.kind, Kind::Text { .. }) && o.outline.is_none() {
            // Text without a box: the letters cast it.
            cr.translate(2.0, 3.0);
            cr.push_group();
            let dark = Slide { color: "#000000".into(), ..slide.clone() };
            draw_content(cr, ctx, &dark, o, media, pictures, Hints { placeholders: false, ..hints });
            cr.pop_group_to_source().ok();
            cr.paint_with_alpha(0.35).ok();
        } else {
            for (i, alpha) in [0.10, 0.08, 0.06].iter().enumerate() {
                cr.save().ok();
                let d = 3.0 + i as f64 * 2.0;
                cr.translate(d * 0.6, d);
                object_path(cr, o, media, pictures);
                cr.set_source_rgba(0.0, 0.0, 0.0, *alpha);
                cr.fill().ok();
                cr.restore().ok();
            }
        }
        cr.restore().ok();
    }
    draw_content(cr, ctx, slide, o, media, pictures, hints);
    if let Some(line) = &o.outline
        && line.width > 0.0
    {
        object_path(cr, o, media, pictures);
        let (r, g, b) = rgb(&line.color);
        cr.set_source_rgb(r, g, b);
        cr.set_line_width(line.width);
        cr.set_line_join(cairo::LineJoin::Round);
        cr.stroke().ok();
    }
    cr.restore().ok();
}

/// The object itself: fill, picture, text, table or chart.
fn draw_content(
    cr: &cairo::Context,
    ctx: &pango::Context,
    slide: &Slide,
    o: &Object,
    media: &BTreeMap<String, Vec<u8>>,
    pictures: &Pictures,
    hints: Hints,
) {
    {
        match &o.kind {
            Kind::Shape { shape, fill } => {
                let (r, g, b) = rgb(fill);
                cr.set_source_rgb(r, g, b);
                shape_path(cr, *shape, (o.x, o.y, o.w, o.h));
                cr.fill().ok();
            }
            Kind::Image { image } => {
                let Some(pic) = pictures.get(image, media) else {
                    cr.set_source_rgba(0.5, 0.5, 0.5, 0.25);
                    cr.rectangle(o.x, o.y, o.w, o.h);
                    cr.fill().ok();
                    return;
                };
                let (pw, ph) = (pic.width().max(1) as f64, pic.height().max(1) as f64);
                // Fit inside the box, keeping its shape.
                let scale = (o.w / pw).min(o.h / ph);
                let (dw, dh) = (pw * scale, ph * scale);
                cr.save().ok();
                cr.translate(o.x + (o.w - dw) / 2.0, o.y + (o.h - dh) / 2.0);
                cr.scale(scale, scale);
                cr.set_source_surface(&pic, 0.0, 0.0).ok();
                cr.source().set_filter(cairo::Filter::Good);
                cr.paint().ok();
                cr.restore().ok();
            }
            Kind::Text { text } => {
                if hints.skip_object == Some(o.id) {
                    return;
                }
                let size = slide.body_size.unwrap_or(24.0);
                let frame = (o.x + 6.0, o.y + 4.0, o.w - 12.0, o.h - 8.0, size, false);
                if text.is_empty() && hints.placeholders {
                    text_box(cr, ctx, slide, "Text", frame, Look { size, title: false, faint: true, top: false });
                } else {
                    text_box(cr, ctx, slide, text, frame, Look { size, title: false, faint: false, top: false });
                }
            }
            Kind::Table { rows, header } => draw_table(cr, ctx, slide, o, rows, *header),
            Kind::Chart { chart, names, labels, series } => {
                // Charts keep their own colours, on a white card.
                cr.set_source_rgb(1.0, 1.0, 1.0);
                rounded(cr, o.x, o.y, o.w, o.h, 8.0);
                cr.fill().ok();
                crate::sheets::chart::draw(
                    cr,
                    ctx,
                    *chart,
                    &crate::sheets::chart::Data { names, labels, series },
                    (o.x + 10.0, o.y + 10.0, o.w - 20.0, o.h - 20.0),
                );
            }
        }
    }
}

fn rounded(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    cr.arc(x + r, y + r, r, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
    cr.close_path();
}

/// A table in the slide's own font and colour: hairlines in the text colour,
/// the heading row tinted.
fn draw_table(cr: &cairo::Context, ctx: &pango::Context, slide: &Slide, o: &Object, rows: &[Vec<String>], header: bool) {
    let n_rows = rows.len().max(1);
    let n_cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let (cw, rh) = (o.w / n_cols as f64, o.h / n_rows as f64);
    let (r, g, b) = rgb(&slide.color);
    if header {
        cr.set_source_rgba(r, g, b, 0.12);
        cr.rectangle(o.x, o.y, o.w, rh);
        cr.fill().ok();
    }
    cr.set_source_rgba(r, g, b, 0.35);
    cr.set_line_width(1.0);
    for i in 0..=n_rows {
        let y = o.y + i as f64 * rh;
        cr.move_to(o.x, y);
        cr.line_to(o.x + o.w, y);
    }
    for j in 0..=n_cols {
        let x = o.x + j as f64 * cw;
        cr.move_to(x, o.y);
        cr.line_to(x, o.y + o.h);
    }
    cr.stroke().ok();
    let size = (rh * 0.42).clamp(9.0, slide.body_size.unwrap_or(24.0).min(24.0));
    let layout = pango::Layout::new(ctx);
    for (i, row) in rows.iter().enumerate() {
        for (j, cell) in row.iter().enumerate() {
            if cell.is_empty() {
                continue;
            }
            let mut font = pango::FontDescription::from_string(&slide.font);
            font.set_size((size * pango::SCALE as f64) as i32);
            if header && i == 0 {
                font.set_weight(pango::Weight::Bold);
            }
            layout.set_font_description(Some(&font));
            layout.set_width(((cw - 12.0).max(1.0) * pango::SCALE as f64) as i32);
            layout.set_ellipsize(pango::EllipsizeMode::End);
            layout.set_text(cell);
            let th = layout.pixel_size().1 as f64;
            let (x, y) = (o.x + j as f64 * cw + 6.0, o.y + i as f64 * rh);
            cr.save().ok();
            cr.rectangle(x - 6.0, y, cw, rh);
            cr.clip();
            cr.set_source_rgb(r, g, b);
            cr.move_to(x, y + (rh - th) / 2.0);
            pangocairo::functions::show_layout(cr, &layout);
            cr.restore().ok();
        }
    }
}

/// The deck as pages: one 960 × 540 pt page per slide.
pub struct DeckPages {
    pub deck: Deck,
    pub media: BTreeMap<String, Vec<u8>>,
    pub pictures: Pictures,
    pub ctx: pango::Context,
}

impl crate::print::Pages for DeckPages {
    fn count(&self) -> usize {
        self.deck.slides.len()
    }
    fn size(&self) -> (f64, f64) {
        (W, H)
    }
    fn draw(&self, cr: &cairo::Context, index: usize) {
        if let Some(s) = self.deck.slides.get(index) {
            draw(cr, &self.ctx, s, &self.media, &self.pictures, (W, H), Hints::default());
        }
    }
}
