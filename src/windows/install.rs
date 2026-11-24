//! Native Windows Installation, Desktop Shortcut & Start Menu Integration
//!
//! Provides zero-friction self-installation for portable Resky:
//! - Installs into `%LOCALAPPDATA%\Programs\Resky` (User Mode) or `%ProgramFiles%\Resky` (Admin)
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
    RegCloseKey, RegCreateKeyExW, RegDeleteKeyW, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE,
    REG_SZ,
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
    let subkey = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Resky");
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

        set_reg_sz(hkey, "DisplayName", "Resky Remote Desktop");
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
    let subkey = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Resky");
    unsafe {
        let _ = RegDeleteKeyW(root, PCWSTR(subkey.as_ptr()));
    }
}

pub fn get_install_paths() -> (PathBuf, PathBuf, PathBuf, bool) {
    let is_admin = crate::windows::desktop::is_current_process_elevated();

    let target_dir = if is_admin {
        let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".to_string());
        PathBuf::from(pf).join("Resky")
    } else {
        let local_appdata = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
            let userprofile = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\Default".to_string());
            format!("{}\\AppData\\Local", userprofile)
        });
        PathBuf::from(local_appdata).join("Programs").join("Resky")
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

pub fn get_installed_exe_path() -> Option<PathBuf> {
    if let Ok(prog_files) = std::env::var("ProgramFiles") {
        let p = PathBuf::from(prog_files).join("Resky").join("resky.exe");
        if p.exists() {
            return Some(p);
        }
    }

    if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
        let p = PathBuf::from(local_appdata)
            .join("Programs")
            .join("Resky")
            .join("resky.exe");
        if p.exists() {
            return Some(p);
        }
    }

    None
}

pub fn is_installed() -> bool {
    if let Ok(cur) = std::env::current_exe() {
        let cur_str = cur.to_string_lossy().to_lowercase();

        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
            let expected = format!("{}\\programs\\resky", local_appdata.to_lowercase());
            if cur_str.starts_with(&expected) {
                return true;
            }
        }

        if let Ok(prog_files) = std::env::var("ProgramFiles") {
            let expected = format!("{}\\resky", prog_files.to_lowercase());
            if cur_str.starts_with(&expected) {
                return true;
            }
        }
    }

    get_installed_exe_path().is_some()
}

pub fn install_to_system() -> anyhow::Result<PathBuf> {
    let (target_dir, desktop_dir, start_menu_dir, is_admin) = get_install_paths();

    std::fs::create_dir_all(&target_dir)?;
    let target_exe = target_dir.join("resky.exe");

    let current_exe = std::env::current_exe()?;
    if current_exe.to_string_lossy().to_lowercase() != target_exe.to_string_lossy().to_lowercase() {
        std::fs::copy(&current_exe, &target_exe)?;
        info!("Copied Resky executable to: {:?}", target_exe);
    }

    // 1. Create Desktop Shortcut
    if !desktop_dir.exists() {
        let _ = std::fs::create_dir_all(&desktop_dir);
    }
    let desktop_lnk = desktop_dir.join("Resky.lnk");
    create_shortcut(&target_exe, &desktop_lnk, "Resky Remote Desktop")?;
    info!("Created Desktop Shortcut: {:?}", desktop_lnk);

    // 2. Create Start Menu Shortcut
    if !start_menu_dir.exists() {
        let _ = std::fs::create_dir_all(&start_menu_dir);
    }
    let start_lnk = start_menu_dir.join("Resky.lnk");
    create_shortcut(&target_exe, &start_lnk, "Resky Remote Desktop")?;
    info!("Created Start Menu Shortcut: {:?}", start_lnk);

    // 3. Register in Windows Add/Remove Programs
    let _ = register_uninstall(&target_exe, is_admin);

    // 4. If autostart is enabled, point the Scheduled Task to the installed executable
    if is_autostart_enabled() {
        let _ = set_autostart_enabled(true);
    }

    Ok(target_exe)
}

pub fn uninstall_from_system() {
    let (_target_dir, desktop_dir, start_menu_dir, _is_admin) = get_install_paths();

    // 1. Remove Desktop shortcuts
    let _ = std::fs::remove_file(desktop_dir.join("Resky.lnk"));
    let _ = std::fs::remove_file("C:\\Users\\Public\\Desktop\\Resky.lnk");
    let _ = std::fs::remove_file(crate::windows::system::get_desktop_dir().join("Resky.lnk"));

    // 2. Remove Start Menu shortcuts
    let _ = std::fs::remove_file(start_menu_dir.join("Resky.lnk"));
    if let Ok(appdata) = std::env::var("APPDATA") {
        let user_start = PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Resky.lnk");
        let _ = std::fs::remove_file(user_start);
    }

    // 3. Unregister from Windows Add/Remove Programs & Autostart
    unregister_uninstall(false);
    unregister_uninstall(true);
    let _ = set_autostart_enabled(false);

    info!("Resky shortcuts and registry registration uninstalled.");

    // Native Windows MessageBox
    unsafe {
        let msg = to_wide("Resky shortcuts and registry registration have been removed from your system.\n\nYou may now delete the Resky executable.");
        let title = to_wide("Resky Uninstallation");
        let _ = MessageBoxW(None, PCWSTR(msg.as_ptr()), PCWSTR(title.as_ptr()), MB_OK | MB_ICONINFORMATION);
    }
}

const AUTOSTART_TASK_NAME: &str = "Resky";
const CREATE_NO_WINDOW: u32 = 0x08000000;

fn remove_legacy_run_autostart() {
    let subkey = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let val_name = to_wide("Resky");
    let mut hkey = HKEY::default();
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr()), None, KEY_WRITE, &mut hkey).is_ok() {
            let _ = RegDeleteValueW(hkey, PCWSTR(val_name.as_ptr()));
            let _ = RegCloseKey(hkey);
        }
    }
}

pub fn is_autostart_enabled() -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("schtasks")
        .args(["/Query", "/TN", AUTOSTART_TASK_NAME])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn set_autostart_enabled(enabled: bool) -> anyhow::Result<()> {
    use std::os::windows::process::CommandExt;

    // Remove any legacy HKCU\...\Run entry (Windows blocks elevated apps in Run)
    remove_legacy_run_autostart();

    if enabled {
        let exe = match get_installed_exe_path() {
            Some(p) => p,
            None => std::env::current_exe()?,
        };
        let cmd = format!("\"{}\" --tray", exe.to_string_lossy());
        let output = std::process::Command::new("schtasks")
            .args([
                "/Create",
                "/TN",
                AUTOSTART_TASK_NAME,
                "/TR",
                &cmd,
                "/SC",
                "ONLOGON",
                "/RL",
                "HIGHEST",
                "/F",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create elevated startup task: {}", err.trim());
        }
        info!("Enabled Windows Elevated Startup Task (ONLOGON / HIGHEST): {}", cmd);
    } else {
        let _ = std::process::Command::new("schtasks")
            .args(["/Delete", "/TN", AUTOSTART_TASK_NAME, "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        info!("Disabled Windows Elevated Startup Task");
    }
    Ok(())
}
