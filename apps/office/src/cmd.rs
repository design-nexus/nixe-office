//! Running external commands without blocking the UI thread.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

pub fn present(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).exists();
    }
    std::env::var_os("PATH").map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file())).unwrap_or(false)
}

/// Fire and forget. The child is reaped on a helper thread so no zombies remain.
pub fn spawn(args: &[&str]) {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    std::thread::spawn(move || {
        if let Some((program, rest)) = args.split_first() {
            let _ = Command::new(program).args(rest).stdin(Stdio::null()).status();
        }
    });
}

/// Open a file in the user's default editor (Omarchy's choice), falling back to xdg-open.
pub fn open_in_editor(path: &Path) {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, "");
    }
    let p = path.to_string_lossy().to_string();
    if present("omarchy-launch-editor") {
        spawn(&["omarchy-launch-editor", &p]);
    } else {
        spawn(&["xdg-open", &p]);
    }
}

/// Write a file atomically: temp file in the same directory, then rename.
pub fn atomic_write(path: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
    let dir = path.parent().context("path has no parent")?;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.tmp-{}", path.file_name().unwrap_or_default().to_string_lossy(), std::process::id()));
    std::fs::write(&tmp, contents.as_ref())?;
    std::fs::rename(&tmp, path).with_context(|| format!("could not write {}", path.display()))?;
    Ok(())
}
