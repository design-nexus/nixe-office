//! Settings as a card over the window: the groups listed on the left, the
//! chosen group's options on the right, and a search across all of them.

use crate::{app, doc, prefs, widgets, window};
use gtk::prelude::*;
use nyx_ui::settings_card::{self, About, Spec, Standard};

pub use nyx_ui::settings_card::{escape, fit, focus_search, is_open};

/// Each settings group's icon and one-line summary in the dialog's list.
const GROUPS: &[(&str, &str, &str)] = &[
    ("Writing", "nxo-file-text-symbolic", "Spelling"),
    ("Presenting", "nxo-presentation-symbolic", "Presenter view"),
    ("Files", "nxo-files-symbolic", "Paper, unsaved work"),
];

/// Every shortcut of this app in one list, for the card's Keyboard group.
fn shortcuts() -> &'static [doc::Shortcut] {
    static ALL: std::sync::OnceLock<Vec<doc::Shortcut>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| window::shortcut_groups().into_iter().flat_map(|(_, keys)| keys).collect())
}

fn spec() -> Spec {
    let kind = app::kind();
    Spec {
        groups: GROUPS,
        search_text: None,
        standard: Some(Standard {
            app: kind.id(),
            about: About {
                name: kind.full_name(),
                icon: match kind {
                    app::Kind::Write => "io.github.design_nexus.nyx.Write",
                    app::Kind::Sheets => "io.github.design_nexus.nyx.Sheets",
                    app::Kind::Present => "io.github.design_nexus.nyx.Present",
                },
                version: env!("CARGO_PKG_VERSION"),
                description: match kind {
                    app::Kind::Write => "Documents, from letters to reports.",
                    app::Kind::Sheets => "Spreadsheets with formulas and charts.",
                    app::Kind::Present => "Slides, presented on any screen.",
                },
                licence: "MIT",
                website: "https://github.com/design-nexus/nyx-office",
                issues: "https://github.com/design-nexus/nyx-office/issues",
                credits: &[],
            },
            shortcuts: shortcuts(),
            reduce_motion: || prefs::get().reduce_motion,
            set_reduce_motion: |on| prefs::update(|p| p.reduce_motion = on),
        }),
    }
}

pub fn open() {
    let (Some(overlay), Some(window)) = (window::overlay(), window::window()) else { return };
    settings_card::open(&overlay, &window, spec(), || {
        let page = widgets::page("settings");
        crate::settings_page::build(&page);
        (page.root.upcast(), page.body)
    });
}
