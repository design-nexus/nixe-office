//! The slideshow: slides full screen on black. With a second screen and the
//! presenter view on, the slides go to the other screen and this one shows the
//! current and next slide, the notes and a timer.

use super::Present;
use super::model::*;
use super::render::{self, Hints, Pictures};
use crate::{prefs, widgets, window};
use gtk::prelude::*;
use gtk::{gdk, glib};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Instant;

struct Show {
    editor: Rc<Present>,
    deck: Deck,
    media: BTreeMap<String, Vec<u8>>,
    pictures: Pictures,
    ctx: gtk::pango::Context,
    index: Cell<usize>,
    /// How many build steps of the current slide are showing.
    built: Cell<usize>,
    /// What's moving, and since when.
    anim: Cell<Option<(Anim, Instant)>>,
    /// Past the last slide: a black screen, and the next step leaves.
    ended: Cell<bool>,
    started: Instant,
    windows: RefCell<Vec<gtk::Window>>,
    areas: RefCell<Vec<gtk::DrawingArea>>,
    readout: gtk::Label,
    timer: gtk::Label,
    notes: gtk::Label,
    closing: Cell<bool>,
    /// The slides' own window: its area, and the players laid over it.
    stage: gtk::Overlay,
    stage_area: RefCell<Option<gtk::DrawingArea>>,
    /// Video and audio on the slide showing: the widget and where it goes.
    players: RefCell<Vec<(gtk::Widget, Object)>>,
    /// A slide number being typed, to go to with Enter.
    typed: RefCell<String>,
    /// The laser pointer is on (L), and where it is on the slide, in points.
    laser: Cell<bool>,
    pointer: Cell<Option<(f64, f64)>>,
}

/// Where a slide sits letterboxed in a `w` × `h` area: (x, y, scale).
fn letterbox(w: f64, h: f64) -> (f64, f64, f64) {
    let scale = (w / W).min(h / H);
    ((w - W * scale) / 2.0, (h - H * scale) / 2.0, scale)
}

#[derive(Clone, Copy)]
enum Anim {
    /// The latest build step coming in.
    Build,
    /// The slide coming in over slide `from`.
    Transition { from: usize, t: Transition },
}

impl Anim {
    fn secs(self) -> f64 {
        match self {
            Anim::Build => 0.5,
            Anim::Transition { t, .. } => t.secs,
        }
    }
}

impl Show {
    /// Stop and take away the players of the slide that was showing.
    fn clear_players(&self) {
        for (w, _) in self.players.borrow_mut().drain(..) {
            if let Some(v) = w.downcast_ref::<gtk::Video>()
                && let Some(m) = v.media_stream()
            {
                m.pause();
            }
            if let Some(c) = w.first_child().and_then(|c| c.downcast::<gtk::MediaControls>().ok())
                && let Some(m) = c.media_stream()
            {
                m.pause();
            }
            self.stage.remove_overlay(&w);
        }
    }

    /// Put players over the slide's video and audio (those built in so far).
    fn load_players(&self) {
        self.clear_players();
        let i = self.index.get();
        let Some(slide) = self.deck.composed(i).filter(|_| !self.ended.get()) else { return };
        let hidden = slide.hidden_after(self.built.get());
        for o in slide.objects.iter().filter(|o| !hidden.contains(&o.id)) {
            let Kind::Media { media, video } = &o.kind else { continue };
            let Some(path) = self.media.get(media).and_then(|b| media_file(media, b)) else { continue };
            let widget: gtk::Widget = if *video {
                let v = gtk::Video::for_filename(Some(&path));
                v.set_autoplay(false);
                v.add_css_class("slide-video");
                v.upcast()
            } else {
                let file = gtk::MediaFile::for_filename(&path);
                let controls = gtk::MediaControls::new(Some(&file));
                controls.set_valign(gtk::Align::Center);
                controls.set_hexpand(true);
                let b = widgets::hbox(0);
                b.add_css_class("slide-audio");
                b.append(&controls);
                b.upcast()
            };
            widget.set_halign(gtk::Align::Start);
            widget.set_valign(gtk::Align::Start);
            self.stage.add_overlay(&widget);
            self.players.borrow_mut().push((widget, o.clone()));
        }
        self.place_players();
        // The pointer shows while there's something to play.
        if let Some(w) = self.windows.borrow().first() {
            w.set_cursor_from_name(Some(if self.players.borrow().is_empty() { "none" } else { "default" }));
        }
    }

