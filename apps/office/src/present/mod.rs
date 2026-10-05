//! Nexus Present: slides.

pub mod model;
mod arrange;
mod looks;
mod media;
pub(crate) mod data;
mod pptx;
pub(crate) mod pptx_more;
pub mod render;
mod show;
mod templates;

use crate::doc::{self, Find, SidebarGroup, Template};
use crate::print::Pages;
use crate::{color, container, widgets, window};
use anyhow::{Result, bail};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use model::*;
use render::{Hints, Pictures};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::{Rc, Weak};

const FORMAT: &str = "nexus-present";
const UNDO_LIMIT: usize = 80;
const SIZES: &[&str] =
    &["14", "16", "18", "20", "24", "26", "28", "32", "36", "40", "42", "44", "48", "50", "54", "60", "72", "88"];
/// The canvas leaves this much room around the slide.
const PAD: f64 = 28.0;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Target {
    Title,
    Body,
    Object(u32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    None,
    /// Moving an object: where it was, and where the pointer took hold.
    Move {
        id: u32,
        x0: f64,
        y0: f64,
    },
    Resize {
        id: u32,
        w0: f64,
        h0: f64,
    },
    /// Turning an object by its handle: where the pointer took hold, on the slide.
    Rotate {
        id: u32,
        sx: f64,
        sy: f64,
    },
}

/// How far above an object its rotation handle sits, in screen pixels.
const ROTATE_GAP: f64 = 26.0;

struct Controls {
    layout: gtk::DropDown,
    font: gtk::DropDown,
    fonts: Vec<String>,
    title_size: gtk::DropDown,
    body_size: gtk::DropDown,
    bold: gtk::ToggleButton,
    italic: gtk::ToggleButton,
    background: Rc<color::ColorButton>,
    color: Rc<color::ColorButton>,
    fill: Rc<color::ColorButton>,
    object_tools: gtk::Box,
    group: gtk::Button,
    shadow: gtk::ToggleButton,
    master: gtk::ToggleButton,
}

pub struct Present {
    me: RefCell<Weak<Present>>,
    root: gtk::Box,
    deck: RefCell<Deck>,
    media: RefCell<BTreeMap<String, Vec<u8>>>,
    pictures: Pictures,
    current: Cell<usize>,
    selected: Cell<Option<u32>>,
    /// Editing the master slide (objects on every slide) instead of this slide.
    master_mode: Cell<bool>,
    /// More objects selected with Shift-click.
    also: RefCell<Vec<u32>>,
    /// Where each dragged object started.
    drag_from: RefCell<Vec<(u32, f64, f64)>>,
    /// Snap guides to draw while dragging: (vertical, position in slide points).
    guides: RefCell<Vec<(bool, f64)>>,
    canvas: gtk::DrawingArea,
    /// Scrolls the canvas when it's zoomed in past the window.
    desk: gtk::ScrolledWindow,
    /// 1 fits the slide to the window.
    zoom: Cell<f64>,
    edit: gtk::TextView,
    edit_css: gtk::CssProvider,
    editing: Cell<Option<Target>>,
    notes: gtk::TextView,
    thumbs: gtk::Box,
    controls: RefCell<Option<Controls>>,
    drag: Cell<Drag>,
    undo: RefCell<Vec<(Deck, usize)>>,
    redo: RefCell<Vec<(Deck, usize)>>,
    /// An undo step is open for a burst of typing.
    group: RefCell<Option<glib::SourceId>>,
    syncing: Cell<bool>,
    thumbs_job: RefCell<Option<glib::SourceId>>,
}

thread_local! {
    static PRESENT: RefCell<Option<Rc<Present>>> = const { RefCell::new(None) };
}

fn ctx() -> gtk::pango::Context {
    thread_local! {
        static CTX: gtk::pango::Context = crate::print::text_context();
    }
    CTX.with(|c| c.clone())
}

impl Present {
    pub fn create() -> Rc<dyn doc::Editor> {
        Present::build()
    }

    fn build() -> Rc<Present> {
        let canvas = gtk::DrawingArea::new();
        canvas.set_hexpand(true);
        canvas.set_vexpand(true);
        canvas.set_focusable(true);
        canvas.add_css_class("slide-canvas");
        let edit = gtk::TextView::new();
        edit.add_css_class("slide-edit");
        edit.set_wrap_mode(gtk::WrapMode::WordChar);
        edit.set_visible(false);
        edit.set_halign(gtk::Align::Start);
        edit.set_valign(gtk::Align::Start);
        let edit_css = gtk::CssProvider::new();
        #[allow(deprecated)]
        edit.style_context().add_provider(&edit_css, gtk::STYLE_PROVIDER_PRIORITY_USER);
        let notes = gtk::TextView::new();
        notes.add_css_class("notes-text");
        notes.set_wrap_mode(gtk::WrapMode::WordChar);
        notes.set_top_margin(8);
        notes.set_left_margin(12);
        notes.set_right_margin(12);
        let p = Rc::new(Present {
            me: RefCell::new(Weak::new()),
            root: widgets::vbox(0),
            deck: RefCell::new(Deck::default()),
            media: RefCell::new(BTreeMap::new()),
            pictures: Pictures::default(),
            current: Cell::new(0),
            selected: Cell::new(None),
            master_mode: Cell::new(false),
            also: RefCell::new(Vec::new()),
            drag_from: RefCell::new(Vec::new()),
            guides: RefCell::new(Vec::new()),
            canvas,
            edit,
            edit_css,
            editing: Cell::new(None),
            notes,
            thumbs: widgets::vbox(6),
            controls: RefCell::new(None),
            drag: Cell::new(Drag::None),
            undo: RefCell::new(Vec::new()),
            redo: RefCell::new(Vec::new()),
            group: RefCell::new(None),
            syncing: Cell::new(false),
            thumbs_job: RefCell::new(None),
            desk: gtk::ScrolledWindow::new(),
            zoom: Cell::new(1.0),
        });
        *p.me.borrow_mut() = Rc::downgrade(&p);
        p.root.add_css_class("present-editor");
        p.root.append(&p.build_toolbar());
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&p.canvas));
        overlay.add_overlay(&p.edit);
        overlay.add_css_class("slide-desk");
        p.desk.set_child(Some(&overlay));
        p.desk.set_vexpand(true);
        p.root.append(&p.desk);
        // The window's size sets the fitted size the zoom starts from.
        for adj in [p.desk.hadjustment(), p.desk.vadjustment()] {
            let me = Rc::downgrade(&p);
            adj.connect_changed(move |_| {
                if let Some(p) = me.upgrade() {
                    p.size_canvas();
                }
            });
        }
        let notes_box = widgets::vbox(0);
        notes_box.add_css_class("notes-box");
        let head = widgets::label("SPEAKER NOTES", "notes-head");
        notes_box.append(&head);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .min_content_height(96)
            .max_content_height(160)
            .child(&p.notes)
            .build();
        notes_box.append(&scroll);
        p.root.append(&notes_box);
        p.thumbs.add_css_class("slide-thumbs");
        p.connect_canvas();
        p.connect_text();
        p.refresh_thumbs();
        PRESENT.with(|x| *x.borrow_mut() = Some(p.clone()));
        p
    }

    fn weak(&self) -> Weak<Present> {
        self.me.borrow().clone()
    }

    fn rc(&self) -> Rc<Present> {
        self.me.borrow().upgrade().expect("the editor is alive")
    }

    /// The slide being edited (in master mode, the master on this slide's background).
    fn slide(&self) -> Slide {
        self.deck.borrow().editing(self.current.get(), self.master_mode.get())
    }

    // ---------- Toolbar ----------

    fn build_toolbar(self: &Rc<Self>) -> gtk::Box {
        let bar = widgets::hbox(0);
        bar.add_css_class("toolbar");
        bar.add_css_class("doc-toolbar");
        let flow = widgets::wrap_row(4, 4);
        flow.add_css_class("tool-row");
        flow.set_hexpand(true);
        flow.set_valign(gtk::Align::Center);
        bar.append(&flow);
        let group = || {
            let g = widgets::hbox(2);
            g.add_css_class("tool-group");
            g
        };
        let button = |icon: &str, tip: &str, f: fn(&Present)| {
            let b = widgets::bar_button(icon, tip);
            let me = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(p) = me.upgrade() {
                    f(&p);
                }
            });
            b
        };
        let dropdown = |items: &[&str], tip: &str, class: &str, f: fn(&Present, u32)| {
            let d = gtk::DropDown::from_strings(items);
            d.set_tooltip_text(Some(tip));
            d.add_css_class(class);
            let me = Rc::downgrade(self);
            d.connect_selected_notify(move |d| {
                if let Some(p) = me.upgrade()
                    && !p.syncing.get()
                    && d.selected() != gtk::INVALID_LIST_POSITION
                {
                    f(&p, d.selected());
                }
            });
            d
        };

        let g = group();
        g.append(&button("nxo-undo-2-symbolic", "Undo (Ctrl+Z)", doc::Editor::undo));
        g.append(&button("nxo-redo-2-symbolic", "Redo (Ctrl+Shift+Z)", doc::Editor::redo));
        flow.append(&g);

        let g = group();
        g.append(&button("nxo-plus-symbolic", "New slide (Ctrl+M)", |p| p.add_slide()));
        let names: Vec<&str> = Layout::ALL.iter().map(|l| l.name()).collect();
        let layout = dropdown(&names, "Layout", "layout-picker", |p, i| {
            let l = Layout::ALL[i as usize % Layout::ALL.len()];
            p.change_slide(|s| s.layout = l);
        });
        g.append(&layout);
        g.append(&self.transition_button());
        let master = gtk::ToggleButton::new();
        master.set_icon_name("nxo-crown-symbolic");
        master.add_css_class("tool-toggle");
        master.set_tooltip_text(Some("Master slide: put a logo, footer or slide number on every slide"));
        let me = Rc::downgrade(self);
        master.connect_toggled(move |b| {
            if let Some(p) = me.upgrade()
                && !p.syncing.get()
            {
                p.set_master_mode(b.is_active());
            }
        });
        g.append(&master);
        flow.append(&g);

        let g = group();
        g.append(&self.looks_button());
        let me = Rc::downgrade(self);
        let background = color::button("nxo-paint-bucket-symbolic", "Slide background", &BACKGROUNDS, "White", move |c| {
            if let Some(p) = me.upgrade() {
                let c = c.unwrap_or_else(|| "#ffffff".into());
                p.change_slide(move |s| s.background = c.clone());
            }
        });
        let me = Rc::downgrade(self);
        let text_color = color::button("nxo-baseline-symbolic", "Text color", color::TEXT, "Automatic", move |c| {
            if let Some(p) = me.upgrade() {
                p.change_slide(move |s| {
                    s.color = c.clone().unwrap_or_else(|| {
                        let (r, g, b) = render::rgb(&s.background);
                        if 0.299 * r + 0.587 * g + 0.114 * b > 0.6 { "#202536".into() } else { "#ffffff".into() }
                    })
                });
            }
        });
        g.append(&background.button);
        g.append(&text_color.button);
        flow.append(&g);

        let g = group();
        let fonts = font_families(&self.canvas);
        let font_refs: Vec<&str> = fonts.iter().map(String::as_str).collect();
        let font = gtk::DropDown::from_strings(&font_refs);
        font.set_enable_search(true);
        font.set_expression(Some(gtk::PropertyExpression::new(
            gtk::StringObject::static_type(),
            None::<gtk::Expression>,
            "string",
        )));
        font.set_tooltip_text(Some("Font for this slide"));
        font.add_css_class("font-picker");
        let me = Rc::downgrade(self);
        let fl = fonts.clone();
        font.connect_selected_notify(move |d| {
            if let Some(p) = me.upgrade()
                && !p.syncing.get()
                && let Some(f) = fl.get(d.selected() as usize).cloned()
            {
                p.change_slide(move |s| s.font = f.clone());
            }
        });
        g.append(&font);
        let title_size = dropdown(SIZES, "Title size", "size-picker", |p, i| {
            let v = SIZES[i as usize].parse().ok();
            p.change_slide(move |s| s.title_size = v);
        });
        let body_size = dropdown(SIZES, "Body size", "size-picker", |p, i| {
            let v = SIZES[i as usize].parse().ok();
            p.change_slide(move |s| s.body_size = v);
        });
        g.append(&title_size);
        g.append(&body_size);
        let toggle = |icon: &str, tip: &str, f: fn(&Present, bool)| {
            let b = gtk::ToggleButton::new();
            b.set_icon_name(icon);
            b.add_css_class("tool-toggle");
            b.set_tooltip_text(Some(tip));
            let me = Rc::downgrade(self);
            b.connect_toggled(move |b| {
                if let Some(p) = me.upgrade()
                    && !p.syncing.get()
                {
                    f(&p, b.is_active());
                }
            });
            b
        };
        let bold = toggle("nxo-bold-symbolic", "Bold (Ctrl+B)", |p, on| p.change_slide(move |s| s.bold = on));
        let italic = toggle("nxo-italic-symbolic", "Italic (Ctrl+I)", |p, on| p.change_slide(move |s| s.italic = on));
        g.append(&bold);
        g.append(&italic);
        flow.append(&g);

        let g = group();
        g.append(&button("nxo-text-cursor-symbolic", "Add a text box", |p| {
            p.add_object(Kind::Text { text: String::new() }, 360.0, 90.0)
        }));
        g.append(&button("nxo-image-symbolic", "Add a picture", |p| p.pick_picture()));
        g.append(&self.shapes_button());
        g.append(&button("nxo-film-symbolic", "Add video or audio (it plays in the slideshow)", |p| p.pick_media()));
        g.append(&button("nxo-book-marked-symbolic", "Add the slide number", |p| p.add_slide_number()));
        g.append(&button("nxo-table-2-symbolic", "Add a table (double-click it to fill it in)", |p| {
            p.add_object(Kind::table(3, 3), 520.0, 180.0)
        }));
        g.append(&button("nxo-chart-column-symbolic", "Add a chart (double-click it to change its numbers)", |p| {
            p.add_object(Kind::sample_chart(), 480.0, 300.0)
        }));
        flow.append(&g);

        // Shown while an object is selected.
        let object_tools = group();
        let me = Rc::downgrade(self);
        let fill = color::button("nxo-palette-symbolic", "Shape color", &BACKGROUNDS, "Blue", move |c| {
            if let Some(p) = me.upgrade() {
                let c = c.unwrap_or_else(|| "#5273cf".into());
                p.change_object(move |o| {
                    if let Kind::Shape { fill, .. } = &mut o.kind {
                        *fill = c.clone();
                    }
                });
            }
        });
        object_tools.append(&fill.button);
        object_tools.append(&self.arrange_button());
        let group_button = button("nxo-group-symbolic", "Group the selected objects (Ctrl+G)", |p| p.group_chosen());
        object_tools.append(&group_button);
        object_tools.append(&button("nxo-ungroup-symbolic", "Ungroup (Ctrl+Shift+G)", |p| p.ungroup_chosen()));
        object_tools.append(&self.rotate_button());
        object_tools.append(&self.outline_button());
        let shadow = gtk::ToggleButton::new();
        shadow.set_icon_name("nxo-layers-2-symbolic");
        shadow.add_css_class("tool-toggle");
        shadow.set_tooltip_text(Some("Drop shadow"));
        let me = Rc::downgrade(self);
        shadow.connect_toggled(move |b| {
            if let Some(p) = me.upgrade()
                && !p.syncing.get()
            {
                let on = b.is_active();
                p.change_object(move |o| o.shadow = on);
            }
        });
        object_tools.append(&shadow);
        object_tools.append(&self.build_button());
        object_tools.append(&button("nxo-trash-2-symbolic", "Delete the object (Delete)", |p| p.delete_chosen()));
        object_tools.set_visible(false);
        flow.append(&object_tools);

        let g = group();
        let play =
            button("nxo-play-symbolic", "Present from this slide (Shift+F5; F5 from the start)", |p| p.present(p.current.get()));
        play.add_css_class("play-button");
        g.append(&play);
        flow.append(&g);

        let me = Rc::downgrade(self);
        let from_start: doc::Action = Rc::new(move || {
            if let Some(p) = me.upgrade() {
                p.present(0);
            }
        });
        bar.append(&doc::file_menu(vec![("Present from the start", from_start)]));
        *self.controls.borrow_mut() = Some(Controls {
            layout,
            font,
            fonts,
            title_size,
            body_size,
            bold,
            italic,
            background,
            color: text_color,
            fill,
            object_tools,
            group: group_button,
            shadow,
            master,
        });
        bar
    }

    fn looks_button(self: &Rc<Self>) -> gtk::MenuButton {
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("nxo-layout-grid-symbolic");
        menu.add_css_class("bar-button");
        menu.set_tooltip_text(Some("Look: background and text together"));
        menu.set_valign(gtk::Align::Center);
        let pop = gtk::Popover::new();
        let list = widgets::vbox(2);
        list.add_css_class("prompt-box");
        let all = gtk::CheckButton::with_label("Every slide");
        for (name, bg, fg) in LOOKS {
            let b = gtk::Button::new();
            b.add_css_class("menu-item");
            let row = widgets::hbox(10);
            let chip = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            chip.add_css_class("look-chip");
            color::paint(&chip, bg);
            let a = widgets::label("Aa", "");
            let provider = gtk::CssProvider::new();
            provider.load_from_string(&format!("label {{ color: {fg}; font-weight: 700; padding: 2px 8px; }}"));
            #[allow(deprecated)]
            a.style_context().add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_USER);
            chip.append(&a);
            row.append(&chip);
            row.append(&widgets::label(name, ""));
            b.set_child(Some(&row));
            let me = Rc::downgrade(self);
            let (p, all) = (pop.clone(), all.clone());
            let (bg, fg) = (bg.to_string(), fg.to_string());
            b.connect_clicked(move |_| {
                p.popdown();
                let Some(pr) = me.upgrade() else { return };
                if all.is_active() {
                    pr.change_deck(|d| {
                        for s in &mut d.slides {
                            s.background = bg.clone();
                            s.color = fg.clone();
                        }
                    });
                } else {
                    pr.change_slide(|s| {
                        s.background = bg.clone();
                        s.color = fg.clone();
                    });
                }
            });
            list.append(&b);
        }
        list.append(&all);
        pop.set_child(Some(&list));
        menu.set_popover(Some(&pop));
        menu
    }

    fn arrange_button(self: &Rc<Self>) -> gtk::MenuButton {
        use arrange::Line;
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("nxo-align-center-symbolic");
        menu.add_css_class("bar-button");
        menu.set_tooltip_text(Some("Line up, space out or reorder: one object against the slide, several against each other"));
        menu.set_valign(gtk::Align::Center);
        let pop = gtk::Popover::new();
        pop.add_css_class("menu-popover");
        let list = widgets::vbox(0);
        type Act = fn(&Present);
        let items: [(&str, &str, Act); 10] = [
            ("nxo-align-start-vertical-symbolic", "Align left", |p| p.align(Line::Left)),
            ("nxo-align-center-vertical-symbolic", "Align centers across", |p| p.align(Line::Center)),
            ("nxo-align-end-vertical-symbolic", "Align right", |p| p.align(Line::Right)),
            ("nxo-align-start-horizontal-symbolic", "Align top", |p| p.align(Line::Top)),
            ("nxo-align-center-horizontal-symbolic", "Align middles down", |p| p.align(Line::Middle)),
            ("nxo-align-end-horizontal-symbolic", "Align bottom", |p| p.align(Line::Bottom)),
            ("nxo-align-horizontal-distribute-center-symbolic", "Space evenly across", |p| p.distribute(true)),
            ("nxo-align-vertical-distribute-center-symbolic", "Space evenly down", |p| p.distribute(false)),
            ("nxo-layers-symbolic", "Bring to front", |p| p.restack(true)),
            ("nxo-layers-symbolic", "Send to back", |p| p.restack(false)),
        ];
        for (i, (icon, label, f)) in items.into_iter().enumerate() {
            let b = gtk::Button::new();
            b.add_css_class("menu-item");
            let row = widgets::hbox(10);
            row.append(&gtk::Image::from_icon_name(icon));
            row.append(&widgets::label(label, ""));
            b.set_child(Some(&row));
            let me = Rc::downgrade(self);
            let p = pop.clone();
            b.connect_clicked(move |_| {
                p.popdown();
                if let Some(pr) = me.upgrade() {
                    f(&pr);
                }
            });
            list.append(&b);
            if i == 5 || i == 7 {
                list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            }
        }
        pop.set_child(Some(&list));
        menu.set_popover(Some(&pop));
        menu
    }

    /// Set the toolbar and notes from the current slide.
    fn sync(&self) {
        self.syncing.set(true);
        let s = self.slide();
        if let Some(c) = self.controls.borrow().as_ref() {
            c.layout.set_selected(Layout::ALL.iter().position(|l| *l == s.layout).unwrap_or(0) as u32);
            if let Some(i) = c.fonts.iter().position(|f| f.eq_ignore_ascii_case(&s.font)) {
                c.font.set_selected(i as u32);
            }
            let size_pos = |v: Option<f64>, d: Option<model::Frame>| {
                let v = v.or(d.map(|f| f.4)).unwrap_or(24.0);
                SIZES
                    .iter()
                    .position(|x| x.parse::<f64>().ok() == Some(v))
                    .map(|i| i as u32)
                    .unwrap_or(gtk::INVALID_LIST_POSITION)
            };
            c.title_size.set_selected(size_pos(s.title_size, title_frame(s.layout)));
            c.body_size.set_selected(size_pos(s.body_size, body_frame(s.layout)));
            c.bold.set_active(s.bold);
            c.italic.set_active(s.italic);
            c.background.set_current(Some(&s.background));
            c.color.set_current(Some(&s.color));
            let obj = self.selected.get().and_then(|id| s.objects.iter().find(|o| o.id == id).cloned());
            c.object_tools.set_visible(obj.is_some());
            let n = self.chosen().len();
            c.group.set_sensitive(n >= 2);
            c.shadow.set_active(obj.as_ref().is_some_and(|o| o.shadow));
            if let Some(Kind::Shape { fill, .. }) = obj.as_ref().map(|o| &o.kind) {
                c.fill.set_current(Some(fill));
                c.fill.button.set_sensitive(true);
            } else {
                c.fill.button.set_sensitive(false);
            }
        }
        let buf = self.notes.buffer();
        let (a, b) = buf.bounds();
        if buf.text(&a, &b, false) != s.notes {
            buf.set_text(&s.notes);
        }
        self.syncing.set(false);
        doc::status_changed();
    }

    // ---------- Changes ----------

    fn push_undo(&self) {
        let snap = (self.deck.borrow().clone(), self.current.get());
        let mut u = self.undo.borrow_mut();
        u.push(snap);
        if u.len() > UNDO_LIMIT {
            u.remove(0);
        }
        self.redo.borrow_mut().clear();
    }

    fn close_group(&self) {
        if let Some(id) = self.group.borrow_mut().take() {
            id.remove();
        }
    }

    /// One undo step for a burst of typing.
    fn begin_typing(&self) {
        if self.group.borrow().is_none() {
            self.push_undo();
        } else {
            self.close_group();
        }
        let me = self.weak();
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(900), move || {
            if let Some(p) = me.upgrade() {
                p.group.borrow_mut().take();
            }
        });
        *self.group.borrow_mut() = Some(id);
    }

    fn change_deck(&self, f: impl FnOnce(&mut Deck)) {
        self.close_group();
        let before = self.deck.borrow().clone();
        f(&mut self.deck.borrow_mut());
        if *self.deck.borrow() == before {
            return;
        }
        self.undo.borrow_mut().push((before, self.current.get()));
        self.redo.borrow_mut().clear();
        self.changed();
    }

    fn change_slide(&self, f: impl FnOnce(&mut Slide)) {
        let (i, master) = (self.current.get(), self.master_mode.get());
        self.change_deck(|d| {
            d.edit(i, master, f);
        });
    }

    /// Change every selected object the same way.
    fn change_object(&self, f: impl Fn(&mut Object)) {
        let chosen = self.chosen();
        if chosen.is_empty() {
            return;
        }
        self.change_slide(|s| {
            for o in s.objects.iter_mut().filter(|o| chosen.contains(&o.id)) {
                f(o);
                o.clamp();
            }
        });
    }

    /// Something in the deck changed: redraw and tell the shell.
    fn changed(&self) {
        let n = self.deck.borrow().slides.len();
        if self.current.get() >= n {
            self.current.set(n - 1);
        }
        self.canvas.queue_draw();
        self.schedule_thumbs();
        self.sync();
        doc::edited();
    }

    fn go_to(&self, i: usize) {
        self.end_edit();
        let n = self.deck.borrow().slides.len();
        self.current.set(i.min(n.saturating_sub(1)));
        self.clear_selection();
        self.canvas.queue_draw();
        self.mark_thumbs();
        self.sync();
    }

    fn add_slide(&self) {
        let i = self.current.get();
        let mut at = None;
        self.change_deck(|d| at = d.add_after(i));
        match at {
            Some(at) => self.go_to(at),
            None => window::toast(&format!("A presentation holds up to {MAX_SLIDES} slides.")),
        }
    }

    fn add_object(&self, kind: Kind, w: f64, h: f64) {
        let mut id = 0;
        let full = self.slide().objects.len() >= MAX_OBJECTS;
        if full {
            window::toast(&format!("A slide holds up to {MAX_OBJECTS} objects."));
            return;
        }
        let text = matches!(kind, Kind::Text { .. });
        self.change_slide(|s| {
            id = s.next_id();
            // Each new object lands a little lower than the last, so none hide another.
            let step = (s.objects.len() % 6) as f64 * 18.0;
            let mut o = Object::new(id, kind, ((W - w) / 2.0 + step, (H - h) / 2.0 + step, w, h));
            o.clamp();
            s.objects.push(o);
        });
        self.clear_selection();
        self.selected.set(Some(id));
        self.sync();
        if text {
            self.begin_edit(Target::Object(id));
        }
    }


    fn pick_picture(&self) {
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        let f = gtk::FileFilter::new();
        f.set_name(Some("Pictures"));
        f.add_pixbuf_formats();
        filters.append(&f);
        let dialog = gtk::FileDialog::builder().title("Add a picture").modal(true).filters(&filters).build();
        dialog.set_initial_folder(Some(&gio::File::for_path(crate::paths::home().join("Pictures"))));
        let me = self.weak();
        dialog.open(window::window().as_ref(), gio::Cancellable::NONE, move |res| {
            let (Ok(file), Some(p)) = (res, me.upgrade()) else { return };
            if let Err(e) = p.insert_picture(&file) {
                window::toast(&format!("Couldn't add that picture: {e:#}"));
            }
        });
    }

    fn insert_picture(&self, file: &gio::File) -> Result<()> {
        // Pictures are kept as PNG, at most twice the slide's size.
        let texture = gdk::Texture::from_file(file)?;
        let png = texture.save_to_png_bytes().to_vec();
        let name = {
            let media = self.media.borrow();
            let mut n = media.len() + 1;
            while media.contains_key(&format!("picture-{n}.png")) {
                n += 1;
            }
            format!("picture-{n}.png")
        };
        self.media.borrow_mut().insert(name.clone(), png);
        let (tw, th) = (texture.width().max(1) as f64, texture.height().max(1) as f64);
        let scale = (W * 0.6 / tw).min(H * 0.6 / th).min(1.0);
        self.add_object(Kind::Image { image: name }, (tw * scale).max(48.0), (th * scale).max(32.0));
        Ok(())
    }

    // ---------- Slide thumbnails ----------

    fn schedule_thumbs(&self) {
        if let Some(id) = self.thumbs_job.borrow_mut().take() {
            id.remove();
        }
        let me = self.weak();
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(120), move || {
            if let Some(p) = me.upgrade() {
                p.thumbs_job.borrow_mut().take();
                p.refresh_thumbs();
            }
        });
        *self.thumbs_job.borrow_mut() = Some(id);
    }

    fn refresh_thumbs(&self) {
        let n = self.deck.borrow().slides.len();
        let mut have = 0;
        let mut c = self.thumbs.first_child();
        while let Some(w) = c {
            have += 1;
            c = w.next_sibling();
        }
        if have != n {
            while let Some(c) = self.thumbs.first_child() {
                self.thumbs.remove(&c);
            }
            for i in 0..n {
                self.thumbs.append(&self.thumb(i));
            }
        } else {
            let mut c = self.thumbs.first_child();
            while let Some(w) = c {
                if let Some(area) = w.first_child().and_then(|b| b.last_child()) {
                    area.queue_draw();
                }
                c = w.next_sibling();
            }
        }
        self.mark_thumbs();
    }

    fn mark_thumbs(&self) {
        let mut c = self.thumbs.first_child();
        let mut i = 0;
        while let Some(w) = c {
            if i == self.current.get() {
                w.add_css_class("active");
            } else {
                w.remove_css_class("active");
            }
            i += 1;
            c = w.next_sibling();
        }
    }

    fn thumb(&self, i: usize) -> gtk::Button {
        let b = gtk::Button::new();
        b.add_css_class("slide-thumb");
        let row = widgets::hbox(8);
        let num = widgets::label(&(i + 1).to_string(), "thumb-number");
        num.set_valign(gtk::Align::Start);
        row.append(&num);
        let area = gtk::DrawingArea::new();
        area.set_content_width(176);
        area.set_content_height(99);
        let me = self.weak();
        area.set_draw_func(move |_, cr, w, h| {
            if let Some(p) = me.upgrade() {
                let d = p.deck.borrow();
                if let Some(s) = d.composed(i) {
                    render::draw(cr, &ctx(), &s, &p.media.borrow(), &p.pictures, (w as f64, h as f64), Hints::default());
                }
            }
        });
        row.append(&area);
        b.set_child(Some(&row));
        let me = self.weak();
        b.connect_clicked(move |_| {
            if let Some(p) = me.upgrade() {
                window::navigate("document");
                p.go_to(i);
            }
        });
        let click = gtk::GestureClick::new();
        click.set_button(3);
        let me = self.weak();
        let bb = b.clone();
        click.connect_pressed(move |_, _, _, _| {
            if let Some(p) = me.upgrade() {
                p.slide_menu(i, &bb);
            }
        });
        b.add_controller(click);
        b
    }

    fn slide_menu(&self, i: usize, parent: &gtk::Button) {
        let pop = gtk::Popover::new();
        pop.add_css_class("menu-popover");
        pop.set_parent(parent);
        let list = widgets::vbox(0);
        let item = |label: &str, f: Box<dyn Fn(&Present)>| {
            let b = gtk::Button::with_label(label);
            b.add_css_class("menu-item");
            if let Some(l) = b.child().and_downcast::<gtk::Label>() {
                l.set_xalign(0.0);
            }
            let me = self.weak();
            let p = pop.clone();
            b.connect_clicked(move |_| {
                p.popdown();
                if let Some(pr) = me.upgrade() {
                    f(&pr);
                }
            });
            list.append(&b);
        };
        let hidden = self.deck.borrow().slides.get(i).is_some_and(|s| s.hide_master);
        item(
            if hidden { "Show master objects" } else { "Hide master objects" },
            Box::new(move |p| {
                let (cur, master) = (p.current.get(), p.master_mode.get());
                p.master_mode.set(false);
                p.current.set(i);
                p.change_slide(|s| s.hide_master = !s.hide_master);
                p.current.set(cur);
                p.master_mode.set(master);
            }),
        );
        item(
            "Duplicate",
            Box::new(move |p| {
                let mut at = None;
                p.change_deck(|d| at = d.duplicate(i));
                if let Some(at) = at {
                    p.go_to(at);
                }
            }),
        );
        item("Move up", Box::new(move |p| p.move_slide(i, -1)));
        item("Move down", Box::new(move |p| p.move_slide(i, 1)));
        if self.deck.borrow().slides.len() > 1 {
            let del = gtk::Button::with_label("Delete slide");
            del.add_css_class("menu-item");
            del.add_css_class("danger-text");
            if let Some(l) = del.child().and_downcast::<gtk::Label>() {
                l.set_xalign(0.0);
            }
            let armed = Rc::new(Cell::new(false));
            let me = self.weak();
            let p = pop.clone();
            del.connect_clicked(move |b| {
                if !armed.replace(true) {
                    b.set_label("Click again to delete");
                    return;
                }
                p.popdown();
                if let Some(pr) = me.upgrade() {
                    pr.change_deck(|d| {
                        d.remove(i);
                    });
                    pr.go_to(i.min(pr.deck.borrow().slides.len() - 1));
                }
            });
            list.append(&del);
        }
        pop.set_child(Some(&list));
        pop.connect_closed(|p| p.unparent());
        pop.popup();
    }

    fn move_slide(&self, i: usize, by: i64) {
        let mut to = None;
        self.change_deck(|d| to = d.move_slide(i, by));
        if let Some(to) = to {
            self.go_to(to);
        }
    }

    // ---------- Canvas ----------

    /// The scale that fits the slide in the window, before zooming.
    fn fit_scale(&self) -> f64 {
        // The scrolled window's view: its adjustments' page sizes.
        let (vw, vh) = (self.desk.hadjustment().page_size(), self.desk.vadjustment().page_size());
        let (vw, vh) = if vw > 0.0 && vh > 0.0 { (vw, vh) } else { (self.canvas.width() as f64, self.canvas.height() as f64) };
        ((vw - 2.0 * PAD) / W).min((vh - 2.0 * PAD) / H).max(0.05)
    }

    /// The slide's place on the canvas: (x, y, scale) in widget pixels.
    fn frame(&self) -> (f64, f64, f64) {
        let (cw, ch) = (self.canvas.width() as f64, self.canvas.height() as f64);
        let scale = self.fit_scale() * self.zoom.get();
        ((cw - W * scale) / 2.0, (ch - H * scale) / 2.0, scale)
    }

    /// Zoomed in, the canvas is as large as the slide (the desk scrolls it).
    fn size_canvas(&self) {
        let (w, h) = if self.zoom.get() > 1.0 {
            let scale = self.fit_scale() * self.zoom.get();
            ((W * scale + 2.0 * PAD) as i32, (H * scale + 2.0 * PAD) as i32)
        } else {
            (-1, -1)
        };
        if self.canvas.size_request() != (w, h) {
            self.canvas.set_size_request(w, h);
        }
        self.canvas.queue_draw();
        self.place_edit();
    }

    fn to_slide(&self, x: f64, y: f64) -> (f64, f64) {
        let (ox, oy, s) = self.frame();
        ((x - ox) / s, (y - oy) / s)
    }

    fn draw_canvas(&self, cr: &cairo::Context, w: f64, h: f64) {
        let pal = crate::theme::palette();
        let (bg, accent, border) = (render::rgb(&pal.bg), render::rgb(&pal.accent), render::rgb(&pal.border));
        cr.set_source_rgb(bg.0, bg.1, bg.2);
        cr.rectangle(0.0, 0.0, w, h);
        cr.fill().ok();
        let (ox, oy, s) = self.frame();
        let slide = self.slide();
        let editing = self.editing.get();
        cr.save().ok();
        cr.translate(ox, oy);
        let hints = Hints {
            placeholders: true,
            skip_title: editing == Some(Target::Title),
            skip_body: editing == Some(Target::Body),
            skip_object: if let Some(Target::Object(id)) = editing { Some(id) } else { None },
            ..Default::default()
        };
        // The slide as it shows, master included; in master mode, the master alone.
        let shown = if self.master_mode.get() { slide.clone() } else { self.deck.borrow().composed(self.current.get()).unwrap_or_else(|| slide.clone()) };
        render::draw(cr, &ctx(), &shown, &self.media.borrow(), &self.pictures, (W * s, H * s), hints);
        cr.restore().ok();
        cr.set_source_rgb(border.0, border.1, border.2);
        cr.set_line_width(1.0);
        cr.rectangle(ox - 0.5, oy - 0.5, W * s + 1.0, H * s + 1.0);
        cr.stroke().ok();
        if self.master_mode.get() {
            // Say so above the slide, in the window's own accent.
            cr.set_source_rgb(accent.0, accent.1, accent.2);
            cr.set_line_width(2.0);
            cr.rectangle(ox - 2.0, oy - 2.0, W * s + 4.0, H * s + 4.0);
            cr.stroke().ok();
            let layout = self.canvas.create_pango_layout(Some("MASTER SLIDE · on every slide"));
            layout.set_font_description(Some(&gtk::pango::FontDescription::from_string("Noto Sans Bold 8.5")));
            cr.move_to(ox, oy - 20.0);
            pangocairo::functions::show_layout(cr, &layout);
        }
        // The selection: outlines, and a corner handle when one object is picked.
        let chosen = self.chosen();
        cr.set_source_rgb(accent.0, accent.1, accent.2);
        for o in slide.objects.iter().filter(|o| chosen.contains(&o.id)) {
            let (x, y, ow, oh) = (ox + o.x * s, oy + o.y * s, o.w * s, o.h * s);
            let lead = self.selected.get() == Some(o.id);
            // Drawn in the object's own turned frame.
            cr.save().ok();
            cr.translate(x + ow / 2.0, y + oh / 2.0);
            cr.rotate(o.rotation.to_radians());
            cr.translate(-ow / 2.0, -oh / 2.0);
            cr.set_line_width(if lead { 2.0 } else { 1.5 });
            if !lead {
                cr.set_dash(&[4.0, 3.0], 0.0);
            }
            cr.rectangle(0.0, 0.0, ow, oh);
            cr.stroke().ok();
            cr.set_dash(&[], 0.0);
            if lead && chosen.len() == 1 {
                // Resize at the corner; turn by the round handle above.
                cr.rectangle(ow - 5.0, oh - 5.0, 10.0, 10.0);
                cr.fill().ok();
                cr.set_line_width(1.5);
                cr.move_to(ow / 2.0, 0.0);
                cr.line_to(ow / 2.0, -ROTATE_GAP + 6.0);
                cr.stroke().ok();
                cr.arc(ow / 2.0, -ROTATE_GAP, 6.0, 0.0, std::f64::consts::TAU);
                cr.fill().ok();
            }
            cr.restore().ok();
        }
        // Snap guides while dragging.
        cr.set_line_width(1.0);
        for (vertical, at) in self.guides.borrow().iter() {
            if *vertical {
                cr.move_to(ox + at * s + 0.5, oy);
                cr.line_to(ox + at * s + 0.5, oy + H * s);
            } else {
                cr.move_to(ox, oy + at * s + 0.5);
                cr.line_to(ox + W * s, oy + at * s + 0.5);
            }
        }
        cr.stroke().ok();
        // Build order badges: which click brings each object in.
        let steps = slide.steps();
        for (n, st) in steps.iter().enumerate() {
            let Some(o) = slide.objects.iter().find(|o| o.id == st.id) else { continue };
            // Coming in at the top left corner, going out at the top right (in red).
            let (x, y) = if st.out { (ox + (o.x + o.w) * s, oy + o.y * s) } else { (ox + o.x * s, oy + o.y * s) };
            if st.out {
                cr.set_source_rgb(0.78, 0.24, 0.2);
            } else {
                cr.set_source_rgb(accent.0, accent.1, accent.2);
            }
            cr.arc(x, y, 9.0, 0.0, std::f64::consts::TAU);
            cr.fill().ok();
            let layout = self.canvas.create_pango_layout(Some(&(n + 1).to_string()));
            let mut font = gtk::pango::FontDescription::from_string("Noto Sans Bold 8");
            font.set_weight(gtk::pango::Weight::Bold);
            layout.set_font_description(Some(&font));
            let (tw, th) = layout.pixel_size();
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.move_to(x - tw as f64 / 2.0, y - th as f64 / 2.0);
            pangocairo::functions::show_layout(cr, &layout);
        }
    }

    fn connect_canvas(self: &Rc<Self>) {
        let me = Rc::downgrade(self);
        self.canvas.set_draw_func(move |_, cr, w, h| {
            if let Some(p) = me.upgrade() {
                p.draw_canvas(cr, w as f64, h as f64);
            }
        });
        let me = Rc::downgrade(self);
        self.canvas.connect_resize(move |_, _, _| {
            if let Some(p) = me.upgrade() {
                p.place_edit();
            }
        });
        let drag = gtk::GestureDrag::new();
        let me = Rc::downgrade(self);
        drag.connect_drag_begin(move |g, x, y| {
            if let Some(p) = me.upgrade() {
                p.press(x, y, g.current_event_state().contains(gdk::ModifierType::SHIFT_MASK));
            }
        });
        let me = Rc::downgrade(self);
        drag.connect_drag_update(move |g, dx, dy| {
            let Some(p) = me.upgrade() else { return };
            let s = p.frame().2;
            let (dx, dy) = (dx / s, dy / s);
            match p.drag.get() {
                // Alt turns snapping off.
                Drag::Move { .. } => {
                    let alt = gtk::prelude::EventControllerExt::current_event_state(g).contains(gdk::ModifierType::ALT_MASK);
                    p.drag_by(dx, dy, !alt)
                }
                Drag::Resize { id, w0, h0 } => p.with_object(id, |o| {
                    // The pointer's move, in the object's own turned frame.
                    let (sn, cs) = (-o.rotation).to_radians().sin_cos();
                    o.w = w0 + dx * cs - dy * sn;
                    o.h = h0 + dx * sn + dy * cs;
                }),
                Drag::Rotate { id, sx, sy } => {
                    let shift = gtk::prelude::EventControllerExt::current_event_state(g).contains(gdk::ModifierType::SHIFT_MASK);
                    p.with_object(id, |o| {
                        let (cx, cy) = o.center();
                        let a = (sy + dy - cy).atan2(sx + dx - cx).to_degrees() + 90.0;
                        // Shift turns in 15° steps; near-straight snaps straight.
                        let a = if shift { (a / 15.0).round() * 15.0 } else { a };
                        let straight = (a / 90.0).round() * 90.0;
                        let a = if (a - straight).abs() < 2.5 { straight } else { a };
                        let a = (a + 180.0).rem_euclid(360.0) - 180.0;
                        o.rotation = (a * 10.0).round() / 10.0;
                    });
                }
                Drag::None => {}
            }
        });
        let me = Rc::downgrade(self);
        drag.connect_drag_end(move |_, dx, dy| {
            let Some(p) = me.upgrade() else { return };
            p.end_move();
            if p.drag.get() != Drag::None {
                if dx.abs() + dy.abs() < 1.0 {
                    // Only a click: nothing moved, so no undo step.
                    p.undo.borrow_mut().pop();
                } else {
                    p.changed();
                }
            }
            p.drag.set(Drag::None);
        });
        self.canvas.add_controller(drag);
        let click = gtk::GestureClick::new();
        let me = Rc::downgrade(self);
        click.connect_pressed(move |_, n, x, y| {
            let Some(p) = me.upgrade() else { return };
            if n == 2 {
                let (sx, sy) = p.to_slide(x, y);
                let slide = p.slide();
                if let Some(o) = slide.objects.iter().rev().find(|o| o.contains(sx, sy)) {
                    match o.kind {
                        Kind::Text { .. } => p.begin_edit(Target::Object(o.id)),
                        Kind::Table { .. } | Kind::Chart { .. } => p.edit_data(o.id),
                        _ => {}
                    }
                }
            }
        });
        self.canvas.add_controller(click);
        let keys = gtk::EventControllerKey::new();
        let me = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, mods| {
            let Some(p) = me.upgrade() else { return glib::Propagation::Proceed };
            if p.canvas_key(key, mods) { glib::Propagation::Stop } else { glib::Propagation::Proceed }
        });
        self.canvas.add_controller(keys);
    }

    /// Move or resize an object while dragging (the undo step was taken at the start).
    fn with_object(&self, id: u32, f: impl FnOnce(&mut Object)) {
        let i = self.current.get();
        let master = self.master_mode.get();
        self.deck.borrow_mut().edit(i, master, |s| {
            if let Some(o) = s.objects.iter_mut().find(|o| o.id == id) {
                f(o);
                o.clamp();
            }
        });
        self.canvas.queue_draw();
    }

    fn press(&self, x: f64, y: f64, shift: bool) {
        let (sx, sy) = self.to_slide(x, y);
        let slide = self.slide();
        let s = self.frame().2;
        // The handles of the selected object (one object picked), in its own frame.
        if let Some(o) = self.selected.get().and_then(|id| slide.objects.iter().find(|o| o.id == id))
            && self.chosen().len() == 1
        {
            let (lx, ly) = o.to_local(sx, sy);
            if (lx - (o.x + o.w)).abs() * s <= 8.0 && (ly - (o.y + o.h)).abs() * s <= 8.0 {
                self.end_edit();
                self.push_undo();
                self.drag.set(Drag::Resize { id: o.id, w0: o.w, h0: o.h });
                return;
            }
            if (lx - (o.x + o.w / 2.0)).abs() * s <= 9.0 && (ly - o.y + ROTATE_GAP / s).abs() * s <= 9.0 {
                self.end_edit();
                self.push_undo();
                self.drag.set(Drag::Rotate { id: o.id, sx, sy });
                return;
            }
        }
        if let Some(o) = slide.objects.iter().rev().find(|o| o.contains(sx, sy)) {
            if self.editing.get() == Some(Target::Object(o.id)) {
                return;
            }
            self.end_edit();
            self.pick(o.id, shift);
            self.push_undo();
            self.begin_move();
            self.drag.set(Drag::Move { id: o.id, x0: o.x, y0: o.y });
            self.canvas.grab_focus();
            self.sync();
            self.canvas.queue_draw();
            return;
        }
        if shift {
            // Shift-click on empty slide keeps the selection.
            return;
        }
        self.clear_selection();
        let inside =
            |f: Option<Frame>| f.is_some_and(|(fx, fy, fw, fh, _, _)| sx >= fx && sx <= fx + fw && sy >= fy && sy <= fy + fh);
        if inside(title_frame(slide.layout)) {
            self.begin_edit(Target::Title);
        } else if inside(body_frame(slide.layout)) {
            self.begin_edit(Target::Body);
        } else {
            self.end_edit();
            self.canvas.grab_focus();
        }
        self.sync();
        self.canvas.queue_draw();
    }

    fn canvas_key(&self, key: gdk::Key, mods: gdk::ModifierType) -> bool {
        let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
        let step = if shift { 10.0 } else { 1.0 };
        let has = self.selected.get().is_some();
        match key {
            gdk::Key::Delete | gdk::Key::BackSpace if has => self.delete_chosen(),
            gdk::Key::Escape if has => {
                self.clear_selection();
                self.sync();
                self.canvas.queue_draw();
            }
            gdk::Key::Left if has => self.nudge(-step, 0.0),
            gdk::Key::Right if has => self.nudge(step, 0.0),
            gdk::Key::Up if has => self.nudge(0.0, -step),
            gdk::Key::Down if has => self.nudge(0.0, step),
            gdk::Key::Return if has => {
                if let Some(id) = self.selected.get() {
                    match self.slide().objects.iter().find(|o| o.id == id).map(|o| &o.kind) {
                        Some(Kind::Text { .. }) => self.begin_edit(Target::Object(id)),
                        Some(Kind::Table { .. } | Kind::Chart { .. }) => self.edit_data(id),
                        _ => {}
                    }
                }
            }
            gdk::Key::Page_Down | gdk::Key::Down | gdk::Key::Right => self.go_to(self.current.get() + 1),
            gdk::Key::Page_Up | gdk::Key::Up | gdk::Key::Left => self.go_to(self.current.get().saturating_sub(1)),
            gdk::Key::Home => self.go_to(0),
            gdk::Key::End => self.go_to(usize::MAX),
            _ => return false,
        }
        true
    }

    // ---------- Editing text on the slide ----------

    fn target_frame(&self, t: Target) -> Option<(f64, f64, f64, f64, f64, bool)> {
        let s = self.slide();
        match t {
            Target::Title => title_frame(s.layout).map(|f| (f.0, f.1, f.2, f.3, s.title_size.unwrap_or(f.4), f.5)),
            Target::Body => body_frame(s.layout).map(|f| (f.0, f.1, f.2, f.3, s.body_size.unwrap_or(f.4), f.5)),
            Target::Object(id) => s
                .objects
                .iter()
                .find(|o| o.id == id)
                .map(|o| (o.x + 6.0, o.y + 4.0, o.w - 12.0, o.h - 8.0, s.body_size.unwrap_or(24.0), false)),
        }
    }

    fn target_text(&self, t: Target) -> String {
        let s = self.slide();
        match t {
            Target::Title => s.title,
            Target::Body => s.body,
            Target::Object(id) => s
                .objects
                .iter()
                .find_map(|o| if let (true, Kind::Text { text }) = (o.id == id, &o.kind) { Some(text.clone()) } else { None })
                .unwrap_or_default(),
        }
    }

    fn begin_edit(&self, t: Target) {
        if self.editing.get() == Some(t) {
            return;
        }
        self.end_edit();
        let Some(_) = self.target_frame(t) else { return };
        self.editing.set(Some(t));
        if let Target::Object(id) = t {
            self.selected.set(Some(id));
        }
        self.syncing.set(true);
        self.edit.buffer().set_text(&self.target_text(t));
        self.syncing.set(false);
        self.edit.set_visible(true);
        self.place_edit();
        self.edit.grab_focus();
        let buf = self.edit.buffer();
        buf.place_cursor(&buf.end_iter());
        self.canvas.queue_draw();
        self.sync();
    }

    fn end_edit(&self) {
        if self.editing.take().is_some() {
            self.edit.set_visible(false);
            self.close_group();
            self.canvas.queue_draw();
            self.schedule_thumbs();
        }
    }

    /// Put the text editor over what it edits, at the slide's scale.
    fn place_edit(&self) {
        let Some(t) = self.editing.get() else { return };
        let Some((x, y, w, h, size, centred)) = self.target_frame(t) else { return };
        let (ox, oy, s) = self.frame();
        let slide = self.slide();
        self.edit.set_margin_start((ox + x * s) as i32);
        self.edit.set_margin_top((oy + y * s) as i32);
        self.edit.set_size_request((w * s).max(40.0) as i32, (h * s).max(24.0) as i32);
        self.edit.set_justification(if centred { gtk::Justification::Center } else { gtk::Justification::Left });
        let weight = if slide.bold {
            700
        } else if t == Target::Title {
            600
        } else {
            400
        };
        let px = size * s * 96.0 / 72.0 * 0.75;
        self.edit_css.load_from_string(&format!(
            "textview.slide-edit, textview.slide-edit > text {{ background: alpha({bg}, 0.92); color: {fg}; caret-color: {fg}; font-family: \"{font}\"; font-size: {px:.1}px; font-weight: {weight}; font-style: {style}; }}",
            bg = slide.background,
            fg = slide.color,
            font = slide.font.replace('"', ""),
            style = if slide.italic { "italic" } else { "normal" },
        ));
    }

    fn connect_text(self: &Rc<Self>) {
        let me = Rc::downgrade(self);
        self.edit.buffer().connect_changed(move |b| {
            let Some(p) = me.upgrade() else { return };
            if p.syncing.get() {
                return;
            }
            let Some(t) = p.editing.get() else { return };
            let (a, e) = b.bounds();
            let text = b.text(&a, &e, false).to_string();
            p.begin_typing();
            let (i, master) = (p.current.get(), p.master_mode.get());
            p.deck.borrow_mut().edit(i, master, |s| {
                match t {
                    Target::Title => s.title = text,
                    Target::Body => s.body = text,
                    Target::Object(id) => {
                        if let Some(Kind::Text { text: tx }) = s.objects.iter_mut().find(|o| o.id == id).map(|o| &mut o.kind) {
                            *tx = text;
                        }
                    }
                }
            });
            p.schedule_thumbs();
            doc::edited();
        });
        let keys = gtk::EventControllerKey::new();
        let me = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape
                && let Some(p) = me.upgrade()
            {
                p.end_edit();
                p.canvas.grab_focus();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.edit.add_controller(keys);
        let me = Rc::downgrade(self);
        self.notes.buffer().connect_changed(move |b| {
            let Some(p) = me.upgrade() else { return };
            if p.syncing.get() {
                return;
            }
            let (a, e) = b.bounds();
            let text = b.text(&a, &e, false).to_string();
            p.begin_typing();
            let i = p.current.get();
            if let Some(s) = p.deck.borrow_mut().slides.get_mut(i) {
                s.notes = text;
            }
            doc::edited();
        });
    }

    // ---------- Presenting ----------

    fn present(&self, from: usize) {
        self.end_edit();
        show::start(self.rc(), from);
    }

    // ---------- Files ----------

    fn load(&self, deck: Deck, media: BTreeMap<String, Vec<u8>>) {
        self.end_edit();
        *self.deck.borrow_mut() = deck;
        *self.media.borrow_mut() = media;
        self.pictures.clear();
        self.undo.borrow_mut().clear();
        self.redo.borrow_mut().clear();
        self.current.set(0);
        self.selected.set(None);
        self.canvas.queue_draw();
        // Rebuild every thumbnail: the slides are new.
        while let Some(c) = self.thumbs.first_child() {
            self.thumbs.remove(&c);
        }
        self.refresh_thumbs();
        self.sync();
    }

    fn native_bytes(&self) -> Result<Vec<u8>> {
        let d = self.deck.borrow();
        let used = d.image_names();
        let media: BTreeMap<String, Vec<u8>> =
            self.media.borrow().iter().filter(|(k, _)| used.contains(k)).map(|(k, v)| (k.clone(), v.clone())).collect();
        container::write(FORMAT, &d.to_json(), &media)
    }

    fn read_native(bytes: &[u8]) -> Result<(Deck, BTreeMap<String, Vec<u8>>)> {
        if container::is_zip(bytes) {
            let c = container::read(bytes, FORMAT)?;
            return Ok((Deck::from_json(&c.content)?, c.media));
        }
        Deck::from_legacy_json(bytes)
    }
}

