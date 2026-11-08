//! Windows Server 2022 & 2025 Platform Support

pub mod capture;
pub mod input;

use tracing::info;
use windows::core::s;
use windows::Win32::Graphics::Gdi::{
    GetDC, GetDeviceCaps, ReleaseDC, DESKTOPHORZRES, DESKTOPVERTRES,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

type SetProcessDpiAwarenessContextFn = unsafe extern "system" fn(isize) -> i32;

pub fn init() {
    unsafe {
        if let Ok(user32) = GetModuleHandleA(s!("user32.dll")) {
            if let Some(proc) = GetProcAddress(user32, s!("SetProcessDpiAwarenessContext")) {
                let func: SetProcessDpiAwarenessContextFn = std::mem::transmute(proc);
                if func(-4) != 0 {
                    info!("✅ [SERVER 2022] Per-Monitor V2 DPI awareness active.");
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

        let width = virt_w.max(primary_w).max(phys_w).max(1);
        let height = virt_h.max(primary_h).max(phys_h).max(1);
        let x = virt_x;
        let y = virt_y;

        DesktopGeometry { x, y, width, height }
    }
}