    /// Size and place the players over their spots on the slide.
    fn place_players(&self) {
        let Some(area) = self.stage_area.borrow().clone() else { return };
        let (w, h) = (area.width() as f64, area.height() as f64);
        let scale = (w / W).min(h / H);
        let (ox, oy) = ((w - W * scale) / 2.0, (h - H * scale) / 2.0);
        for (widget, o) in self.players.borrow().iter() {
            widget.set_margin_start((ox + o.x * scale).round() as i32);
            widget.set_margin_top((oy + o.y * scale).round() as i32);
            widget.set_size_request((o.w * scale).round().max(40.0) as i32, (o.h * scale).round().max(24.0) as i32);
        }
    }

    fn builds(&self, i: usize) -> usize {
        self.deck.slides.get(i).map(|s| s.steps().len()).unwrap_or(0)
    }

    /// Forward: the next build on this slide, else the next slide. Back: undo
    /// the last build, else the slide before, fully built.
    fn step(self: &Rc<Self>, by: i64) {
        let n = self.deck.slides.len();
        let i = self.index.get();
        self.anim.set(None);
        if self.ended.get() {
            if by > 0 {
                self.stop();
                return;
            }
            self.ended.set(false);
        } else if by > 0 {
            if self.built.get() < self.builds(i) {
                self.built.set(self.built.get() + 1);
                let effect = self.deck.slides[i].steps().get(self.built.get() - 1).map(|s| s.effect);
                if effect.is_some_and(|e| e != Effect::Appear) {
                    self.animate(Anim::Build);
                }
            } else if i + 1 >= n {
                self.ended.set(true);
            } else {
                self.index.set(i + 1);
                self.built.set(0);
                if let Some(t) = self.deck.slides[i + 1].transition {
                    self.animate(Anim::Transition { from: i, t });
                }
            }
        } else if self.built.get() > 0 {
            self.built.set(self.built.get() - 1);
        } else if i > 0 {
            self.index.set(i - 1);
            self.built.set(self.builds(i - 1));
        }
        self.refresh();
    }

    fn jump(self: &Rc<Self>, i: usize) {
        self.ended.set(false);
        self.anim.set(None);
        self.index.set(i.min(self.deck.slides.len() - 1));
        self.built.set(0);
        self.refresh();
    }