/// Backgrounds and shape colours: light and dark.
const BACKGROUNDS: [&str; 10] =
    ["#ffffff", "#f4f1ea", "#eaf3f5", "#fff8f3", "#20263a", "#23364d", "#2f5bd3", "#284b4a", "#55395a", "#5273cf"];

fn font_families(w: &impl IsA<gtk::Widget>) -> Vec<String> {
    let mut names: Vec<String> = w.pango_context().list_families().iter().map(|f| f.name().to_string()).collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    if !names.iter().any(|n| n == DEFAULT_FONT) {
        names.insert(0, DEFAULT_FONT.to_string());
    }
    names
}

fn replace_in(text: &str, query: &str, with: &str, case: bool) -> (String, usize) {
    if query.is_empty() {
        return (text.to_string(), 0);
    }
    if case {
        return (text.replace(query, with), text.matches(query).count());
    }
    let lower = text.to_lowercase();
    if lower.len() != text.len() {
        return (text.to_string(), 0);
    }
    let q = query.to_lowercase();
    let mut out = String::new();
    let mut last = 0;
    let mut n = 0;
    for (i, _) in lower.match_indices(&q) {
        out.push_str(&text[last..i]);
        out.push_str(with);
        last = i + q.len();
        n += 1;
    }
    out.push_str(&text[last..]);
    (out, n)
}

