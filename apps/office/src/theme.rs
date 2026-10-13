//! Theming: the shared Nixe theme engine, given this app's stylesheet.

pub use nixe_theme::*;

use crate::prefs;

/// Install the stylesheet and the current palette, and follow Omarchy live.
pub fn install() {
    nixe_theme::install(Style {
        window_class: "office-window",
        base_skip: &[
            "window.@APP@ scrollbar",
            "window.@APP@ scrollbar slider",
            ".toast",
        ],
        css: include_str!("style.css"),
        no_glow_css: "\n.theme-card.selected, .toast, window.office-window .page-card.selected { box-shadow: 0 0 0 1px alpha(@theme_accent, 0.6); }\n\
             window.office-window scale > trough > slider { box-shadow: 0 0 0 1px alpha(@theme_accent, 0.72); }\n",
        extra_colors: None,
        prefs: || {
            let p = prefs::get();
            ThemePrefs { mode: p.mode, theme: p.theme, glow: p.glow, reduce_motion: p.reduce_motion }
        },
        set_theme: Some(|mode, theme| {
            prefs::update(|p| {
                p.mode = mode;
                p.theme = theme;
            })
        }),
    });
}
