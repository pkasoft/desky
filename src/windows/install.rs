//! Native Windows Installation, Desktop Shortcut & Start Menu Integration
//!
//! Provides zero-friction self-installation for portable Desky:
//! - Installs into `%LOCALAPPDATA%\Programs\Desky` (User Mode) or `%ProgramFiles%\Desky` (Admin)
//! - Generates native `.lnk` shortcuts on Desktop and Start Menu Programs
//! - Registers with Windows "Add/Remove Programs" for clean uninstallation
//! - Zero external dependencies, pure native Win32 COM & Registry APIs

#![allow(unsafe_op_in_unsafe_fn)]
use std::path::{Path, PathBuf};
use tracing::info;
use windows::core::{Interface, PCWSTR};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    IPersistFile,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteKeyW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn create_shortcut(target_exe: &Path, shortcut_path: &Path, description: &str) -> anyhow::Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;

        let wide_target = to_wide(&target_exe.to_string_lossy());
        link.SetPath(PCWSTR(wide_target.as_ptr()))?;

        if let Some(parent) = target_exe.parent() {
            let wide_workdir = to_wide(&parent.to_string_lossy());
            link.SetWorkingDirectory(PCWSTR(wide_workdir.as_ptr()))?;
        }

        let wide_desc = to_wide(description);
        link.SetDescription(PCWSTR(wide_desc.as_ptr()))?;
        link.SetIconLocation(PCWSTR(wide_target.as_ptr()), 0)?;

        let persist: IPersistFile = link.cast()?;
        let wide_lnk = to_wide(&shortcut_path.to_string_lossy());
        persist.Save(PCWSTR(wide_lnk.as_ptr()), true)?;

        CoUninitialize();
    }
    Ok(())
}

fn set_reg_sz(hkey: HKEY, name: &str, value: &str) {
    let wname = to_wide(name);
    let wval = to_wide(value);
    let slice = unsafe {
        std::slice::from_raw_parts(wval.as_ptr() as *const u8, wval.len() * 2)
    };
    unsafe {
        let _ = RegSetValueExW(hkey, PCWSTR(wname.as_ptr()), None, REG_SZ, Some(slice));
    }
}

fn set_reg_dword(hkey: HKEY, name: &str, value: u32) {
    let wname = to_wide(name);
    let bytes = value.to_ne_bytes();
    unsafe {
        let _ = RegSetValueExW(hkey, PCWSTR(wname.as_ptr()), None, REG_DWORD, Some(&bytes));
    }
}

pub fn register_uninstall(target_exe: &Path, is_admin: bool) -> anyhow::Result<()> {
    let root = if is_admin { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER };
    let subkey = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Desky");
    let mut hkey = HKEY::default();

    unsafe {
        RegCreateKeyExW(
            root,
            PCWSTR(subkey.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        ).ok()?;

        let parent_dir = target_exe.parent().map(|p| p.to_string_lossy()).unwrap_or_default();
        let uninstall_cmd = format!("\"{}\" --uninstall", target_exe.to_string_lossy());

        set_reg_sz(hkey, "DisplayName", "Desky Remote Desktop");
        set_reg_sz(hkey, "DisplayVersion", env!("CARGO_PKG_VERSION"));
        set_reg_sz(hkey, "Publisher", "pkasoft");
        set_reg_sz(hkey, "InstallLocation", &parent_dir);
        set_reg_sz(hkey, "DisplayIcon", &target_exe.to_string_lossy());
        set_reg_sz(hkey, "UninstallString", &uninstall_cmd);
        set_reg_sz(hkey, "HelpLink", "https://pkasoft.com");
        set_reg_sz(hkey, "URLInfoAbout", "https://pkasoft.com");
        set_reg_dword(hkey, "NoModify", 1);
        set_reg_dword(hkey, "NoRepair", 1);

        if let Ok(meta) = std::fs::metadata(target_exe) {
            let size_kb = (meta.len() / 1024) as u32;
            set_reg_dword(hkey, "EstimatedSize", size_kb);
        }

        let _ = RegCloseKey(hkey);
    }

    Ok(())
}

pub fn unregister_uninstall(is_admin: bool) {
    let root = if is_admin { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER };
    let subkey = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Desky");
    unsafe {
        let _ = RegDeleteKeyW(root, PCWSTR(subkey.as_ptr()));
    }
}

pub fn get_install_paths() -> (PathBuf, PathBuf, PathBuf, bool) {
    let is_admin = crate::windows::desktop::is_current_process_elevated();

    let target_dir = if is_admin {
        let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".to_string());
        PathBuf::from(pf).join("Desky")
    } else {
        let local_appdata = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
            let userprofile = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\Default".to_string());
            format!("{}\\AppData\\Local", userprofile)
        });
        PathBuf::from(local_appdata).join("Programs").join("Desky")
    };

    let desktop_dir = if is_admin {
        let pub_desktop = PathBuf::from("C:\\Users\\Public\\Desktop");
        if pub_desktop.exists() {
            pub_desktop
        } else {
            crate::windows::system::get_desktop_dir()
        }
    } else {
        crate::windows::system::get_desktop_dir()
    };

    let start_menu_dir = if is_admin {
        let pd = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".to_string());
        PathBuf::from(pd).join("Microsoft").join("Windows").join("Start Menu").join("Programs")
    } else {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| {
            let userprofile = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\Default".to_string());
            format!("{}\\AppData\\Roaming", userprofile)
        });
        PathBuf::from(appdata).join("Microsoft").join("Windows").join("Start Menu").join("Programs")
    };

    (target_dir, desktop_dir, start_menu_dir, is_admin)
}

