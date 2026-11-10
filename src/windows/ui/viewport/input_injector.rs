//! Remote Input Injection & Coordinate Mapping Engine

#![allow(unsafe_op_in_unsafe_fn)]

use std::sync::atomic::Ordering;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

use crate::protocol::{InputEvent, MouseButton};
use super::settings_modal::{DISPLAY_MODE, SHOW_SETTINGS_MODAL};
use super::tabs::ACTIVE_TAB_INDEX;
use super::toolbar::TOOLBAR_HEIGHT;
use super::{REMOTE_ORIG_H, REMOTE_ORIG_W, VIEWPORT_IS_ADMIN_MODE};

#[inline]
pub fn is_remote_session_interactive() -> bool {
    if ACTIVE_TAB_INDEX.load(Ordering::Relaxed) == 0 {
        return false;
    }
    if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
        return false;
    }
    if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
        return false;
    }
    true
}

pub fn map_client_to_screen(
    client_x: i32,
    client_y: i32,
    client_w: i32,
    client_h: i32,
    screen_w: i32,
    screen_h: i32,
    toolbar_h: i32,
    mode: u8,
) -> Option<(i32, i32)> {
    if client_y < toolbar_h {
        return None;
    }
    let view_y = client_y - toolbar_h;
    let view_h = client_h - toolbar_h;
    if view_h <= 0 || client_w <= 0 || screen_w <= 0 || screen_h <= 0 {
        return None;
    }

    match mode {
        0 => {
            let scale_x = client_w as f32 / screen_w as f32;
            let scale_y = view_h as f32 / screen_h as f32;
            let scale = scale_x.min(scale_y);

            let render_w = (screen_w as f32 * scale) as i32;
            let render_h = (screen_h as f32 * scale) as i32;

            let offset_x = (client_w - render_w) / 2;
            let offset_y = (view_h - render_h) / 2;

            let rel_x = client_x - offset_x;
            let rel_y = view_y - offset_y;

            if rel_x >= 0 && rel_x < render_w && rel_y >= 0 && rel_y < render_h {
                let sx = ((rel_x as f32 / scale) as i32).clamp(0, screen_w - 1);
                let sy = ((rel_y as f32 / scale) as i32).clamp(0, screen_h - 1);
                Some((sx, sy))
            } else {
                None
            }
        }
        1 => {
            let sx = ((client_x as f32 / client_w as f32) * screen_w as f32) as i32;
            let sy = ((view_y as f32 / view_h as f32) * screen_h as f32) as i32;
            Some((sx.clamp(0, screen_w - 1), sy.clamp(0, screen_h - 1)))
        }
        _ => {
            let offset_x = (client_w - screen_w) / 2;
            let offset_y = (view_h - screen_h) / 2;
            let sx = client_x - offset_x;
            let sy = view_y - offset_y;
            if sx >= 0 && sx < screen_w && sy >= 0 && sy < screen_h {
                Some((sx, sy))
            } else {
                None
            }
        }
    }
}

pub unsafe fn inject_remote_mouse(
    hwnd: HWND,
    client_x: i32,
    client_y: i32,
    is_down: bool,
    is_right: bool,
) {
    if !is_remote_session_interactive() {
        return;
    }
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let client_w = rect.right - rect.left;
    let client_h = rect.bottom - rect.top;

    let screen_w = REMOTE_ORIG_W.load(Ordering::Relaxed) as i32;
    let screen_h = REMOTE_ORIG_H.load(Ordering::Relaxed) as i32;
    let mode = DISPLAY_MODE.load(Ordering::Relaxed);

    if let Some((sx, sy)) = map_client_to_screen(
        client_x,
        client_y,
        client_w,
        client_h,
        screen_w,
        screen_h,
        TOOLBAR_HEIGHT,
        mode,
    ) {
        crate::p2p::send_remote_input(InputEvent::MouseMove { x: sx, y: sy });
        if is_down {
            let button = if is_right {
                MouseButton::Right
            } else {
                MouseButton::Left
            };
            crate::p2p::send_remote_input(InputEvent::MouseDown { button });
        } else {
            let button = if is_right {
                MouseButton::Right
            } else {
                MouseButton::Left
            };
            crate::p2p::send_remote_input(InputEvent::MouseUp { button });
        }
    }
}

pub unsafe fn inject_remote_mouse_move(
    hwnd: HWND,
    client_x: i32,
    client_y: i32,
) {
    if !is_remote_session_interactive() {
        return;
    }
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let client_w = rect.right - rect.left;
    let client_h = rect.bottom - rect.top;

    let screen_w = REMOTE_ORIG_W.load(Ordering::Relaxed) as i32;
    let screen_h = REMOTE_ORIG_H.load(Ordering::Relaxed) as i32;
    let mode = DISPLAY_MODE.load(Ordering::Relaxed);

    if let Some((sx, sy)) = map_client_to_screen(
        client_x,
        client_y,
        client_w,
        client_h,
        screen_w,
        screen_h,
        TOOLBAR_HEIGHT,
        mode,
    ) {
        crate::p2p::send_remote_input(InputEvent::MouseMove { x: sx, y: sy });
    }
}

pub unsafe fn inject_remote_mouse_middle(
    hwnd: HWND,
    client_x: i32,
    client_y: i32,
    is_down: bool,
) {
    if !is_remote_session_interactive() {
        return;
    }
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let client_w = rect.right - rect.left;
    let client_h = rect.bottom - rect.top;

    let screen_w = REMOTE_ORIG_W.load(Ordering::Relaxed) as i32;
    let screen_h = REMOTE_ORIG_H.load(Ordering::Relaxed) as i32;
    let mode = DISPLAY_MODE.load(Ordering::Relaxed);

    if let Some((sx, sy)) = map_client_to_screen(
        client_x,
        client_y,
        client_w,
        client_h,
        screen_w,
        screen_h,
        TOOLBAR_HEIGHT,
        mode,
    ) {
        crate::p2p::send_remote_input(InputEvent::MouseMove { x: sx, y: sy });
        if is_down {
            crate::p2p::send_remote_input(InputEvent::MouseDown {
                button: MouseButton::Middle,
            });
        } else {
            crate::p2p::send_remote_input(InputEvent::MouseUp {
                button: MouseButton::Middle,
            });
        }
    }
}

pub fn inject_remote_wheel(delta: i32) {
    if !is_remote_session_interactive() {
        return;
    }
    crate::p2p::send_remote_input(InputEvent::MouseWheel { delta });
}

pub fn inject_remote_key(key_code: u32, is_down: bool) {
    if !is_remote_session_interactive() {
        return;
    }
    if is_down {
        crate::p2p::send_remote_input(InputEvent::KeyDown { key_code });
    } else {
        crate::p2p::send_remote_input(InputEvent::KeyUp { key_code });
    }
}
