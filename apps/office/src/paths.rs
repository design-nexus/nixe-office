//! Well-known locations. Every path honours the XDG overrides so the whole app
//! can be pointed at a scratch copy of `~/.config` for testing.

use std::path::PathBuf;

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(fallback))
}

pub fn config_home() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config")
}

pub fn state_home() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state")
}

/// `~/.cache/nexus-office`: rendered icons and thumbnails.
pub fn cache_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("nexus-office")
}

/// `~/.local/share/nexus-office`: the recent-files lists.
pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("nexus-office")
}

/// `~/.config/nexus-office`: everything the user can edit lives here.
pub fn app_dir() -> PathBuf {
    config_home().join("nexus-office")
}

pub fn prefs_file() -> PathBuf {
    app_dir().join("settings.toml")
}

pub fn custom_themes_dir() -> PathBuf {
    app_dir().join("themes")
}

/// One recent-files list per app (`write`, `sheets`, `present`).
pub fn recent_file(app: &str) -> PathBuf {
    data_dir().join(format!("recent-{app}.json"))
}

/// Snapshots of unsaved work, one folder per app.
pub fn recovery_dir(app: &str) -> PathBuf {
    state_home().join("nexus-office/recovery").join(app)
}

pub fn documents_dir() -> PathBuf {
    let dirs = config_home().join("user-dirs.dirs");
    if let Ok(text) = std::fs::read_to_string(dirs) {
        for line in text.lines() {
            if let Some(v) = line.trim().strip_prefix("XDG_DOCUMENTS_DIR=") {
                let v = v.trim_matches('"').replace("$HOME", &home().to_string_lossy());
                if !v.is_empty() {
                    return PathBuf::from(v);
                }
            }
        }
    }
    home().join("Documents")
}

pub fn omarchy_theme_dir() -> PathBuf {
    state_home().join("omarchy/current/theme")
}

pub fn omarchy_colors() -> PathBuf {
    omarchy_theme_dir().join("colors.toml")
}

/// Replace `$HOME` with `~` for display.
pub fn pretty(path: &std::path::Path) -> String {
    let home = home();
    match path.strip_prefix(&home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}
