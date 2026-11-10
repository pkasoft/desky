//! Windows Desktop Station & Lock Screen Synchronization
//! Enables capturing and controlling the Windows Lock Screen (Winlogon), UAC Secure Desktop,
//! and normal User Desktop (Default).

use std::cell::RefCell;
use tracing::debug;
use windows::core::w;
use windows::Win32::Foundation::{HANDLE, LUID};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

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