    /// Start an animation and redraw every frame until it's done.
    fn animate(self: &Rc<Self>, a: Anim) {
        if prefs::get().reduce_motion {
            return;
        }
        self.anim.set(Some((a, Instant::now())));
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
            let Some(s) = weak.upgrade() else { return glib::ControlFlow::Break };
            for a in s.areas.borrow().iter() {
                a.queue_draw();
            }
            match s.anim.get() {
                Some((a, start)) if start.elapsed().as_secs_f64() < a.secs() && !s.closing.get() => glib::ControlFlow::Continue,
                _ => {
                    s.anim.set(None);
                    for a in s.areas.borrow().iter() {
                        a.queue_draw();
                    }
                    glib::ControlFlow::Break
                }
            }
        });
    }

    /// The running animation and how far along it is, 0 to 1.
    fn progress(&self) -> Option<(Anim, f64)> {
        let (a, start) = self.anim.get()?;
        let t = start.elapsed().as_secs_f64() / a.secs().max(0.01);
        (t < 1.0).then_some((a, t))
    }

    fn redraw(&self) {
        for a in self.areas.borrow().iter() {
            a.queue_draw();
        }
    }

    /// Over the slide showing: the laser's dot, and a slide number being typed.
    fn paint_extras(&self, cr: &cairo::Context, w: f64, h: f64) {
        let (ox, oy, scale) = letterbox(w, h);
        if self.laser.get()
            && let Some((px, py)) = self.pointer.get()
            && (0.0..=W).contains(&px)
            && (0.0..=H).contains(&py)
        {
            let (x, y) = (ox + px * scale, oy + py * scale);
            let r = (7.0 * scale).max(4.0);
            let glow = cairo::RadialGradient::new(x, y, 0.0, x, y, r * 3.0);
            glow.add_color_stop_rgba(0.0, 1.0, 0.1, 0.1, 0.55);
            glow.add_color_stop_rgba(1.0, 1.0, 0.1, 0.1, 0.0);
            cr.set_source(&glow).ok();
            cr.arc(x, y, r * 3.0, 0.0, std::f64::consts::TAU);
            cr.fill().ok();
            cr.set_source_rgb(1.0, 0.15, 0.1);
            cr.arc(x, y, r, 0.0, std::f64::consts::TAU);
            cr.fill().ok();
        }
        let typed = self.typed.borrow();
        if !typed.is_empty() {
            let layout = gtk::pango::Layout::new(&self.ctx);
            let mut font = gtk::pango::FontDescription::from_string("Noto Sans Bold");
            font.set_size(18 * gtk::pango::SCALE);
            layout.set_font_description(Some(&font));
            layout.set_text(&format!("Go to slide {typed}"));
            let (tw, th) = layout.pixel_size();
            let (bw, bh) = (tw as f64 + 32.0, th as f64 + 18.0);
            let (bx, by) = (w - bw - 24.0, h - bh - 24.0);
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.72);
            cr.rectangle(bx, by, bw, bh);
            cr.fill().ok();
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.move_to(bx + 16.0, by + 9.0);
            pangocairo::functions::show_layout(cr, &layout);
        }
    }

    /// Typing a slide number: a digit more, or Enter to go there. True when used.
    fn type_key(self: &Rc<Self>, key: gdk::Key) -> bool {
        let digit = key.to_unicode().filter(|c| c.is_ascii_digit());
        if let Some(d) = digit {
            let mut t = self.typed.borrow_mut();
            if t.len() < 4 {
                t.push(d);
            }
            drop(t);
            self.redraw();
            return true;
        }
        let typed = std::mem::take(&mut *self.typed.borrow_mut());
        if typed.is_empty() {
            return false;
        }
        match key {
            gdk::Key::Return | gdk::Key::KP_Enter => {
                if let Ok(n) = typed.parse::<usize>()
                    && n >= 1
                {
                    self.jump(n - 1);
                }
                self.redraw();
                true
            }
            gdk::Key::BackSpace => {
                let mut t = typed;
                t.pop();
                *self.typed.borrow_mut() = t;
                self.redraw();
                true
            }
            // Any other key lets the number go.
            _ => {
                self.redraw();
                matches!(key, gdk::Key::Escape)
            }
        }
    }

    fn refresh(&self) {
        self.redraw();
        self.load_players();
        let n = self.deck.slides.len();
        let i = self.index.get();
        let b = self.builds(i);
        self.readout.set_text(&if self.ended.get() {
            "End of the slideshow".to_string()
        } else if b > 0 {
            format!("Slide {} of {n} · build {} of {b}", i + 1, self.built.get())
        } else {
            format!("Slide {} of {n}", i + 1)
        });
        let notes = self.deck.slides.get(i).map(|s| s.notes.clone()).unwrap_or_default();
        self.notes.set_text(if notes.trim().is_empty() { "No notes for this slide." } else { &notes });
    }

    fn stop(self: &Rc<Self>) {
        if self.closing.replace(true) {
            return;
        }
        self.clear_players();
        for w in self.windows.borrow().iter() {
            w.close();
        }
        self.editor.go_to(self.index.get());
        if let Some(w) = window::window() {
            w.present();
        }
    }

    /// Draw slide `i` letterboxed on black, or the end screen. `live` is the
    /// slide being shown (builds and animations apply); otherwise it's drawn whole.
    fn paint(&self, cr: &cairo::Context, i: usize, w: f64, h: f64, end_screen: bool, live: bool) {
        cr.set_source_rgb(0.0, 0.0, 0.0);
        cr.paint().ok();
        if end_screen {
            let layout = gtk::pango::Layout::new(&self.ctx);
            let mut font = gtk::pango::FontDescription::from_string("Noto Sans");
            font.set_size(14 * gtk::pango::SCALE);
            layout.set_font_description(Some(&font));
            layout.set_text("End of the slideshow. Click or press a key to leave.");
            let (tw, th) = layout.pixel_size();
            cr.set_source_rgb(0.7, 0.7, 0.7);
            cr.move_to((w - tw as f64) / 2.0, (h - th as f64) / 2.0);
            pangocairo::functions::show_layout(cr, &layout);
            return;
        }
        let Some(slide) = self.deck.composed(i) else { return };
        let slide = &slide;
        let scale = (w / W).min(h / H);
        let (sw, sh) = (W * scale, H * scale);
        cr.save().ok();
        cr.translate((w - sw) / 2.0, (h - sh) / 2.0);
        cr.rectangle(0.0, 0.0, sw, sh);
        cr.clip();
        if !live {
            render::draw(cr, &self.ctx, slide, &self.media, &self.pictures, (sw, sh), Hints::default());
            cr.restore().ok();
            return;
        }
        let progress = self.progress();
        let hints = Hints {
            built: Some(self.built.get()),
            anim: match progress {
                Some((Anim::Build, t)) => Some(t),
                _ => None,
            },
            ..Default::default()
        };
        let draw_now = |cr: &cairo::Context| render::draw(cr, &self.ctx, slide, &self.media, &self.pictures, (sw, sh), hints);
        match progress {
            Some((Anim::Transition { from, t: tr }, t)) => {
                let ease = 0.5 - 0.5 * (std::f64::consts::PI * t).cos();
                let draw_from = |cr: &cairo::Context| {
                    if let Some(prev) = self.deck.composed(from) {
                        render::draw(cr, &self.ctx, &prev, &self.media, &self.pictures, (sw, sh), Hints::default());
                    }
                };
                match tr.kind {
                    TransitionKind::MagicMove => {
                        let Some(prev) = self.deck.composed(from) else {
                            draw_now(cr);
                            cr.restore().ok();
                            return;
                        };
                        // The slides themselves cross-fade, without their objects.
                        let bare = |s: &Slide| Slide { objects: Vec::new(), ..s.clone() };
                        render::draw(cr, &self.ctx, &bare(&prev), &self.media, &self.pictures, (sw, sh), Hints::default());
                        cr.push_group();
                        render::draw(cr, &self.ctx, &bare(slide), &self.media, &self.pictures, (sw, sh), Hints::default());
                        cr.pop_group_to_source().ok();
                        cr.paint_with_alpha(ease).ok();
                        // What both slides have moves; the rest fades out or in.
                        let gone = prev.hidden_after(prev.steps().len());
                        let old: Vec<&Object> = prev.objects.iter().filter(|o| !gone.contains(&o.id)).collect();
                        let waiting = slide.hidden_after(0);
                        let mut used = vec![false; old.len()];
                        let mut moves: Vec<(Object, f64)> = Vec::new();
                        for o in slide.objects.iter().filter(|o| !waiting.contains(&o.id)) {
                            match old.iter().enumerate().position(|(j, p)| !used[j] && p.same_as(o)) {
                                Some(j) => {
                                    used[j] = true;
                                    let p = old[j];
                                    let mix = |a: f64, b: f64| a + (b - a) * ease;
                                    let mut m = o.clone();
                                    (m.x, m.y, m.w, m.h) = (mix(p.x, o.x), mix(p.y, o.y), mix(p.w, o.w), mix(p.h, o.h));
                                    m.rotation = mix(p.rotation, o.rotation);
                                    moves.push((m, 1.0));
                                }
                                None => moves.push((o.clone(), ease)),
                            }
                        }
                        for (j, p) in old.iter().enumerate() {
                            if !used[j] {
                                render::draw_alone(cr, &self.ctx, &prev, p, &self.media, &self.pictures, (sw, sh), 1.0 - ease);
                            }
                        }
                        for (m, alpha) in &moves {
                            render::draw_alone(cr, &self.ctx, slide, m, &self.media, &self.pictures, (sw, sh), *alpha);
                        }
                    }
                    TransitionKind::Fade => {
                        draw_from(cr);
                        cr.push_group();
                        draw_now(cr);
                        cr.pop_group_to_source().ok();
                        cr.paint_with_alpha(ease).ok();
                    }
                    TransitionKind::Push => {
                        cr.save().ok();
                        cr.translate(-ease * sw, 0.0);
                        draw_from(cr);
                        cr.translate(sw, 0.0);
                        draw_now(cr);
                        cr.restore().ok();
                    }
                    TransitionKind::Wipe => {
                        draw_from(cr);
                        cr.save().ok();
                        cr.rectangle(0.0, 0.0, ease * sw, sh);
                        cr.clip();
                        draw_now(cr);
                        cr.restore().ok();
                    }
                    TransitionKind::Dissolve => {
                        // Blocks of the new slide appear in a fixed scattered order.
                        draw_from(cr);
                        let (cols, rows) = (32usize, 18usize);
                        let shown = (t * (cols * rows) as f64) as usize;
                        cr.save().ok();
                        for k in 0..cols * rows {
                            let rank = (k * 7919 + 104_729) % (cols * rows);
                            if rank < shown {
                                let (c, r) = (k % cols, k / cols);
                                cr.rectangle(c as f64 * sw / cols as f64, r as f64 * sh / rows as f64, sw / cols as f64 + 0.5, sh / rows as f64 + 0.5);
                            }
                        }
                        cr.clip();
                        draw_now(cr);
                        cr.restore().ok();
                    }
                }
            }
            _ => draw_now(cr),
        }
        cr.restore().ok();
    }
}

