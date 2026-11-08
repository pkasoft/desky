//! Windows 7, Windows 8 & Windows 8.1 Platform Support

pub use crate::windows::win10_11::capture;
pub use crate::windows::win10_11::input;

use tracing::info;
use windows::core::s;
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress, LoadLibraryA};

type SetProcessDpiAwarenessFn = unsafe extern "system" fn(i32) -> i32;
type SetProcessDPIAwareFn = unsafe extern "system" fn() -> i32;

/// Initializes Windows 7 / 8 / 8.1 DPI awareness
pub fn init() {
    unsafe {
        // 1. Windows 8.1: SetProcessDpiAwareness
        if let Ok(shcore) = LoadLibraryA(s!("shcore.dll")) {
            if let Some(proc) = GetProcAddress(shcore, s!("SetProcessDpiAwareness")) {
                let func: SetProcessDpiAwarenessFn = std::mem::transmute(proc);
                if func(2) == 0 {
                    info!("✅ [WIN 8.1] Per-Monitor DPI awareness active.");
                    return;
                }
            }
        }

        // 2. Windows 7 / 8: SetProcessDPIAware
        if let Ok(user32) = GetModuleHandleA(s!("user32.dll")) {
            if let Some(proc) = GetProcAddress(user32, s!("SetProcessDPIAware")) {
                let func: SetProcessDPIAwareFn = std::mem::transmute(proc);
                if func() != 0 {
                    info!("✅ [WIN 7/8] System DPI awareness active.");
                    return;
                }
            }
        }
    }
}
