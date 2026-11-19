//! Native Win32 System Tray Icon & Context Menu
//!
//! Provides a lightweight, zero-dependency system tray icon via `Shell_NotifyIconW`:
//! - Renders a crisp 32x32 emerald Desky icon directly in memory via `CreateIconIndirect`
//! - Left-click / Double-click restores & focuses the main Desky window
//! - Right-click opens a native popup menu ("Open Desky" / "Quit Desky")

#![allow(unsafe_op_in_unsafe_fn)]

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos,
    SetForegroundWindow, ShowWindow, TrackPopupMenu, HICON, ICONINFO, MF_SEPARATOR, MF_STRING,
    SW_HIDE, SW_RESTORE, SW_SHOW, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON, WM_USER,
};

pub const WM_TRAYICON: u32 = WM_USER + 101;
pub const TRAY_ICON_ID: u32 = 1;
pub const ID_TRAY_OPEN: usize = 4001;
pub const ID_TRAY_QUIT: usize = 4002;

static TRAY_ICON_ADDED: AtomicBool = AtomicBool::new(false);
static TRAY_HICON: AtomicIsize = AtomicIsize::new(0);

unsafe fn create_desky_tray_icon() -> HICON {
    let existing = TRAY_HICON.load(Ordering::Relaxed);
    if existing != 0 {
        return HICON(existing as *mut _);
    }

    let size = 32i32;
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
    let hbm_color = match CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0) {
        Ok(h) => h,
        Err(_) => return HICON::default(),
    };

    if !bits_ptr.is_null() {
        let pixels = std::slice::from_raw_parts_mut(bits_ptr as *mut u32, (size * size) as usize);
        // Emerald background: #2D6A4F (BGRA: 0xFF_2D_6A_4F)
        let bg: u32 = 0xFF2D6A4F;
        let border: u32 = 0xFF1B4332;
        let white: u32 = 0xFFFFFFFF;

        for y in 0..size {
            for x in 0..size {
                let idx = (y * size + x) as usize;
                // Rounded square mask (radius 6)
                let dx = if x < 6 { 6 - x } else if x >= size - 6 { x - (size - 7) } else { 0 };
                let dy = if y < 6 { 6 - y } else if y >= size - 6 { y - (size - 7) } else { 0 };
                let dist_sq = dx * dx + dy * dy;

                if dx > 0 && dy > 0 && dist_sq > 36 {
                    pixels[idx] = 0x00000000; // transparent
                } else if x == 0 || y == 0 || x == size - 1 || y == size - 1 || (dx > 0 && dy > 0 && dist_sq >= 25) {
                    pixels[idx] = border;
                } else {
                    pixels[idx] = bg;
                }
            }
        }

        // Draw crisp white stylized "D" / monitor mark in center (x: 9..23, y: 8..24)
        for y in 8..24 {
            for x in 9..24 {
                let idx = (y * size + x) as usize;
                let is_left_bar = x >= 9 && x <= 12;
                let is_top_bar = (y >= 8 && y <= 11) && x <= 19;
                let is_bot_bar = (y >= 20 && y <= 23) && x <= 19;
                let is_right_curve = (x >= 19 && x <= 22) && (y >= 10 && y <= 21)
                    && !((y == 10 || y == 21) && x == 22);
                let is_inner_dot = (x >= 15 && x <= 17) && (y >= 14 && y <= 17);
                if is_left_bar || is_top_bar || is_bot_bar || is_right_curve || is_inner_dot {
                    pixels[idx] = white;
                }
            }
        }
    }

    let mask_bytes = [0u8; 128]; // 32x32 / 8 = 128 bytes
    let hbm_mask: HBITMAP = CreateBitmap(size, size, 1, 1, Some(mask_bytes.as_ptr() as *const _));

    let icon_info = ICONINFO {
        fIcon: true.into(),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: hbm_mask,
        hbmColor: hbm_color,
    };
    let hicon = CreateIconIndirect(&icon_info).unwrap_or_default();

    let _ = DeleteObject(HGDIOBJ(hbm_color.0));
    let _ = DeleteObject(HGDIOBJ(hbm_mask.0));

    if !hicon.is_invalid() {
        TRAY_HICON.store(hicon.0 as isize, Ordering::Relaxed);
    }
    hicon
}

fn fill_tip(buf: &mut [u16; 128], text: &str) {
    buf.fill(0);
    for (i, ch) in text.encode_utf16().take(127).enumerate() {
        buf[i] = ch;
    }
}

pub unsafe fn add_tray_icon(hwnd: HWND) {
    if TRAY_ICON_ADDED.swap(true, Ordering::SeqCst) {
        return;
    }
    let hicon = create_desky_tray_icon();
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ICON_ID,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAYICON,
        hIcon: hicon,
        ..Default::default()
    };
    fill_tip(&mut nid.szTip, "Desky Remote Desktop - Online");
    let _ = Shell_NotifyIconW(NIM_ADD, &nid);
}

pub unsafe fn update_tray_tooltip(hwnd: HWND, tip: &str) {
    if !TRAY_ICON_ADDED.load(Ordering::Relaxed) {
        return;
    }
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ICON_ID,
        uFlags: NIF_TIP,
        ..Default::default()
    };
    fill_tip(&mut nid.szTip, tip);
    let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
}

pub unsafe fn remove_tray_icon(hwnd: HWND) {
    if !TRAY_ICON_ADDED.swap(false, Ordering::SeqCst) {
        return;
    }
    let nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ICON_ID,
        ..Default::default()
    };
    let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    let old = TRAY_HICON.swap(0, Ordering::Relaxed);
    if old != 0 {
        let _ = DestroyIcon(HICON(old as *mut _));
    }
}

pub unsafe fn show_tray_context_menu(hwnd: HWND) {
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let _ = SetForegroundWindow(hwnd);

    if let Ok(hmenu) = CreatePopupMenu() {
        let _ = AppendMenuW(hmenu, MF_STRING, ID_TRAY_OPEN, w!("Open Desky"));
        let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(hmenu, MF_STRING, ID_TRAY_QUIT, w!("Quit Desky"));
        let _ = TrackPopupMenu(
            hmenu,
            TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_LEFTALIGN,
            pt.x,
            pt.y,
            Some(0),
            hwnd,
            None,
        );
        let _ = DestroyMenu(hmenu);
    }
}

pub unsafe fn show_main_window(hwnd: HWND) {
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = ShowWindow(hwnd, SW_RESTORE);
    let _ = SetForegroundWindow(hwnd);
}

pub unsafe fn hide_main_window(hwnd: HWND) {
    let _ = ShowWindow(hwnd, SW_HIDE);
}
