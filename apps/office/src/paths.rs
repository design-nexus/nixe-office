//! Well-known locations. Every path honours the XDG overrides so the whole app
//! can be pointed at a scratch copy of `~/.config` for testing.

pub use nixe_core::paths::*;

use std::path::PathBuf;

/// One recent-files list per app (`write`, `sheets`, `present`).
pub fn recent_file(app: &str) -> PathBuf {
    data_dir().join(format!("recent-{app}.json"))
}

/// Snapshots of unsaved work, one folder per app.
pub fn recovery_dir(app: &str) -> PathBuf {
    state_home().join("nexus-office/recovery").join(app)
}

/// Each app's zoom, a number on its own (`write`, `sheets`, `present`).
pub fn zoom_file(app: &str) -> PathBuf {
    state_home().join("nexus-office").join(format!("zoom-{app}"))
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

