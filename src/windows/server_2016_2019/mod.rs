//! Windows Server 2016 & 2019 Platform Support

pub use crate::windows::server_2022::capture;
pub use crate::windows::server_2022::input;

use tracing::info;
use windows::core::s;
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

type SetProcessDpiAwarenessContextFn = unsafe extern "system" fn(isize) -> i32;

/// Initializes Windows Server 2016 / 2019 DPI awareness
pub fn init() {
    crate::windows::audio::ensure_windows_audio_services();
    unsafe {
        if let Ok(user32) = GetModuleHandleA(s!("user32.dll")) {
            if let Some(proc) = GetProcAddress(user32, s!("SetProcessDpiAwarenessContext")) {
                let func: SetProcessDpiAwarenessContextFn = std::mem::transmute(proc);
                if func(-4) != 0 {
                    info!("✅ [SERVER 2016/2019] Per-Monitor V2 DPI awareness active.");
                    return;
                }
            }
        }
    }
}
