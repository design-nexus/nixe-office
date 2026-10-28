//! Settings as a card over the window: the groups listed on the left, the
//! chosen group's options on the right, and a search across all of them.

use crate::{widgets, window};
use gtk::prelude::*;
use nyx_ui::settings_card::{self, Spec};

pub use nyx_ui::settings_card::{escape, fit, focus_search, is_open};

/// Each settings group's icon and one-line summary in the dialog's list.
const GROUPS: &[(&str, &str, &str)] = &[
    ("Writing", "nxo-file-text-symbolic", "Spelling"),
    ("Presenting", "nxo-presentation-symbolic", "Presenter view"),
    ("Files", "nxo-files-symbolic", "Paper, unsaved work"),
    ("Write window", "nxo-palette-symbolic", "Theme, accent, motion"),
    ("Sheets window", "nxo-palette-symbolic", "Theme, accent, motion"),
    ("Present window", "nxo-palette-symbolic", "Theme, accent, motion"),
    ("Keyboard", "input-keyboard-symbolic", "Shortcuts"),
];

const SPEC: Spec = Spec { groups: GROUPS, search_text: None };

pub fn open() {
    let (Some(overlay), Some(window)) = (window::overlay(), window::window()) else { return };
    settings_card::open(&overlay, &window, SPEC, || {
        let page = widgets::page("settings");
        crate::settings_page::build(&page);
        (page.root.upcast(), page.body)
    });
}
