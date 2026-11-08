//! Windows Server 2012 & 2012 R2 Platform Support
//!
//! Provides backwards-compatible legacy DPI initialization and delegates
//! capture and mouse handling using the virtual desktop pipeline.

pub use crate::windows::server_2022::capture;
pub use crate::windows::server_2022::input;

use tracing::info;
use windows::core::s;
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress, LoadLibraryA};

type SetProcessDpiAwarenessFn = unsafe extern "system" fn(i32) -> i32;
type SetProcessDPIAwareFn = unsafe extern "system" fn() -> i32;

/// Initializes Windows Server 2012 / 2012 R2 DPI awareness
pub fn init() {
    unsafe {
        // 1. Try Windows Server 2012 R2: SetProcessDpiAwareness(2) from shcore.dll
        if let Ok(shcore) = LoadLibraryA(s!("shcore.dll")) {
            if let Some(proc) = GetProcAddress(shcore, s!("SetProcessDpiAwareness")) {
                let func: SetProcessDpiAwarenessFn = std::mem::transmute(proc);
                if func(2) == 0 {
                    info!("✅ [SERVER 2012 R2] Per-Monitor DPI awareness active.");
                    return;
                }
            }
        }

        // 2. Try Windows Server 2012: SetProcessDPIAware from user32.dll
        if let Ok(user32) = GetModuleHandleA(s!("user32.dll")) {
            if let Some(proc) = GetProcAddress(user32, s!("SetProcessDPIAware")) {
                let func: SetProcessDPIAwareFn = std::mem::transmute(proc);
                if func() != 0 {
                    info!("✅ [SERVER 2012] System DPI awareness active.");
                    return;
                }
            }
        }
    }
}
