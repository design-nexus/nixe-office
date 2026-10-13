//! A shape or chart in the text: drawn with Cairo at the size the user chose,
//! the same way it prints.

use super::model::{Drawing, Figure};
use super::picture::PX;
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FigureView {
        pub drawing: RefCell<Option<Drawing>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FigureView {
        const NAME: &'static str = "NxoFigure";
        type Type = super::FigureView;
        type Interfaces = (gdk::Paintable,);
    }

    impl ObjectImpl for FigureView {}

    impl PaintableImpl for FigureView {
        fn intrinsic_width(&self) -> i32 {
            self.drawing.borrow().as_ref().map(|d| (d.width * PX).round() as i32).unwrap_or(1)
        }
        fn intrinsic_height(&self) -> i32 {
            self.drawing.borrow().as_ref().map(|d| (d.height * PX).round() as i32).unwrap_or(1)
        }
        fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            let Some(d) = self.drawing.borrow().clone() else { return };
            let Some(snap) = snapshot.downcast_ref::<gtk::Snapshot>() else { return };
            let cr = snap.append_cairo(&gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32));
            // Drawn in points, scaled to the screen.
            cr.scale(width / d.width.max(1.0), height / d.height.max(1.0));
            draw(&cr, &d.drawing, (0.0, 0.0, d.width, d.height));
        }
    }
}

glib::wrapper! {
    pub struct FigureView(ObjectSubclass<imp::FigureView>) @implements gdk::Paintable;
}

impl FigureView {
    pub fn new(d: &Drawing) -> FigureView {
        let f: FigureView = glib::Object::new();
        f.imp().drawing.replace(Some(d.clone()));
        f
    }

    pub fn drawing(&self) -> Drawing {
        self.imp().drawing.borrow().clone().expect("a figure has its drawing")
    }

    pub fn set_drawing(&self, d: Drawing) {
        self.imp().drawing.replace(Some(d));
        self.invalidate_size();
        self.invalidate_contents();
    }
}

/// Draw a shape or chart filling (x, y, w, h), in points.
pub fn draw(cr: &cairo::Context, figure: &Figure, (x, y, w, h): (f64, f64, f64, f64)) {
    match figure {
        Figure::Shape { shape, fill } => {
            crate::present::render::shape_path(cr, *shape, (x, y, w, h));
            let (r, g, b) = crate::present::render::rgb(fill);
            cr.set_source_rgb(r, g, b);
            cr.fill().ok();
        }
        Figure::Chart { chart, names, labels, series } => {
            let ctx = crate::print::text_context();
            crate::sheets::chart::draw(cr, &ctx, *chart, &crate::sheets::chart::Data { names, labels, series }, (x, y, w, h));
        }
    }
}

// ---------- In the editor ----------

use super::Writer;
use crate::present::model::Shape;
use crate::{color, widgets};
use std::rc::Rc;

/// Fills offered for shapes in a document.
const FILLS: &[&str] = &["#5273cf", "#3f9e8f", "#e47b89", "#e0a43a", "#8a6fd1", "#4fa3c7", "#d9644f", "#6b9e4a", "#202329", "#9aa0aa"];

impl Writer {
    pub(super) fn figure_at_cursor(&self) -> Option<FigureView> {
        let it = self.cursor_iter();
        let mut before = it;
        it.paintable()
            .and_downcast::<FigureView>()
            .or_else(|| if before.backward_char() { before.paintable().and_downcast::<FigureView>() } else { None })
    }

    fn insert_figure(&self, figure: Figure, (width, height): (f64, f64)) {
        self.checkpoint();
        let b = self.tb();
        let mut at = match b.selection_bounds() {
            Some((_, e)) => e,
            None => self.cursor_iter(),
        };
        b.insert_paintable(&mut at, &FigureView::new(&Drawing { drawing: figure, width, height }));
        self.edited();
        self.view.grab_focus();
    }

    /// Change the figure beside the cursor, as one undo step.
    pub(super) fn change_figure(&self, f: impl FnOnce(&mut Drawing)) {
        let Some(fig) = self.figure_at_cursor() else { return };
        self.checkpoint();
        let mut d = fig.drawing();
        f(&mut d);
        fig.set_drawing(d);
        self.view.queue_resize();
        self.edited();
    }

