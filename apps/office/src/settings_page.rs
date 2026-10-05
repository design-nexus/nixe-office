//! The groups shown in the settings card.

use crate::widgets::{self, Page};
use crate::{app, prefs, theme, window};
use gtk::prelude::*;

pub fn build(page: &Page) {
    let p = prefs::get();
    let kind = app::kind();

    // ----- Editing, per app -----
    match kind {
        app::Kind::Write => {
            let g = page.group("Writing");
            let (r, _) = widgets::switch_row(
                "Check spelling as you type",
                "Underline misspelled words. Right-click one for suggestions.",
                p.spellcheck,
                |on| {
                    prefs::update(|p| p.spellcheck = on);
                    crate::write::spelling_changed();
                },
            );
            g.add(&r);
            let (r, _) = widgets::switch_row(
                "Smart quotes and symbols",
                "As you type, \"quotes\" become curly, -- a dash —, ... an ellipsis …, and (c) ©.",
                p.autocorrect,
                |on| prefs::update(|p| p.autocorrect = on),
            );
            g.add(&r);
            let langs = crate::write::spelling_languages();
            if !langs.is_empty() {
                let mut options = vec![(String::new(), "System language".to_string())];
                options.extend(langs);
                let (r, _) = widgets::choice_row(
                    "Spelling language",
                    "The dictionary used for checking.",
                    options,
                    &p.spelling_language,
                    |id| {
                        prefs::update(|p| p.spelling_language = id);
                        crate::write::spelling_changed();
                    },
                );
                g.add(&r);
            }
        }
        app::Kind::Sheets => {}
        app::Kind::Present => {
            let g = page.group("Presenting");
            let (r, _) = widgets::switch_row(
                "Presenter view",
                "With a second screen connected, show your notes, the next slide and a timer on this one while presenting.",
                p.presenter_view,
                |on| prefs::update(|p| p.presenter_view = on),
            );
            g.add(&r);
        }
    }

    // ----- Files -----
    let g = page.group("Files");
    g.add(&widgets::segmented_row(
        "Paper size",
        "For new files, PDF export and printing.",
        widgets::opts(&crate::print::PAPERS.map(|p| (p.0, p.1))),
        &p.paper,
        |v| prefs::update(|p| p.paper = v),
    ));
    let (r, _) = widgets::switch_row(
        "Keep unsaved work",
        "Save a copy of your changes every few seconds, so they can be brought back if the app closes unexpectedly.",
        p.recovery,
        |on| prefs::update(|p| p.recovery = on),
    );
    g.add(&r);

    // ----- This window -----
    let g = page.group(&format!("{} window", kind.name()));
    let app_themes = theme::all();
    let options: Vec<(String, String)> = app_themes.iter().map(|t| (t.id.clone(), t.name.clone())).collect();
    let (theme_row, theme_dd) = widgets::choice_row(
        "Theme",
        "Dracula, Catppuccin, Tokyo Night, One Dark Pro and more. Add your own in <tt>~/.config/nexus-office/themes</tt>.",
        options,
        &p.theme,
        |id| {
            prefs::update(|p| {
                p.theme = id;
                p.mode = prefs::ThemeMode::Theme;
            });
            theme::apply();
        },
    );
    theme_dd.set_sensitive(p.mode == prefs::ThemeMode::Theme || !theme::omarchy_available());
    if theme::omarchy_available() {
        let dd = theme_dd.clone();
        let (r, _) = widgets::switch_row(
            "Follow Omarchy theme",
            "Match the desktop's colors and update live whenever the Omarchy theme changes.",
            p.mode == prefs::ThemeMode::Omarchy,
            move |on| {
                prefs::update(|p| p.mode = if on { prefs::ThemeMode::Omarchy } else { prefs::ThemeMode::Theme });
                dd.set_sensitive(!on);
                theme::apply();
            },
        );
        g.add(&r);
    }
    g.add(&theme_row);

    let swatches = widgets::hbox(4);
    let refresh_swatches = {
        let swatches = swatches.clone();
        move || {
            while let Some(c) = swatches.first_child() {
                swatches.remove(&c);
            }
            let pal = theme::palette();
            for c in [&pal.bg, &pal.surface, &pal.muted, &pal.text, &pal.accent, &pal.danger] {
                let s = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                s.add_css_class("swatch");
                let provider = gtk::CssProvider::new();
                provider.load_from_string(&format!("box {{ background: {c}; }}"));
                #[allow(deprecated)]
                s.style_context().add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_USER);
                swatches.append(&s);
            }
        }
    };
    refresh_swatches();
    theme::subscribe(&swatches, refresh_swatches);
    g.add(&widgets::row("Current colors", "", Some(swatches.upcast_ref())));
    g.note("The page itself keeps its own colors, so what you see is what prints.");

    let (r, _) = widgets::switch_row("Glow", "Soft accent glow around focused and selected elements.", p.glow, |on| {
        prefs::update(|p| p.glow = on);
        theme::apply();
    });
    g.add(&r);
    let (r, _) =
        widgets::switch_row("Reduce motion", "Turn off transitions and animations in this window.", p.reduce_motion, |on| {
            prefs::update(|p| p.reduce_motion = on);
            theme::apply();
        });
    g.add(&r);

    // ----- Keyboard -----
    let g = page.group("Keyboard");
    for (_, keys) in window::shortcut_groups() {
        for (k, what) in keys {
            g.add(&widgets::row(what, "", Some(widgets::key_caps(k).upcast_ref())));
        }
    }
    g.note(&format!("Open a file from a terminal or a binding with <tt>nexus-{} FILE</tt>.", kind.id()));
}
