//! Preferences shared by Write, Sheets and Present (`~/.config/nexus-office/settings.toml`).

use crate::{cmd, paths};
use gtk::glib;
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeMode {
    /// Follow the active Omarchy theme live.
    Omarchy,
    /// Use a bundled or custom theme.
    Theme,
}

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

thread_local! {
    static BROKEN: Cell<bool> = const { Cell::new(false) };
    static PREFS: RefCell<Prefs> = RefCell::new(load());
    static PENDING: Cell<Option<glib::SourceId>> = const { Cell::new(None) };
}

fn load() -> Prefs {
    let file = paths::prefs_file();
    let Ok(text) = std::fs::read_to_string(&file) else { return Prefs::default() };
    match toml::from_str(&text) {
        Ok(p) => p,
        Err(e) => {
            // Don't lose a file with a typo in it: keep a copy before the
            // defaults are saved over it.
            let backup = file.with_extension("toml.bak");
            let _ = std::fs::copy(&file, &backup);
            eprintln!("nexus-office: {} couldn't be read ({e}); kept a copy as {}", file.display(), backup.display());
            BROKEN.with(|b| b.set(true));
            Prefs::default()
        }
    }
}

/// True (once) when the settings file couldn't be read at start.
pub fn take_broken() -> bool {
    BROKEN.with(|b| b.replace(false))
}

pub fn get() -> Prefs {
    PREFS.with(|p| p.borrow().clone())
}

/// Change the prefs now; the file is written about 450 ms after the last change,
/// so dragging a slider doesn't rewrite it on every tick.
pub fn update(change: impl FnOnce(&mut Prefs)) {
    PREFS.with(|p| change(&mut p.borrow_mut()));
    if let Some(id) = PENDING.with(|p| p.take()) {
        id.remove();
    }
    let id = glib::timeout_add_local_once(std::time::Duration::from_millis(450), || {
        PENDING.with(|p| p.set(None));
        save();
    });
    PENDING.with(|p| p.set(Some(id)));
}

/// Write any pending change now (on quit).
pub fn flush() {
    if let Some(id) = PENDING.with(|p| p.take()) {
        id.remove();
        save();
    }
}

fn save() {
    let text = PREFS.with(|p| toml::to_string_pretty(&*p.borrow()));
    if let Ok(text) = text {
        let _ = cmd::atomic_write(&paths::prefs_file(), text);
    }
}
