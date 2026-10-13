//! Recently opened files, one list per app (`~/.local/share/nixe-office/recent-<app>.json`).

use crate::{app, cmd, paths};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recent {
    pub path: PathBuf,
    /// Seconds since 1970 when it was last opened or saved.
    pub opened: u64,
}

const LIMIT: usize = 60;

fn file() -> PathBuf {
    paths::recent_file(app::kind().id())
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Newest first, without files that have since been deleted.
pub fn list() -> Vec<Recent> {
    let items: Vec<Recent> = std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    items.into_iter().filter(|r| r.path.exists()).collect()
}

fn store(items: &[Recent]) {
    if let Ok(text) = serde_json::to_string_pretty(items) {
        let _ = cmd::atomic_write(&file(), text);
    }
}

/// Note that a file was opened or saved.
pub fn record(path: &Path) {
    let mut items = list();
    items.retain(|r| r.path != path);
    items.insert(0, Recent { path: path.to_path_buf(), opened: now() });
    items.truncate(LIMIT);
    store(&items);
}

pub fn forget(path: &Path) {
    let mut items = list();
    items.retain(|r| r.path != path);
    store(&items);
}

/// "just now", "5 min ago", "3 hours ago", "yesterday", "12 days ago".
pub fn ago(then: u64) -> String {
    let d = now().saturating_sub(then);
    match d {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", d / 60),
        3600..7200 => "1 hour ago".into(),
        7200..86400 => format!("{} hours ago", d / 3600),
        86400..172800 => "yesterday".into(),
        _ => format!("{} days ago", d / 86400),
    }
}
