//! Windows Server 2012 / 2016 / 2019 / 2022 / 2025 Platform Compatibility Engine
//!
//! Specifically resolves:
//! 1. Incomplete screen capture in Windows Server (RDP / Nexus VPS / virtual displays)
//!    by querying full virtual desktop geometry (SM_CXVIRTUALSCREEN) and true device pixels
//!    (DESKTOPHORZRES / DESKTOPVERTRES), preventing clipped desktop borders.
//! 2. Dynamic multi-tier Per-Monitor DPI awareness initialization across all Windows Server editions.
//! 3. Dual-channel mouse injection (SetCursorPos + normalized virtual mouse_event) for 100%
//!    pixel-perfect remote cursor tracking.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;
use windows::core::s;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
    GetDeviceCaps, GetDIBits, ReleaseDC, SelectObject, SetStretchBltMode, StretchBlt, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, COLORONCOLOR, DESKTOPHORZRES, DESKTOPVERTRES, DIB_RGB_COLORS,
    HGDIOBJ, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress, LoadLibraryA};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, mouse_event, KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DrawIconEx, GetCursorInfo, GetIconInfo, GetSystemMetrics, SetCursorPos, CURSORINFO,
    CURSOR_SHOWING, DI_NORMAL, HICON, ICONINFO, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};
use crate::protocol::{InputEvent, MouseButton};

static DPI_INITIALIZED: AtomicBool = AtomicBool::new(false);

type SetProcessDpiAwarenessContextFn = unsafe extern "system" fn(isize) -> i32;
type SetProcessDpiAwarenessFn = unsafe extern "system" fn(i32) -> i32;
type SetProcessDPIAwareFn = unsafe extern "system" fn() -> i32;

/// Initializes dynamic Per-Monitor DPI awareness across Windows Server editions.
pub fn init_server_compatibility() {
    if DPI_INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }

    unsafe {
        // 1. Windows Server 2016 / 2019 / 2022 / 2025: SetProcessDpiAwarenessContext (Per-Monitor V2 = -4)
        if let Ok(user32) = GetModuleHandleA(s!("user32.dll")) {
            if let Some(proc) = GetProcAddress(user32, s!("SetProcessDpiAwarenessContext")) {
                let func: SetProcessDpiAwarenessContextFn = std::mem::transmute(proc);
                if func(-4) != 0 {
                    info!("✅ [SERVER 2022] Per-Monitor V2 DPI awareness active.");
                    log_screen_topology();
                    return;
                }
            }
        }

        // 2. Windows Server 2012 R2: SetProcessDpiAwareness (Per-Monitor = 2) from shcore.dll
        if let Ok(shcore) = LoadLibraryA(s!("shcore.dll")) {
            if let Some(proc) = GetProcAddress(shcore, s!("SetProcessDpiAwareness")) {
                let func: SetProcessDpiAwarenessFn = std::mem::transmute(proc);
                if func(2) == 0 {
                    info!("✅ [SERVER 2012 R2] Per-Monitor DPI awareness active.");
                    log_screen_topology();
                    return;
                }
            }
        }

        // 3. Fallback for Windows Server 2012: SetProcessDPIAware from user32.dll
        if let Ok(user32) = GetModuleHandleA(s!("user32.dll")) {
            if let Some(proc) = GetProcAddress(user32, s!("SetProcessDPIAware")) {
                let func: SetProcessDPIAwareFn = std::mem::transmute(proc);
                if func() != 0 {
                    info!("✅ [SERVER 2012] System DPI awareness active.");
                    log_screen_topology();
                    return;
                }
            }
        }
    }
}

/// Logs full diagnostic details about the server display topology
fn log_screen_topology() {
    let geom = get_server_desktop_geometry();
    info!(
        "🖥️ [SERVER TOPOLOGY] Origin: ({}, {}), Resolution: {}x{} (Full Desktop)",
        geom.x, geom.y, geom.width, geom.height
    );
}

#[derive(Clone, Copy, Debug)]
pub struct DesktopGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Computes the exact full-desktop geometry for Windows Server, handling virtual displays and RDP.
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

