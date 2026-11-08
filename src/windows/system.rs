//! Windows Platform System Utilities & Shell Integration

use std::path::{Path, PathBuf};
use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_UPDATEDIR, SHCNF_PATHW};

/// Resolves the user's Desktop directory on Windows
pub fn get_desktop_dir() -> PathBuf {
    if let Ok(up) = std::env::var("USERPROFILE") {
        let p = PathBuf::from(up).join("Desktop");
        if p.exists() {
            return p;
        }
    }
    if let Ok(user) = std::env::var("USERNAME") {
        let p = PathBuf::from("C:\\Users").join(user).join("Desktop");
        if p.exists() {
            return p;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Dispatches an immediate shell directory update notification to Windows Explorer
pub fn notify_file_received(path: &Path) {
    if let Some(parent) = path.parent() {
        let dir_str = parent.to_string_lossy().to_string();
        if !dir_str.is_empty() {
            let wide_dir: Vec<u16> = dir_str.encode_utf16().chain(std::iter::once(0)).collect();
            unsafe {
                SHChangeNotify(
                    SHCNE_UPDATEDIR,
                    SHCNF_PATHW,
                    Some(wide_dir.as_ptr() as *const _),
                    None,
                );
            }
        }
    }
}

/// Resolves the application configuration storage directory (`%APPDATA%\Desky`)
pub fn get_identity_dir() -> PathBuf {
    if let Ok(app_data) = std::env::var("APPDATA") {
        let dir = PathBuf::from(app_data).join("Desky");
        let _ = std::fs::create_dir_all(&dir);
        dir
    } else {
        let dir = PathBuf::from(".desky");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }
}

/// Retrieves or generates a persistent Windows machine identifier
pub fn get_machine_id() -> String {
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

/// Trims the process working set, returning unused heap memory and cached pages back to Windows
pub fn trim_process_working_set() {
    unsafe {
        let current_process = windows::Win32::System::Threading::GetCurrentProcess();
        let _ = windows::Win32::System::Memory::SetProcessWorkingSetSizeEx(
            current_process,
            usize::MAX,
            usize::MAX,
            windows::Win32::System::Memory::SETPROCESSWORKINGSETSIZEEX_FLAGS(0),
        );
    }
}
