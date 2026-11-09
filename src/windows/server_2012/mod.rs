//! Windows Server 2012 & 2012 R2 Platform Support
//!
//! Provides dedicated capture, input, legacy DPI initialization, and ensures
//! that Windows Audio services (`Audiosrv` / `AudioEndpointBuilder`) are automatically
//! configured and running so desktop sound from video playback streams properly.

pub mod capture;
pub mod input;

use tracing::info;
use windows::core::s;
use windows::Win32::Graphics::Gdi::{
    GetDC, GetDeviceCaps, ReleaseDC, DESKTOPHORZRES, DESKTOPVERTRES,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress, LoadLibraryA};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

type SetProcessDpiAwarenessFn = unsafe extern "system" fn(i32) -> i32;
type SetProcessDPIAwareFn = unsafe extern "system" fn() -> i32;

/// Initializes Windows Server 2012 / 2012 R2 platform runtime
pub fn init() {
    // 1. Ensure Windows Audio service (`Audiosrv`) is enabled and running
    crate::windows::audio::ensure_windows_audio_services();

    unsafe {
        // 2. Try Windows Server 2012 R2: SetProcessDpiAwareness(2) from shcore.dll
        if let Ok(shcore) = LoadLibraryA(s!("shcore.dll")) {
            if let Some(proc) = GetProcAddress(shcore, s!("SetProcessDpiAwareness")) {
                let func: SetProcessDpiAwarenessFn = std::mem::transmute(proc);
                if func(2) == 0 {
                    info!("✅ [SERVER 2012 R2] Per-Monitor DPI awareness active.");
                    return;
                }
            }
        }

        // 3. Try Windows Server 2012: SetProcessDPIAware from user32.dll
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

#[derive(Clone, Copy, Debug)]
pub struct DesktopGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub fn get_server_desktop_geometry() -> DesktopGeometry {
    unsafe {
        let virt_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let virt_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let virt_w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let virt_h = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        let primary_w = GetSystemMetrics(SM_CXSCREEN);
        let primary_h = GetSystemMetrics(SM_CYSCREEN);

        let hdc = GetDC(None);
        let (phys_w, phys_h) = if !hdc.0.is_null() {
            let pw = GetDeviceCaps(Some(hdc), DESKTOPHORZRES);
            let ph = GetDeviceCaps(Some(hdc), DESKTOPVERTRES);
            let _ = ReleaseDC(None, hdc);
            (pw, ph)
        } else {
            (0, 0)
        };

        let width = if virt_w > 0 {
            virt_w
        } else if phys_w > 0 {
            phys_w
        } else {
            primary_w.max(1280)
        };

        let height = if virt_h > 0 {
            virt_h
        } else if phys_h > 0 {
            phys_h
        } else {
            primary_h.max(720)
        };

        DesktopGeometry {
            x: virt_x,
            y: virt_y,
            width,
            height,
        }
    }
}