/// Captures the complete Windows Server desktop frame into top-down BGRA bytes without edge clipping.
pub fn capture_server_screen_frame(
    capture_cursor: bool,
    scale_pct: u8,
) -> Result<(u32, u32, u32, u32, Vec<u8>)> {
    unsafe {
        let geom = get_server_desktop_geometry();
        let orig_w = geom.width;
        let orig_h = geom.height;

        let clamped_scale = (scale_pct as i32).clamp(50, 100);
        let (cap_w, cap_h) = if clamped_scale >= 100 {
            (orig_w, orig_h)
        } else {
            let cw = ((orig_w * clamped_scale) / 100).max(2) & !1;
            let ch = ((orig_h * clamped_scale) / 100).max(2) & !1;
            (cw, ch)
        };

        let hdc_screen = GetDC(None);
        if hdc_screen.0.is_null() {
            anyhow::bail!("Failed to acquire Windows Server screen DC");
        }

        let mem_dc = CreateCompatibleDC(Some(hdc_screen));
        let mem_bmp = CreateCompatibleBitmap(hdc_screen, cap_w, cap_h);
        let old_obj = SelectObject(mem_dc, HGDIOBJ(mem_bmp.0));

        if cap_w == orig_w && cap_h == orig_h {
            let _ = BitBlt(mem_dc, 0, 0, cap_w, cap_h, Some(hdc_screen), geom.x, geom.y, SRCCOPY);
        } else {
            let _ = SetStretchBltMode(mem_dc, COLORONCOLOR);
            let _ = StretchBlt(
                mem_dc,
                0,
                0,
                cap_w,
                cap_h,
                Some(hdc_screen),
                geom.x,
                geom.y,
                orig_w,
                orig_h,
                SRCCOPY,
            );
        }

        // Overlay hardware cursor if requested
        if capture_cursor {
            let mut ci = CURSORINFO {
                cbSize: std::mem::size_of::<CURSORINFO>() as u32,
                ..Default::default()
            };
            if GetCursorInfo(&mut ci).is_ok()
                && (ci.flags.0 & CURSOR_SHOWING.0 != 0)
                && !ci.hCursor.is_invalid()
            {
                let mut icon_info = ICONINFO::default();
                let (hot_x, hot_y) = if GetIconInfo(HICON(ci.hCursor.0), &mut icon_info).is_ok() {
                    if !icon_info.hbmMask.0.is_null() {
                        let _ = DeleteObject(HGDIOBJ(icon_info.hbmMask.0));
                    }
                    if !icon_info.hbmColor.0.is_null() {
                        let _ = DeleteObject(HGDIOBJ(icon_info.hbmColor.0));
                    }
                    (icon_info.xHotspot as i32, icon_info.yHotspot as i32)
                } else {
                    (0, 0)
                };

                let rel_cursor_x = ci.ptScreenPos.x - geom.x - hot_x;
                let rel_cursor_y = ci.ptScreenPos.y - geom.y - hot_y;

                let cx = (rel_cursor_x * cap_w) / orig_w;
                let cy = (rel_cursor_y * cap_h) / orig_h;

                let _ = DrawIconEx(
                    mem_dc,
                    cx,
                    cy,
                    ci.hCursor.into(),
                    0,
                    0,
                    0,
                    None,
                    DI_NORMAL,
                );
            }
        }

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: cap_w,
                biHeight: -cap_h, // negative height for top-down DIB
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let buffer_size = (cap_w * cap_h * 4) as usize;
        let mut bgra_bytes = vec![0u8; buffer_size];

        GetDIBits(
            mem_dc,
            mem_bmp,
            0,
            cap_h as u32,
            Some(bgra_bytes.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        let _ = SelectObject(mem_dc, old_obj);
        let _ = DeleteObject(HGDIOBJ(mem_bmp.0));
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, hdc_screen);

        Ok((orig_w as u32, orig_h as u32, cap_w as u32, cap_h as u32, bgra_bytes))
    }
}

/// Dual-channel pixel-perfect mouse movement for Windows Server
pub fn inject_server_mouse_move(x: i32, y: i32) {
    let geom = get_server_desktop_geometry();
    unsafe {
        let target_x = geom.x + x.clamp(0, geom.width - 1);
        let target_y = geom.y + y.clamp(0, geom.height - 1);

        // 1. Set GDI cursor position
        let _ = SetCursorPos(target_x, target_y);

        // 2. Inject normalized 0..65535 coordinate into virtual desktop event queue
        let norm_x = (((target_x - geom.x) as i64 * 65535) / geom.width.max(1) as i64) as i32;
        let norm_y = (((target_y - geom.y) as i64 * 65535) / geom.height.max(1) as i64) as i32;
        mouse_event(
            MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE | MOUSEEVENTF_VIRTUALDESK,
            norm_x,
            norm_y,
            0,
            0,
        );
    }
}

/// Injects input events with Windows Server coordinate translation
pub fn handle_server_input_event(event: &InputEvent) {
    unsafe {
        match event {
            InputEvent::MouseMove { x, y } => {
                inject_server_mouse_move(*x, *y);
            }
            InputEvent::MouseDown { button } => {
                let flag = match button {
                    MouseButton::Left => MOUSEEVENTF_LEFTDOWN,
                    MouseButton::Right => MOUSEEVENTF_RIGHTDOWN,
                    MouseButton::Middle => MOUSEEVENTF_MIDDLEDOWN,
                };
                mouse_event(flag, 0, 0, 0, 0);
            }
            InputEvent::MouseUp { button } => {
                let flag = match button {
                    MouseButton::Left => MOUSEEVENTF_LEFTUP,
                    MouseButton::Right => MOUSEEVENTF_RIGHTUP,
                    MouseButton::Middle => MOUSEEVENTF_MIDDLEUP,
                };
                mouse_event(flag, 0, 0, 0, 0);
            }
            InputEvent::MouseWheel { delta } => {
                mouse_event(MOUSEEVENTF_WHEEL, 0, 0, *delta, 0);
            }
            InputEvent::KeyDown { key_code } => {
                keybd_event(*key_code as u8, 0, Default::default(), 0);
            }
            InputEvent::KeyUp { key_code } => {
                keybd_event(*key_code as u8, 0, KEYEVENTF_KEYUP, 0);
            }
        }
    }
}
