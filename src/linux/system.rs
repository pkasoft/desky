//! Linux Platform System Utilities

use std::fs;
use std::path::{Path, PathBuf};

/// Resolves the user's Desktop directory on Linux
pub fn get_desktop_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(home).join("Desktop");
        if p.exists() {
            return p;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Dispatches an immediate shell directory update notification (no-op on Linux)
pub fn notify_file_received(_path: &Path) {
    // Linux desktop environments track inotify directory updates automatically
}

/// Resolves the application configuration storage directory (`~/.config/desky`)
pub fn get_identity_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let dir = PathBuf::from(home).join(".config").join("desky");
        let _ = fs::create_dir_all(&dir);
        dir
    } else {
        let dir = PathBuf::from(".desky");
        let _ = fs::create_dir_all(&dir);
        dir
    }
}

/// Retrieves or generates a persistent Linux machine identifier
pub fn get_machine_id() -> String {
    if let Ok(mid) = fs::read_to_string("/etc/machine-id") {
        let trimmed = mid.trim();
        if !trimmed.is_empty() {
            return format!("{}-deadbeef", trimmed);
        }
    }
    format!(
        "{:x}-{:x}-{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id(),
        0xDEADBEEFu32
    )
}
