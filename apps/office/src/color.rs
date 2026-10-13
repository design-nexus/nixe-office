//! A colour picker for toolbars: an icon with a strip in the current colour,
//! opening swatches, a hex field and a "none" choice.

use crate::widgets;
use gtk::prelude::*;
use std::rc::Rc;

pub const TEXT: &[&str] =
    &["#202329", "#5f6672", "#c62828", "#d84315", "#ad5b00", "#2e7d32", "#00838f", "#1565c0", "#4527a0", "#ad1457"];
pub const HIGHLIGHT: &[&str] = &["#fff59d", "#ffe0b2", "#f8bbd0", "#e1bee7", "#bbdefb", "#b2ebf2", "#c8e6c9", "#e0e0e0"];

pub struct ColorButton {
    pub button: gtk::MenuButton,
    strip: gtk::Box,
    strip_css: gtk::CssProvider,
}

/// `none` names the "no colour" choice ("Automatic", "None").
pub fn button(
    icon: &str,
    tooltip: &str,
    swatches: &[&str],
    none: &str,
    on_pick: impl Fn(Option<String>) + 'static,
) -> Rc<ColorButton> {
    let on_pick: Rc<dyn Fn(Option<String>)> = Rc::new(on_pick);
    let button = gtk::MenuButton::new();
    button.add_css_class("colour-button");
    button.set_tooltip_text(Some(tooltip));
    button.set_valign(gtk::Align::Center);
    let face = widgets::vbox(1);
    face.set_halign(gtk::Align::Center);
    face.set_valign(gtk::Align::Center);
    face.append(&gtk::Image::from_icon_name(icon));
    let strip = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    strip.add_css_class("colour-strip");
    face.append(&strip);
    button.set_child(Some(&face));

    let popover = gtk::Popover::new();
    let content = widgets::vbox(8);
    content.add_css_class("colour-popover");
    let grid = gtk::FlowBox::new();
    grid.set_selection_mode(gtk::SelectionMode::None);
    grid.set_max_children_per_line(5);
    grid.set_min_children_per_line(5);
    for c in swatches {
        let b = gtk::Button::new();
        b.add_css_class("swatch-button");
        b.set_tooltip_text(Some(c));
        b.set_child(Some(&dot(c)));
        let (pick, p, c) = (on_pick.clone(), popover.clone(), c.to_string());
        b.connect_clicked(move |_| {
            p.popdown();
            pick(Some(c.clone()));
        });
        grid.append(&b);
    }
    content.append(&grid);
    let row = widgets::hbox(6);
    row.add_css_class("colour-row");
    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some("#rrggbb"));
    entry.set_width_chars(9);
    entry.set_max_length(7);
    entry.set_hexpand(true);
    let (pick, p) = (on_pick.clone(), popover.clone());
    entry.connect_activate(move |e| {
        let t = e.text().trim().to_lowercase();
        let t = if t.starts_with('#') { t } else { format!("#{t}") };
        if t.len() == 7 && t[1..].chars().all(|c| c.is_ascii_hexdigit()) {
            e.remove_css_class("error");
            p.popdown();
            pick(Some(t));
        } else {
            e.add_css_class("error");
        }
    });
    row.append(&entry);
    let clear = gtk::Button::with_label(none);
    let (pick, p) = (on_pick, popover.clone());
    clear.connect_clicked(move |_| {
        p.popdown();
        pick(None);
    });
    row.append(&clear);
    content.append(&row);
    popover.set_child(Some(&content));
    button.set_popover(Some(&popover));
    let strip_css = gtk::CssProvider::new();
    #[allow(deprecated)]
    strip.style_context().add_provider(&strip_css, gtk::STYLE_PROVIDER_PRIORITY_USER);
    let cb = Rc::new(ColorButton { button, strip, strip_css });
    cb.set_current(None);
    cb
}

/// A round swatch of colour `c`.
pub fn dot(c: &str) -> gtk::Box {
    let d = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    d.add_css_class("dot");
    paint(&d, c);
    d
}

/// Give a box a fixed background colour (page colours, not theme colours).
pub fn paint(w: &impl IsA<gtk::Widget>, c: &str) {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(&format!("* {{ background: {c}; }}"));
    #[allow(deprecated)]
    w.as_ref().style_context().add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_USER);
}

impl ColorButton {
    /// Show `c` in the strip under the icon (`None`: no strip colour).
    pub fn set_current(&self, c: Option<&str>) {
        if c.is_some() {
            self.strip.remove_css_class("empty");
        } else {
            self.strip.add_css_class("empty");
        }
        self.strip_css.load_from_string(&format!("* {{ background: {}; }}", c.unwrap_or("transparent")));
    }
}