pub fn is_installed() -> bool {
    let cur = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return false,
    };

    let cur_str = cur.to_string_lossy().to_lowercase();

    if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
        let expected = format!("{}\\programs\\desky", local_appdata.to_lowercase());
        if cur_str.starts_with(&expected) {
            return true;
        }
    }

    if let Ok(prog_files) = std::env::var("ProgramFiles") {
        let expected = format!("{}\\desky", prog_files.to_lowercase());
        if cur_str.starts_with(&expected) {
            return true;
        }
    }

    false
}

pub fn install_to_system() -> anyhow::Result<PathBuf> {
    let (target_dir, desktop_dir, start_menu_dir, is_admin) = get_install_paths();

    std::fs::create_dir_all(&target_dir)?;
    let target_exe = target_dir.join("desky.exe");

    let current_exe = std::env::current_exe()?;
    if current_exe.to_string_lossy().to_lowercase() != target_exe.to_string_lossy().to_lowercase() {
        std::fs::copy(&current_exe, &target_exe)?;
        info!("Copied Desky executable to: {:?}", target_exe);
    }

    // 1. Create Desktop Shortcut
    if !desktop_dir.exists() {
        let _ = std::fs::create_dir_all(&desktop_dir);
    }
    let desktop_lnk = desktop_dir.join("Desky.lnk");
    create_shortcut(&target_exe, &desktop_lnk, "Desky Remote Desktop")?;
    info!("Created Desktop Shortcut: {:?}", desktop_lnk);

    // 2. Create Start Menu Shortcut
    if !start_menu_dir.exists() {
        let _ = std::fs::create_dir_all(&start_menu_dir);
    }
    let start_lnk = start_menu_dir.join("Desky.lnk");
    create_shortcut(&target_exe, &start_lnk, "Desky Remote Desktop")?;
    info!("Created Start Menu Shortcut: {:?}", start_lnk);

    // 3. Register in Windows Add/Remove Programs
    let _ = register_uninstall(&target_exe, is_admin);

    Ok(target_exe)
}

pub fn uninstall_from_system() {
    let (_target_dir, desktop_dir, start_menu_dir, _is_admin) = get_install_paths();

    // 1. Remove Desktop shortcuts
    let _ = std::fs::remove_file(desktop_dir.join("Desky.lnk"));
    let _ = std::fs::remove_file("C:\\Users\\Public\\Desktop\\Desky.lnk");
    let _ = std::fs::remove_file(crate::windows::system::get_desktop_dir().join("Desky.lnk"));

    // 2. Remove Start Menu shortcuts
    let _ = std::fs::remove_file(start_menu_dir.join("Desky.lnk"));
    if let Ok(appdata) = std::env::var("APPDATA") {
        let user_start = PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Desky.lnk");
        let _ = std::fs::remove_file(user_start);
    }

    // 3. Unregister from Windows Add/Remove Programs
    unregister_uninstall(false);
    unregister_uninstall(true);

    info!("Desky shortcuts and registry registration uninstalled.");

    // Native Windows MessageBox
    unsafe {
        let msg = to_wide("Desky shortcuts and registry registration have been removed from your system.\n\nYou may now delete the Desky executable.");
        let title = to_wide("Desky Uninstallation");
        let _ = MessageBoxW(None, PCWSTR(msg.as_ptr()), PCWSTR(title.as_ptr()), MB_OK | MB_ICONINFORMATION);
    }
}