    /// A menu of shapes to put in the text.
    pub(super) fn shapes_menu(self: &Rc<Self>) -> gtk::MenuButton {
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("nxo-shapes-symbolic");
        menu.add_css_class("bar-button");
        menu.set_tooltip_text(Some("Insert a shape"));
        menu.set_valign(gtk::Align::Center);
        let pop = gtk::Popover::new();
        let grid = gtk::FlowBox::new();
        grid.set_selection_mode(gtk::SelectionMode::None);
        grid.set_max_children_per_line(4);
        grid.set_min_children_per_line(4);
        grid.add_css_class("prompt-box");
        for (i, shape) in Shape::ALL.into_iter().enumerate() {
            let fill = FILLS[i % FILLS.len()];
            let b = gtk::Button::new();
            b.add_css_class("flat");
            b.set_tooltip_text(Some(shape.name()));
            let area = gtk::DrawingArea::new();
            area.set_content_width(34);
            area.set_content_height(28);
            let f = Figure::Shape { shape, fill: fill.into() };
            area.set_draw_func(move |_, cr, w, h| draw(cr, &f, (2.0, 2.0, w as f64 - 4.0, h as f64 - 4.0)));
            b.set_child(Some(&area));
            let me = Rc::downgrade(self);
            let p = pop.clone();
            b.connect_clicked(move |_| {
                p.popdown();
                if let Some(w) = me.upgrade() {
                    let size = match shape {
                        Shape::Arrow => (160.0, 70.0),
                        Shape::Rectangle | Shape::Rounded => (160.0, 100.0),
                        _ => (110.0, 110.0),
                    };
                    w.insert_figure(Figure::Shape { shape, fill: fill.into() }, size);
                }
            });
            grid.append(&b);
        }
        pop.set_child(Some(&grid));
        menu.set_popover(Some(&pop));
        menu
    }

    pub(super) fn insert_chart(&self) {
        let width = self.setup.borrow().column_width().min(400.0);
        self.insert_figure(Figure::sample_chart(), (width, width * 0.6));
    }

    /// Shown beside a shape or chart: its size, a shape's colour, a chart's numbers.
    pub(super) fn figure_tools(self: &Rc<Self>) -> (gtk::Box, gtk::Scale, Rc<color::ColorButton>, gtk::Button) {
        let g = widgets::hbox(2);
        g.add_css_class("tool-group");
        g.append(&widgets::label("Drawing", "dim"));
        let width = gtk::Scale::with_range(gtk::Orientation::Horizontal, 10.0, 100.0, 1.0);
        width.set_size_request(120, -1);
        width.set_tooltip_text(Some("Width, as a share of the text width"));
        let me = Rc::downgrade(self);
        width.connect_value_changed(move |s| {
            if let Some(w) = me.upgrade()
                && !w.syncing.get()
            {
                let target = w.setup.borrow().column_width() * s.value() / 100.0;
                w.change_figure(|d| {
                    let ratio = d.height / d.width.max(1.0);
                    d.width = target.max(16.0);
                    d.height = d.width * ratio;
                });
            }
        });
        g.append(&width);
        let me = Rc::downgrade(self);
        let fill = color::button("nxo-paint-bucket-symbolic", "Shape color", FILLS, "Blue", move |c| {
            if let Some(w) = me.upgrade() {
                let c = c.unwrap_or_else(|| FILLS[0].into());
                w.change_figure(|d| {
                    if let Figure::Shape { fill, .. } = &mut d.drawing {
                        *fill = c.clone();
                    }
                });
            }
        });
        g.append(&fill.button);
        let data = gtk::Button::with_label("Chart data…");
        let me = Rc::downgrade(self);
        data.connect_clicked(move |_| {
            let Some(w) = me.upgrade() else { return };
            let Some(fig) = w.figure_at_cursor() else { return };
            let Figure::Chart { chart, names, labels, series } = fig.drawing().drawing else { return };
            let me = Rc::downgrade(&w);
            crate::present::data::edit_chart(chart, &names, &labels, &series, move |new| {
                let Some(w) = me.upgrade() else { return };
                if let crate::present::model::Kind::Chart { chart, names, labels, series } = new {
                    w.change_figure(|d| d.drawing = Figure::Chart { chart, names, labels, series });
                }
            });
        });
        g.append(&data);
        g.set_visible(false);
        (g, width, fill, data)
    }

    /// Show the figure tools for the figure beside the cursor.
    pub(super) fn sync_figure_tools(&self) {
        let fig = self.figure_at_cursor();
        let Some(c) = self.controls.borrow().as_ref().map(|c| (c.figure.clone(), c.figure_width.clone(), c.figure_fill.clone(), c.figure_data.clone()))
        else {
            return;
        };
        let (group, width, fill, data) = c;
        group.set_visible(fig.is_some());
        let Some(fig) = fig else { return };
        let d = fig.drawing();
        width.set_value((d.width / self.setup.borrow().column_width() * 100.0).round());
        let is_shape = matches!(d.drawing, Figure::Shape { .. });
        fill.button.set_visible(is_shape);
        data.set_visible(!is_shape);
        if let Figure::Shape { fill: f, .. } = &d.drawing {
            fill.set_current(Some(f));
        }
    }
}
