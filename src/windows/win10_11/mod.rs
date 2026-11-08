//! Windows 10 & Windows 11 Platform Support

pub mod capture;
pub mod input;

use tracing::info;
use windows::core::s;
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

type SetProcessDpiAwarenessContextFn = unsafe extern "system" fn(isize) -> i32;

/// Initializes Windows 10 & Windows 11 Per-Monitor V2 DPI Awareness
pub fn init() {
    unsafe {
        if let Ok(user32) = GetModuleHandleA(s!("user32.dll")) {
            if let Some(proc) = GetProcAddress(user32, s!("SetProcessDpiAwarenessContext")) {
                let func: SetProcessDpiAwarenessContextFn = std::mem::transmute(proc);
                if func(-4) != 0 {
                    info!("✅ [WIN 10/11] Per-Monitor V2 DPI awareness active.");
                    return;
                }
            }
        }
    }
}
