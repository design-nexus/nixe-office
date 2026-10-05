//! Shapes to add, turning objects, and their borders.

use super::Present;
use super::model::*;
use super::render;
use crate::{color, widgets};
use gtk::prelude::*;
use std::rc::Rc;

/// Shape fills, one per shape so a new slide doesn't look all one colour.
const SHAPE_FILLS: [&str; 8] = ["#5273cf", "#3f9e8f", "#e47b89", "#e0a43a", "#8a6fd1", "#4fa3c7", "#d9644f", "#6b9e4a"];

impl Present {
    /// A menu of every shape, each drawn as it will look.
    pub(super) fn shapes_button(self: &Rc<Self>) -> gtk::MenuButton {
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("nxo-shapes-symbolic");
        menu.add_css_class("bar-button");
        menu.set_tooltip_text(Some("Add a shape"));
        menu.set_valign(gtk::Align::Center);
        let pop = gtk::Popover::new();
        let grid = gtk::FlowBox::new();
        grid.set_selection_mode(gtk::SelectionMode::None);
        grid.set_max_children_per_line(4);
        grid.set_min_children_per_line(4);
        grid.add_css_class("prompt-box");
        for (i, shape) in Shape::ALL.into_iter().enumerate() {
            let b = gtk::Button::new();
            b.add_css_class("flat");
            b.set_tooltip_text(Some(shape.name()));
            let area = gtk::DrawingArea::new();
            area.set_content_width(34);
            area.set_content_height(28);
            let fill = SHAPE_FILLS[i % SHAPE_FILLS.len()];
            area.set_draw_func(move |_, cr, w, h| {
                render::shape_path(cr, shape, (2.0, 2.0, w as f64 - 4.0, h as f64 - 4.0));
                let (r, g, b) = render::rgb(fill);
                cr.set_source_rgb(r, g, b);
                cr.fill().ok();
            });
            b.set_child(Some(&area));
            let me = Rc::downgrade(self);
            let p = pop.clone();
            b.connect_clicked(move |_| {
                p.popdown();
                if let Some(pr) = me.upgrade() {
                    let (w, h) = match shape {
                        Shape::Ellipse | Shape::Star | Shape::Hexagon | Shape::Diamond => (200.0, 200.0),
                        Shape::Arrow => (260.0, 110.0),
                        Shape::Triangle => (220.0, 190.0),
                        _ => (240.0, 140.0),
                    };
                    pr.add_object(Kind::Shape { shape, fill: fill.into() }, w, h);
                }
            });
            grid.append(&b);
        }
        pop.set_child(Some(&grid));
        menu.set_popover(Some(&pop));
        menu
    }

    /// Turn the selection: an angle, quarter turns, or back to straight.
    pub(super) fn rotate_button(self: &Rc<Self>) -> gtk::MenuButton {
        let menu = gtk::MenuButton::new();
        menu.set_icon_name("nxo-rotate-cw-symbolic");
        menu.add_css_class("bar-button");
        menu.set_tooltip_text(Some("Rotate (or drag the round handle above the object; Shift for 15° steps)"));
        menu.set_valign(gtk::Align::Center);
        let pop = gtk::Popover::new();
        let content = widgets::vbox(10);
        content.add_css_class("prompt-box");
        content.append(&widgets::label("Rotate", "popover-title"));
        let angle = gtk::SpinButton::with_range(-180.0, 180.0, 1.0);
        angle.set_digits(0);
        let row = widgets::hbox(6);
        row.append(&angle);
        row.append(&widgets::label("degrees", "dim"));
        content.append(&row);
        let turns = widgets::hbox(6);
        let left = widgets::bar_button("nxo-rotate-ccw-symbolic", "A quarter turn left");
        let right = widgets::bar_button("nxo-rotate-cw-symbolic", "A quarter turn right");
        let straight = gtk::Button::with_label("Straighten");
        turns.append(&left);
        turns.append(&right);
        turns.append(&straight);
        content.append(&turns);
        pop.set_child(Some(&content));
        menu.set_popover(Some(&pop));
        let me = Rc::downgrade(self);
        let a2 = angle.clone();
        pop.connect_show(move |_| {
            if let Some(p) = me.upgrade() {
                let r = p.selected.get().and_then(|id| p.slide().objects.into_iter().find(|o| o.id == id)).map(|o| o.rotation);
                p.syncing.set(true);
                a2.set_value(r.unwrap_or(0.0));
                p.syncing.set(false);
            }
        });
        let me = Rc::downgrade(self);
        angle.connect_value_changed(move |sp| {
            if let Some(p) = me.upgrade()
                && !p.syncing.get()
            {
                let v = sp.value();
                p.change_object(move |o| o.rotation = v);
            }
        });
        for (b, by) in [(&left, -90.0), (&right, 90.0), (&straight, 0.0)] {
            let me = Rc::downgrade(self);
            let a = angle.clone();
            b.connect_clicked(move |_| {
                let Some(p) = me.upgrade() else { return };
                p.change_object(move |o| {
                    o.rotation = if by == 0.0 { 0.0 } else { ((o.rotation + by + 180.0).rem_euclid(360.0)) - 180.0 };
                });
                let r = p.selected.get().and_then(|id| p.slide().objects.into_iter().find(|o| o.id == id)).map(|o| o.rotation);
                p.syncing.set(true);
                a.set_value(r.unwrap_or(0.0));
                p.syncing.set(false);
            });
        }
        menu
    }

    /// A border: its colour (or none) and thickness.
    pub(super) fn outline_button(self: &Rc<Self>) -> gtk::Box {
        let row = widgets::hbox(2);
        let mut swatches: Vec<&str> = color::TEXT.to_vec();
        swatches.push("#ffffff");
        let me = Rc::downgrade(self);
        let colour = color::button("nxo-pen-line-symbolic", "Border color", &swatches, "No border", move |c| {
            if let Some(p) = me.upgrade() {
                p.change_object(move |o| {
                    o.outline = c.clone().map(|color| Outline { color, width: o.outline.as_ref().map(|l| l.width).unwrap_or(2.0) });
                });
            }
        });
        row.append(&colour.button);
        const WIDTHS: [(&str, f64); 4] = [("1 pt", 1.0), ("2 pt", 2.0), ("4 pt", 4.0), ("8 pt", 8.0)];
        let width = gtk::DropDown::from_strings(&WIDTHS.iter().map(|w| w.0).collect::<Vec<_>>());
        width.add_css_class("size-picker");
        width.set_tooltip_text(Some("Border thickness"));
        width.set_selected(1);
        let me = Rc::downgrade(self);
        width.connect_selected_notify(move |d| {
            if let Some(p) = me.upgrade()
                && !p.syncing.get()
            {
                let w = WIDTHS[(d.selected() as usize).min(3)].1;
                p.change_object(move |o| {
                    let color = o.outline.as_ref().map(|l| l.color.clone()).unwrap_or_else(|| "#202329".into());
                    o.outline = Some(Outline { color, width: w });
                });
            }
        });
        row.append(&width);
        row
    }
}