fn replace_slide(s: &mut Slide, query: &str, with: &str, case: bool) -> usize {
    let mut n = 0;
    for t in [&mut s.title, &mut s.body, &mut s.notes] {
        let (r, k) = replace_in(t, query, with, case);
        *t = r;
        n += k;
    }
    for o in &mut s.objects {
        if let Kind::Text { text } = &mut o.kind {
            let (r, k) = replace_in(text, query, with, case);
            *text = r;
            n += k;
        }
    }
    n
}

impl Find for Present {
    fn find(&self, query: &str, case: bool, forward: bool) -> bool {
        let q = if case { query.to_string() } else { query.to_lowercase() };
        let d = self.deck.borrow();
        let hit = |s: &Slide| s.texts().iter().any(|t| if case { t.contains(&q) } else { t.to_lowercase().contains(&q) });
        let n = d.slides.len();
        let cur = self.current.get();
        let order: Vec<usize> =
            if forward { (1..=n).map(|k| (cur + k) % n).collect() } else { (1..=n).map(|k| (cur + n - k) % n).collect() };
        let found = order.into_iter().find(|i| hit(&d.slides[*i]));
        drop(d);
        match found {
            Some(i) => {
                window::navigate("document");
                self.go_to(i);
                true
            }
            None => false,
        }
    }

