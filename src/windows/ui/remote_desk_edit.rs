#![allow(dead_code, unsafe_op_in_unsafe_fn)]
//! Dedicated Win32 Remote Desk Edit Control & Subclass Engine
//!
//! Handles keyboard input, clipboard shortcuts (Ctrl+C, Ctrl+V, Ctrl+A, Ctrl+X),
//! native cue banner placeholder, and Enter-to-Connect for the Home dashboard.

use std::sync::atomic::{AtomicIsize, Ordering};
use tracing::info;
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::FW_SEMIBOLD;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, SetFocus, VK_CONTROL, VK_RETURN};
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, CreateWindowExW, GetParent, IsWindow, SendMessageW,
    SetWindowLongPtrW, GWLP_WNDPROC, HMENU, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CHAR, WM_KEYDOWN,
    WM_SETFONT, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
};

use super::launcher::{handle_homepage_connect, ResponsiveLayout, ID_EDIT_TARGET_CODE};
use super::theme;

const EM_SETSEL: u32 = 0x00B1;
const EM_SETCUEBANNER: u32 = 0x1501;
const WM_CUT: u32 = 0x0300;
const WM_COPY: u32 = 0x0301;
const WM_PASTE: u32 = 0x0302;
const ES_AUTOHSCROLL: u32 = 0x0080;

static PREV_EDIT_WNDPROC: AtomicIsize = AtomicIsize::new(0);

/// Creates the Win32 Edit control for entering Remote Desky IDs
pub unsafe fn create_remote_desk_edit(
    parent: HWND,
    l: ResponsiveLayout,
    h_instance: HINSTANCE,
) -> HWND {
    let edit_hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        w!("EDIT"),
        w!(""),
        WS_VISIBLE | WS_CHILD | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL),
        l.edit_x,
        l.edit_y,
        l.edit_w,
        l.edit_h,
        Some(parent),
        Some(HMENU(ID_EDIT_TARGET_CODE as *mut _)),
        Some(h_instance),
        None,
    )
    .unwrap_or_default();

    if !edit_hwnd.0.is_null() {
        // Set native cue banner placeholder
        let hint = w!("e.g. 800 258 756");
        let _ = SendMessageW(
            edit_hwnd,
            EM_SETCUEBANNER,
            Some(WPARAM(1)), // show even when focused
            Some(LPARAM(hint.as_ptr() as isize)),
        );

        // Apply Segoe UI Semibold font
        let f_edit_size = (20.0 * l.font_scale) as i32;
        let font_edit = theme::create_segoe_font(f_edit_size, FW_SEMIBOLD);
        let _ = SendMessageW(
            edit_hwnd,
            WM_SETFONT,
            Some(WPARAM(font_edit.0 as usize)),
            Some(LPARAM(1)),
        );

        // Subclass for keyboard accelerators & Enter key handling
        subclass_remote_desk_edit(edit_hwnd);
        info!("Remote Desk Edit Control created & subclassed successfully");
    }

    edit_hwnd
}

/// Subclasses the Edit control to intercept Enter, Ctrl+C, Ctrl+V, Ctrl+A, Ctrl+X
pub unsafe fn subclass_remote_desk_edit(hwnd: HWND) {
    if hwnd.0.is_null() || !IsWindow(Some(hwnd)).as_bool() {
        return;
    }
    let old = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, remote_desk_edit_proc as *const () as usize as isize);
    if old != 0 {
        PREV_EDIT_WNDPROC.store(old, Ordering::Relaxed);
    }
}

/// Subclass window procedure for the Remote Desk Edit Control
pub unsafe extern "system" fn remote_desk_edit_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let old_proc = PREV_EDIT_WNDPROC.load(Ordering::Relaxed);
    let old_wndproc = if old_proc != 0 {
        Some(std::mem::transmute::<
            isize,
            unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
        >(old_proc))
    } else {
        None
    };

    match msg {
        WM_KEYDOWN => {
            let ctrl = (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
            let key = wparam.0 as i32;

            // 1. Enter key: Initiate Connection!
            if key == VK_RETURN.0 as i32 {
                let parent = GetParent(hwnd).unwrap_or_default();
                if !parent.0.is_null() {
                    handle_homepage_connect(parent);
                }
                return LRESULT(0);
            }

            // 2. Control Key Combinations
            if ctrl {
                match key {
                    // Ctrl + A -> Select All
                    0x41 => {
                        let _ = SendMessageW(hwnd, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
                        return LRESULT(0);
                    }
                    // Ctrl + C -> Copy
                    0x43 => {
                        let _ = SendMessageW(hwnd, WM_COPY, None, None);
                        return LRESULT(0);
                    }
                    // Ctrl + V -> Paste
                    0x56 => {
                        let _ = SendMessageW(hwnd, WM_PASTE, None, None);
                        return LRESULT(0);
                    }
                    // Ctrl + X -> Cut
                    0x58 => {
                        let _ = SendMessageW(hwnd, WM_CUT, None, None);
                        return LRESULT(0);
                    }
                    _ => {}
                }
            }
        }
        WM_CHAR => {
            let ch = wparam.0 as u16;
            // Suppress default Windows ding beep on Enter, Ctrl+A, Ctrl+C, Ctrl+V, Ctrl+X
            if ch == 0x0D || ch == 0x01 || ch == 0x03 || ch == 0x16 || ch == 0x18 {
                return LRESULT(0);
            }
        }
        _ => {}
    }

    if let Some(prev) = old_wndproc {
        CallWindowProcW(Some(prev), hwnd, msg, wparam, lparam)
    } else {
        windows::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

/// Explicitly focus the Remote Desk edit control and move caret to end
pub unsafe fn focus_remote_desk_edit(hwnd_edit: HWND) {
    if !hwnd_edit.0.is_null() && IsWindow(Some(hwnd_edit)).as_bool() {
        let _ = SetFocus(Some(hwnd_edit));
        let len = windows::Win32::UI::WindowsAndMessaging::GetWindowTextLengthW(hwnd_edit);
        let _ = SendMessageW(hwnd_edit, EM_SETSEL, Some(WPARAM(len as usize)), Some(LPARAM(len as isize)));
    }
}