/// A media file on disk to play from (players read files, not memory): kept
/// in the cache, named by its contents.
fn media_file(name: &str, bytes: &[u8]) -> Option<std::path::PathBuf> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("bin");
    let dir = glib::user_cache_dir().join("nexus-office").join("media");
    let path = dir.join(format!("{:016x}.{ext}", h.finish()));
    if !path.exists() {
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(&path, bytes).ok()?;
    }
    Some(path)
}

fn slide_area(show: &Rc<Show>, offset: usize) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    let weak = Rc::downgrade(show);
    area.set_draw_func(move |_, cr, w, h| {
        let Some(s) = weak.upgrade() else { return };
        let i = s.index.get() + offset;
        // The main view shows the end screen; "next" shows nothing past the end.
        let end = s.ended.get() || i >= s.deck.slides.len();
        if offset > 0 && end {
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.paint().ok();
            return;
        }
        s.paint(cr, i, w as f64, h as f64, end, offset == 0);
        if offset == 0 {
            s.paint_extras(cr, w as f64, h as f64);
        }
    });
    // The pointer, for the laser, in the slide's points.
    let motion = gtk::EventControllerMotion::new();
    let weak = Rc::downgrade(show);
    motion.connect_motion(move |c, x, y| {
        let Some(s) = weak.upgrade() else { return };
        let Some(a) = c.widget() else { return };
        let (ox, oy, scale) = letterbox(a.width() as f64, a.height() as f64);
        s.pointer.set(Some(((x - ox) / scale, (y - oy) / scale)));
        if s.laser.get() {
            s.redraw();
        }
    });
    let weak = Rc::downgrade(show);
    motion.connect_leave(move |_| {
        if let Some(s) = weak.upgrade() {
            s.pointer.set(None);
            s.redraw();
        }
    });
    if offset == 0 {
        area.add_controller(motion);
    }
    show.areas.borrow_mut().push(area.clone());
    area
}

