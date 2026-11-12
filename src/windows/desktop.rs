//! Windows Desktop Station & Lock Screen Synchronization
//! Enables capturing and controlling the Windows Lock Screen (Winlogon), UAC Secure Desktop,
//! and normal User Desktop (Default).

use std::cell::RefCell;
use tracing::debug;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HANDLE, LUID};
use windows::Win32::Security::{
    AdjustTokenPrivileges, GetTokenInformation, LookupPrivilegeValueW, TokenElevation,
    LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_ELEVATION,
    TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

#[link(name = "user32")]
unsafe extern "system" {
    fn OpenInputDesktop(dwFlags: u32, fInherit: i32, dwDesiredAccess: u32) -> isize;
    fn SetThreadDesktop(hDesktop: isize) -> i32;
    fn CloseDesktop(hDesktop: isize) -> i32;
    fn GetUserObjectInformationW(
        hObj: isize,
        nIndex: i32,
        pvInfo: *mut u8,
        nLength: u32,
        lpnLengthNeeded: *mut u32,
    ) -> i32;
}

const MAXIMUM_ALLOWED: u32 = 0x02000000;
const DESKTOP_ALL_ACCESS: u32 = 0x01FF;
const DESKTOP_READWRITE: u32 =
    0x0001 | 0x0002 | 0x0004 | 0x0008 | 0x0010 | 0x0020 | 0x0040 | 0x0080 | 0x0100;

thread_local! {
    static CURRENT_DESKTOP: RefCell<(String, isize)> = RefCell::new((String::new(), 0));
}

/// Synchronizes the calling thread's desktop to whatever desktop is currently receiving user input
/// (e.g. "Default", "Winlogon" for lock screen, or "Screen-saver").
pub fn sync_thread_desktop_to_input() -> bool {
    let active_name = get_active_input_desktop_name();

    CURRENT_DESKTOP.with(|cell| {
        let mut cur = cell.borrow_mut();
        if !cur.0.is_empty() && cur.0.eq_ignore_ascii_case(&active_name) {
            return true;
        }

        unsafe {
            let mut h_desk = OpenInputDesktop(0, 0, MAXIMUM_ALLOWED);
            if h_desk == 0 {
                h_desk = OpenInputDesktop(0, 0, DESKTOP_ALL_ACCESS);
            }
            if h_desk == 0 {
                h_desk = OpenInputDesktop(0, 0, DESKTOP_READWRITE);
            }

            if h_desk == 0 {
                return false;
            }

            if SetThreadDesktop(h_desk) != 0 {
                let old_h = cur.1;
                *cur = (active_name, h_desk);
                if old_h != 0 && old_h != h_desk {
                    let _ = CloseDesktop(old_h);
                }
                true
            } else {
                let _ = CloseDesktop(h_desk);
                false
            }
        }
    })
}

/// Returns the name of the desktop currently receiving user input ("Default", "Winlogon", etc.)
pub fn get_active_input_desktop_name() -> String {
    unsafe {
        let mut h_desk = OpenInputDesktop(0, 0, 0x0001); // DESKTOP_READOBJECTS
        if h_desk == 0 {
            h_desk = OpenInputDesktop(0, 0, MAXIMUM_ALLOWED);
        }
        if h_desk == 0 {
            return "Default".to_string();
        }
        let mut buf = [0u16; 64];
        let mut needed = 0u32;
        let ok = GetUserObjectInformationW(
            h_desk,
            2, // UOI_NAME
            buf.as_mut_ptr() as *mut u8,
            (buf.len() * 2) as u32,
            &mut needed,
        );
        let _ = CloseDesktop(h_desk);
        if ok != 0 {
            let len = (needed as usize / 2).saturating_sub(1);
            String::from_utf16_lossy(&buf[..len.min(buf.len())])
        } else {
            "Default".to_string()
        }
    }
}

/// Attempts to enable SeDebugPrivilege in the process token for accessing secure system desktops
pub fn enable_debug_privilege() {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token).is_ok() {
            let mut luid = LUID::default();
            if LookupPrivilegeValueW(None, w!("SeDebugPrivilege"), &mut luid).is_ok() {
                let tp = TOKEN_PRIVILEGES {
                    PrivilegeCount: 1,
                    Privileges: [LUID_AND_ATTRIBUTES {
                        Luid: luid,
                        Attributes: SE_PRIVILEGE_ENABLED,
                    }],
                };
                let _ = AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None);
                debug!("SeDebugPrivilege enabled for Windows Lock Screen and UAC access");
            }
        }
    }
}

/// Checks if current process is running with Administrator (elevated) privileges
pub fn is_current_process_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_ok() {
            let mut elevation = TOKEN_ELEVATION::default();
            let mut ret_len = 0u32;
            if GetTokenInformation(
                token,
                TokenElevation,
                Some(&mut elevation as *mut _ as *mut _),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut ret_len,
            )
            .is_ok()
            {
                return elevation.TokenIsElevated != 0;
            }
        }
    }
    false
}

/// Automatically configures Windows Group Policy registry to permit software SAS generation (Ctrl+Alt+Del)
pub fn configure_software_sas() {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let _ = Command::new("reg")
        .args([
            "add",
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System",
            "/v",
            "SoftwareSASGeneration",
            "/t",
            "REG_DWORD",
            "/d",
            "3",
            "/f",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

/// Triggers UAC elevation prompt on the local Windows host to re-launch Desky as Administrator
pub fn elevate_process() -> bool {
    unsafe {
        if let Ok(exe_path) = std::env::current_exe() {
            let wide_exe: Vec<u16> = exe_path
                .to_string_lossy()
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let verb = w!("runas");
            let h_inst = ShellExecuteW(
                None,
                verb,
                PCWSTR(wide_exe.as_ptr()),
                None,
                None,
                SW_SHOWNORMAL,
            );
            return (h_inst.0 as usize) > 32;
        }
        false
    }
}
