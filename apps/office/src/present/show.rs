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
    fn builds(&self, i: usize) -> usize {
        self.deck.slides.get(i).map(|s| s.build_steps().len()).unwrap_or(0)
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
                let effect = self.deck.slides[i]
                    .build_steps()
                    .get(self.built.get() - 1)
                    .and_then(|id| self.deck.slides[i].objects.iter().find(|o| o.id == *id))
                    .and_then(|o| o.build)
                    .map(|b| b.effect);
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

    fn refresh(&self) {
        for a in self.areas.borrow().iter() {
            a.queue_draw();
        }
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
        let Some(slide) = self.deck.slides.get(i) else { return };
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
                    if let Some(prev) = self.deck.slides.get(from) {
                        render::draw(cr, &self.ctx, prev, &self.media, &self.pictures, (sw, sh), Hints::default());
                    }
                };
                match tr.kind {
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
    });
    show.areas.borrow_mut().push(area.clone());
    area
}

fn keys(show: &Rc<Show>, w: &gtk::Window) {
    let keys = gtk::EventControllerKey::new();
    let weak = Rc::downgrade(show);
    keys.connect_key_pressed(move |_, key, _, _| {
        let Some(s) = weak.upgrade() else { return glib::Propagation::Proceed };
        match key {
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
    });

    // The slides themselves.
    let stage = bare_window(app.as_ref(), "Slideshow");
    stage.add_css_class("slideshow");
    let area = slide_area(&show, 0);
    let click = gtk::GestureClick::new();
    click.set_button(0);
    let weak = Rc::downgrade(&show);
    click.connect_pressed(move |g, _, _, _| {
        if let Some(s) = weak.upgrade() {
            s.step(if g.current_button() == 3 { -1 } else { 1 });
        }
    });
    area.add_controller(click);
    stage.set_child(Some(&area));
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