fn keys(show: &Rc<Show>, w: &gtk::Window) {
    let keys = gtk::EventControllerKey::new();
    let weak = Rc::downgrade(show);
    keys.connect_key_pressed(move |_, key, _, _| {
        let Some(s) = weak.upgrade() else { return glib::Propagation::Proceed };
        if s.type_key(key) {
            return glib::Propagation::Stop;
        }
        match key {
            gdk::Key::l | gdk::Key::L => {
                s.laser.set(!s.laser.get());
                s.redraw();
            }
            gdk::Key::Right
            | gdk::Key::Down
            | gdk::Key::space
            | gdk::Key::Page_Down
            | gdk::Key::Return
            | gdk::Key::KP_Enter
            | gdk::Key::n => s.step(1),
            gdk::Key::Left | gdk::Key::Up | gdk::Key::BackSpace | gdk::Key::Page_Up | gdk::Key::p => s.step(-1),
            gdk::Key::Home => s.jump(0),
            gdk::Key::End => s.jump(usize::MAX),
            gdk::Key::Escape | gdk::Key::q | gdk::Key::F5 => s.stop(),
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    });
    w.add_controller(keys);
    let weak = Rc::downgrade(show);
    w.connect_close_request(move |_| {
        if let Some(s) = weak.upgrade() {
            s.stop();
        }
        glib::Propagation::Proceed
    });
}

fn bare_window(app: Option<&gtk::Application>, title: &str) -> gtk::Window {
    let w = gtk::Window::builder().title(title).build();
    if let Some(app) = app {
        w.set_application(Some(app));
    }
    w.add_css_class("office-window");
    w.set_titlebar(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    w
}

pub fn start(editor: Rc<Present>, from: usize) {
    let deck = editor.deck.borrow().clone();
    let media = editor.media.borrow().clone();
    let from = from.min(deck.slides.len().saturating_sub(1));
    let main = window::window();
    let app = main.as_ref().and_then(|w| w.application());
    let show = Rc::new(Show {
        editor,
        deck,
        media,
        pictures: Pictures::default(),
        ctx: crate::print::text_context(),
        index: Cell::new(from),
        built: Cell::new(0),
        anim: Cell::new(None),
        ended: Cell::new(false),
        started: Instant::now(),
        windows: RefCell::new(Vec::new()),
        areas: RefCell::new(Vec::new()),
        readout: widgets::label("", "presenter-readout"),
        timer: widgets::label("0:00", "presenter-timer"),
        notes: widgets::label("", "presenter-notes"),
        closing: Cell::new(false),
        stage: gtk::Overlay::new(),
        stage_area: RefCell::new(None),
        players: RefCell::new(Vec::new()),
        typed: RefCell::new(String::new()),
        laser: Cell::new(false),
        pointer: Cell::new(None),
    });

    // The slides themselves.
    let stage = bare_window(app.as_ref(), "Slideshow");
    stage.add_css_class("slideshow");
    let area = slide_area(&show, 0);
    *show.stage_area.borrow_mut() = Some(area.clone());
    let weak = Rc::downgrade(&show);
    area.connect_resize(move |_, _, _| {
        if let Some(s) = weak.upgrade() {
            s.place_players();
        }
    });
    let click = gtk::GestureClick::new();
    click.set_button(0);
    let weak = Rc::downgrade(&show);
    click.connect_pressed(move |g, _, _, _| {
        if let Some(s) = weak.upgrade() {
            s.step(if g.current_button() == 3 { -1 } else { 1 });
        }
    });
    area.add_controller(click);
    show.stage.set_child(Some(&area));
    stage.set_child(Some(&show.stage));
    stage.set_cursor_from_name(Some("none"));
    keys(&show, &stage);
    show.windows.borrow_mut().push(stage.clone());

    // With two screens: slides on the other one, the presenter view on this one.
    let display = gdk::Display::default();
    let here = display.as_ref().zip(main.as_ref().and_then(|w| w.surface())).and_then(|(d, s)| d.monitor_at_surface(&s));
    let others: Vec<gdk::Monitor> = display
        .as_ref()
        .map(|d| {
            let m = d.monitors();
            (0..m.n_items())
                .filter_map(|i| m.item(i).and_downcast::<gdk::Monitor>())
                .filter(|m| Some(m) != here.as_ref())
                .collect()
        })
        .unwrap_or_default();
    let presenter = prefs::get().presenter_view && !others.is_empty() && here.is_some();
    if presenter {
        stage.fullscreen_on_monitor(&others[0]);
        let pw = bare_window(app.as_ref(), "Presenter view");
        pw.add_css_class("presenter");
        let root = widgets::vbox(0);
        let bar = widgets::hbox(12);
        bar.add_css_class("top-bar");
        bar.append(&show.readout);
        bar.append(&show.timer);
        let spacer = widgets::hbox(0);
        spacer.set_hexpand(true);
        bar.append(&spacer);
        for (icon, tip, by) in [("nxo-arrow-left-symbolic", "Previous slide", -1), ("nxo-arrow-right-symbolic", "Next slide", 1)]
        {
            let b = widgets::bar_button(icon, tip);
            let weak = Rc::downgrade(&show);
            b.connect_clicked(move |_| {
                if let Some(s) = weak.upgrade() {
                    s.step(by);
                }
            });
            bar.append(&b);
        }
        let end = gtk::Button::with_label("End show");
        let weak = Rc::downgrade(&show);
        end.connect_clicked(move |_| {
            if let Some(s) = weak.upgrade() {
                s.stop();
            }
        });
        bar.append(&end);
        root.append(&bar);
        let body = widgets::hbox(0);
        body.set_vexpand(true);
        let now = slide_area(&show, 0);
        now.add_css_class("presenter-now");
        body.append(&now);
        let side = widgets::vbox(10);
        side.add_css_class("presenter-side");
        side.append(&widgets::label("NEXT", "group-title"));
        let next = slide_area(&show, 1);
        next.set_size_request(320, 180);
        next.set_vexpand(false);
        side.append(&next);
        side.append(&widgets::label("NOTES", "group-title"));
        show.notes.set_wrap(true);
        show.notes.set_valign(gtk::Align::Start);
        let scroll =
            gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&show.notes).build();
        side.append(&scroll);
        body.append(&side);
        root.append(&body);
        pw.set_child(Some(&root));
        keys(&show, &pw);
        if let Some(h) = &here {
            pw.fullscreen_on_monitor(h);
        }
        show.windows.borrow_mut().push(pw.clone());
        pw.present();
        stage.present();
        // The timer runs while the show does.
        let weak = Rc::downgrade(&show);
        glib::timeout_add_seconds_local(1, move || {
            let Some(s) = weak.upgrade() else { return glib::ControlFlow::Break };
            if s.closing.get() {
                return glib::ControlFlow::Break;
            }
            let t = s.started.elapsed().as_secs();
            s.timer.set_text(&if t >= 3600 {
                format!("{}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60)
            } else {
                format!("{}:{:02}", t / 60, t % 60)
            });
            glib::ControlFlow::Continue
        });
    } else {
        stage.fullscreen();
        stage.present();
    }
    show.refresh();
    // Keep the show alive as long as its windows are.
    let keep = show.clone();
    stage.connect_destroy(move |_| {
        let _ = &keep;
    });
}