    fn replace(&self, query: &str, with: &str, case: bool) -> bool {
        let mut n = 0;
        self.change_slide(|s| n = replace_slide(s, query, with, case));
        Find::find(self, query, case, true) || n > 0
    }

    fn replace_all(&self, query: &str, with: &str, case: bool) -> usize {
        let mut n = 0;
        self.change_deck(|d| {
            for s in &mut d.slides {
                n += replace_slide(s, query, with, case);
            }
        });
        n
    }

    fn count(&self, query: &str, case: bool) -> usize {
        let d = self.deck.borrow();
        let mut n = 0;
        for s in &d.slides {
            for t in s.texts() {
                n += if case { t.matches(query).count() } else { t.to_lowercase().matches(&query.to_lowercase()).count() };
            }
        }
        n
    }
}

impl doc::Editor for Present {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    fn sidebar(&self) -> Option<SidebarGroup> {
        let me = self.weak();
        Some(SidebarGroup {
            title: "Slides".into(),
            add: Some((
                "New slide (Ctrl+M)".into(),
                Rc::new(move || {
                    if let Some(p) = me.upgrade() {
                        window::navigate("document");
                        p.add_slide();
                    }
                }),
            )),
            widget: self.thumbs.clone().upcast(),
        })
    }

    fn templates(&self) -> &'static [Template] {
        templates::TEMPLATES
    }

    fn new_document(&self, template: Option<&str>) {
        self.load(template.and_then(templates::build).unwrap_or_default(), BTreeMap::new());
    }

    fn open(&self, path: &Path) -> Result<()> {
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let bytes = std::fs::read(path)?;
        let (deck, media) = match ext.as_str() {
            "npresent" => Present::read_native(&bytes)?,
            "pptx" => pptx::read(&bytes)?,
            _ => bail!("Nexus Present opens .npresent and .pptx files"),
        };
        self.load(deck, media);
        Ok(())
    }

    fn save(&self, path: &Path) -> Result<Vec<String>> {
        self.end_edit();
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        match ext.as_str() {
            "npresent" => {
                crate::cmd::atomic_write(path, self.native_bytes()?)?;
                Ok(Vec::new())
            }
            "pptx" => {
                let (bytes, lost) = pptx::write(&self.deck.borrow(), &self.media.borrow())?;
                crate::cmd::atomic_write(path, bytes)?;
                Ok(lost)
            }
            _ => bail!("Nexus Present saves .npresent and .pptx files"),
        }
    }

    fn snapshot(&self) -> Result<Vec<u8>> {
        self.native_bytes()
    }

    fn restore(&self, bytes: &[u8]) -> Result<()> {
        let (d, m) = Present::read_native(bytes)?;
        self.load(d, m);
        Ok(())
    }

    fn undo(&self) {
        self.end_edit();
        self.close_group();
        let Some((prev, at)) = self.undo.borrow_mut().pop() else { return };
        let now = self.deck.replace(prev);
        self.redo.borrow_mut().push((now, self.current.get()));
        self.current.set(at);
        self.clear_selection();
        self.changed();
    }

    fn redo(&self) {
        self.end_edit();
        self.close_group();
        let Some((next, at)) = self.redo.borrow_mut().pop() else { return };
        let now = self.deck.replace(next);
        self.undo.borrow_mut().push((now, self.current.get()));
        self.current.set(at);
        self.clear_selection();
        self.changed();
    }

    fn status(&self) -> String {
        let n = self.deck.borrow().slides.len();
        let s = self.slide();
        format!("Slide {} of {} · {} · 16:9", self.current.get() + 1, n, s.layout.name())
    }

    fn find(&self) -> Option<Rc<dyn Find>> {
        Some(self.rc())
    }

    fn pages(&self) -> Rc<dyn Pages> {
        self.end_edit();
        Rc::new(render::DeckPages {
            deck: self.deck.borrow().clone(),
            media: self.media.borrow().clone(),
            pictures: Pictures::default(),
            ctx: crate::print::text_context(),
        })
    }

    fn set_zoom(&self, zoom: f64) {
        self.zoom.set(zoom);
        self.size_canvas();
    }

    fn focus(&self) {
        if self.editing.get().is_none() {
            self.canvas.grab_focus();
        }
    }

    fn shortcuts(&self) -> &'static [doc::Shortcut] {
        &[
            (&["F5"], "Present from the start"),
            (&["Shift", "F5"], "Present from this slide"),
            (&["Ctrl", "M"], "New slide"),
            (&["PgDn"], "Next slide (PgUp: the one before)"),
            (&["Alt", "↑"], "Move the slide up (Alt+↓: down)"),
            (&["Shift", "Click"], "Select more objects"),
            (&["Ctrl", "G"], "Group the selected objects (Ctrl+Shift+G: ungroup)"),
            (&["Alt", "Drag"], "Move without snapping to guides"),
            (&["Delete"], "Delete the selected objects"),
            (&["Arrows"], "Nudge the selected object (Shift: further)"),
            (&["Enter"], "Edit the selected text box, table or chart"),
            (&["Esc"], "Stop editing, or let go of the object"),
            (&["Ctrl", "B"], "Bold slide text (Ctrl+I: italic)"),
        ]
    }

    fn key(&self, key: gdk::Key, mods: gdk::ModifierType) -> bool {
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
        let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
        let alt = mods.contains(gdk::ModifierType::ALT_MASK);
        match key {
            gdk::Key::F5 => self.present(if shift { self.current.get() } else { 0 }),
            gdk::Key::Up if alt => self.move_slide(self.current.get(), -1),
            gdk::Key::Down if alt => self.move_slide(self.current.get(), 1),
            _ if ctrl => match key.to_lower() {
                gdk::Key::m => self.add_slide(),
                gdk::Key::g if self.canvas.has_focus() && shift => self.ungroup_chosen(),
                gdk::Key::g if self.canvas.has_focus() => self.group_chosen(),
                gdk::Key::b if self.notes.has_focus() || self.canvas.has_focus() || self.edit.has_focus() => {
                    let on = !self.slide().bold;
                    self.change_slide(move |s| s.bold = on);
                    self.place_edit();
                }
                gdk::Key::i if self.notes.has_focus() || self.canvas.has_focus() || self.edit.has_focus() => {
                    let on = !self.slide().italic;
                    self.change_slide(move |s| s.italic = on);
                    self.place_edit();
                }
                _ => return false,
            },
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
pub fn write_sample(path: &Path) {
    let mut deck = templates::build("celebration").expect("a template");
    deck.slides[1].objects.push(Object::new(1, Kind::Shape { shape: Shape::Ellipse, fill: "#e47b89".into() }, (700.0, 380.0, 160.0, 100.0)));
    let s = &mut deck.slides[1];
    s.transition = Some(Transition { kind: TransitionKind::MagicMove, secs: 1.0 });
    let mut chart = Object::new(10, Kind::sample_chart(), (500.0, 60.0, 420.0, 280.0));
    chart.build = Some(Build { order: 1, effect: Effect::Fade });
    s.objects.push(chart);
    let mut table = Object::new(11, Kind::table(3, 3), (40.0, 360.0, 400.0, 140.0));
    table.build = Some(Build { order: 2, effect: Effect::Fly });
    s.objects.push(table);
    for (i, shape) in [Shape::Star, Shape::Arrow, Shape::Hexagon, Shape::Rounded].into_iter().enumerate() {
        let mut o = Object::new(20 + i as u32, Kind::Shape { shape, fill: "#e0a43a".into() }, (40.0 + i as f64 * 110.0, 40.0, 90.0, 90.0));
        o.rotation = i as f64 * 15.0;
        o.shadow = i % 2 == 0;
        o.outline = (i > 1).then(|| Outline { color: "#202329".into(), width: 3.0 });
        s.objects.push(o);
    }
    for (id, x) in [(12, 470.0), (13, 560.0)] {
        let mut o = Object::new(id, Kind::Shape { shape: Shape::Rectangle, fill: "#5273cf".into() }, (x, 400.0, 70.0, 70.0));
        o.group = Some(1);
        s.objects.push(o);
    }
    deck.master.push(Object::new(1, Kind::Text { text: "{n}".into() }, (880.0, 500.0, 60.0, 30.0)));
    deck.slides[0].hide_master = true;
    deck.slides[1].objects.push(Object::new(30, Kind::Media { media: "clip-1.mp4".into(), video: true }, (560.0, 360.0, 300.0, 168.0)));
    let media: BTreeMap<String, Vec<u8>> = [("clip-1.mp4".to_string(), b"sample".to_vec())].into();
    let (bytes, _) = pptx::write(&deck, &media).expect("a pptx");
    std::fs::write(path, bytes).expect("written");
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::doc::Editor;

    /// Run from `crate::gtk_tests`: GTK lives on one thread, so the editors' checks
    /// run one after another in a single test.
    pub fn editor_behaves() {
        let dir = std::env::temp_dir().join(format!("nxo-present-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = Present::build();

        // Typing a title on the slide.
        p.begin_edit(Target::Title);
        p.edit.buffer().set_text("Hello");
        p.end_edit();
        assert_eq!(p.slide().title, "Hello");
        // Speaker notes.
        p.notes.buffer().set_text("Say hi");
        assert_eq!(p.slide().notes, "Say hi");
        p.close_group();

        // A shape: add, move with the keys, undo the move.
        p.add_object(Kind::Shape { shape: Shape::Rectangle, fill: "#5273cf".into() }, 200.0, 100.0);
        let id = p.selected.get().unwrap();
        let x0 = p.slide().objects[0].x;
        assert!(p.canvas_key(gdk::Key::Right, gdk::ModifierType::SHIFT_MASK));
        assert_eq!(p.slide().objects[0].x, x0 + 10.0);
        Editor::undo(&*p);
        assert_eq!(p.slide().objects[0].x, x0);
        Editor::redo(&*p);
        assert_eq!(p.slide().objects[0].id, id);

        // Slides: add one after, then move it up.
        p.add_slide();
        assert_eq!(p.current.get(), 1);
        assert_eq!(p.slide().layout, Layout::Content);
        p.move_slide(1, -1);
        assert_eq!(p.current.get(), 0);
        assert_eq!(p.deck.borrow().slides[1].title, "Hello");

        // Save and open, natively and as PowerPoint.
        let path = dir.join("deck.npresent");
        Editor::save(&*p, &path).unwrap();
        let before = p.deck.borrow().clone();
        Editor::new_document(&*p, None);
        Editor::open(&*p, &path).unwrap();
        assert_eq!(*p.deck.borrow(), before);
        let lost = Editor::save(&*p, &dir.join("deck.pptx")).unwrap();
        assert_eq!(lost, vec!["speaker notes"]);
        Editor::open(&*p, &dir.join("deck.pptx")).unwrap();
        assert_eq!(p.deck.borrow().slides[1].title, "Hello");

        // Find, replace and the PDF.
        assert_eq!(Find::count(&*p, "hello", false), 1);
        assert!(Find::find(&*p, "hello", false, true));
        assert_eq!(p.current.get(), 1);
        assert_eq!(Find::replace_all(&*p, "Hello", "Hi", true), 1);
        let pdf = dir.join("deck.pdf");
        crate::print::export_pdf(p.pages().as_ref(), &pdf).unwrap();
        assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
        assert_eq!(p.pages().count(), 2);

        // Several objects: shift-select, line up, space out, group.
        p.go_to(0);
        for (x, y) in [(50.0, 40.0), (300.0, 200.0), (700.0, 90.0)] {
            p.add_object(Kind::Shape { shape: Shape::Rectangle, fill: "#336699".into() }, 100.0, 60.0);
            p.change_object(|o| {
                o.x = x;
                o.y = y;
            });
        }
        let ids: Vec<u32> = p.slide().objects.iter().map(|o| o.id).collect();
        p.clear_selection();
        for id in &ids {
            p.pick(*id, true);
        }
        assert_eq!(p.chosen().len(), 3);
        p.align(arrange::Line::Top);
        assert!(p.slide().objects.iter().all(|o| o.y == 40.0));
        p.change_slide(|s| s.objects[1].x = 120.0);
        p.distribute(true);
        let xs: Vec<f64> = p.slide().objects.iter().map(|o| o.x).collect();
        assert_eq!(xs, [50.0, 375.0, 700.0]);
        p.group_chosen();
        assert!(p.slide().objects.iter().all(|o| o.group == Some(1)));
        // Clicking one member selects the group.
        p.clear_selection();
        p.pick(ids[0], false);
        assert_eq!(p.chosen().len(), 3);
        p.nudge(-500.0, 0.0);
        assert_eq!(p.slide().objects[0].x, 0.0);
        p.ungroup_chosen();
        assert!(p.slide().objects.iter().all(|o| o.group.is_none()));
        p.delete_chosen();
        assert!(p.slide().objects.is_empty());

        // In master mode new objects go on the master, and show on every slide.
        p.set_master_mode(true);
        p.add_slide_number();
        p.set_master_mode(false);
        assert_eq!(p.deck.borrow().master.len(), 1);
        assert!(p.slide().objects.is_empty());
        assert!(matches!(&p.deck.borrow().composed(0).unwrap().objects[0].kind, Kind::Text { text } if text == "1"));
        p.deck.borrow_mut().master.clear();

        // A table and a chart are drawn, and builds hide objects until their click.
        p.add_object(Kind::table(2, 2), 400.0, 120.0);
        p.add_object(Kind::sample_chart(), 400.0, 240.0);
        let chart = p.selected.get().unwrap();
        p.change_object(|o| o.build = Some(Build { order: 1, effect: Effect::Fade }));
        assert_eq!(p.slide().steps().iter().map(|s| s.id).collect::<Vec<_>>(), [chart]);
        crate::print::export_pdf(p.pages().as_ref(), &pdf).unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }
}
