//! Running external commands without blocking the UI thread.

pub use nyx_core::cmd::*;


#[cfg(test)]
mod tests {
    #[test]
    fn atomic_write_keeps_links_and_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("nxo-aw-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let real = dir.join("real.txt");
        let link = dir.join("link.txt");
        std::fs::write(&real, "old").unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).unwrap();
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&real, &link).unwrap();
        super::atomic_write(&link, "new").unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "new");
        assert_eq!(std::fs::metadata(&real).unwrap().permissions().mode() & 0o777, 0o600);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
