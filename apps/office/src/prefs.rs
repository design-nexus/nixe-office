//! Preferences shared by Write, Sheets and Present (`~/.config/nixe-office/settings.toml`).

pub use nixe_theme::ThemeMode;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub mode: ThemeMode,
    pub theme: String,
    pub reduce_motion: bool,
    pub glow: bool,
    /// The sidebar shows only icons, whatever the window width.
    pub sidebar_collapsed: bool,
    /// Paper for new documents, PDF export and printing: letter or a4.
    pub paper: String,
    /// Write: underline misspelled words while typing.
    pub spellcheck: bool,
    /// Write: the spelling language ("" = the system's).
    pub spelling_language: String,
    /// Write: curly quotes and symbols (—, …, ©) as you type.
    pub autocorrect: bool,
    /// Save a copy of unsaved work every few seconds, offered back after a crash.
    pub recovery: bool,
    /// Save named files by themselves a moment after each change, and on closing.
    pub autosave: bool,
    /// Present: open the presenter view on a second screen when one is connected.
    pub presenter_view: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Omarchy,
            theme: "tokyo-night".into(),
            reduce_motion: false,
            glow: true,
            sidebar_collapsed: false,
            paper: default_paper().into(),
            spellcheck: true,
            spelling_language: String::new(),
            autocorrect: true,
            recovery: true,
            autosave: false,
            presenter_view: true,
        }
    }
}

/// Letter in the US and Canada, A4 everywhere else.
fn default_paper() -> &'static str {
    let locale =
        ["LC_PAPER", "LC_ALL", "LANG"].iter().find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty())).unwrap_or_default();
    if locale.is_empty()
        || ["_US", "_CA", "_PH", "_MX"].iter().any(|r| locale.contains(r))
        || locale == "C"
        || locale.starts_with("C.")
    {
        "letter"
    } else {
        "a4"
    }
}

nixe_core::prefs_store!(Prefs);
