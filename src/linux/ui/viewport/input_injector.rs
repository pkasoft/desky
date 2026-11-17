//! Remote Input Injection & Coordinate Mapping Engine (Linux X11)
//!
//! Mirrors `src/windows/ui/viewport/input_injector.rs` by translating viewport
//! coordinates across Auto Fit (0), Stretch (1), and 1:1 (2) display modes and
//! forwarding mouse/keyboard events to the remote peer.

use std::sync::atomic::Ordering;
use x11rb::protocol::xproto::KeyButMask;

use super::settings_modal::{DISPLAY_MODE, SHOW_SETTINGS_MODAL};
use super::tabs::{ACTIVE_TAB_INDEX, VIEWPORT_IS_ADMIN_MODE};
use super::toolbar::TOOLBAR_HEIGHT;
use crate::protocol::{InputEvent, MouseButton};

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
    mx: i16,
    my: i16,
    win_w: u16,
    win_h: u16,
    orig_w: u32,
    orig_h: u32,
) -> Option<(i32, i32)> {
    let tb_h = TOOLBAR_HEIGHT as i32;
    let client_x = mx as i32;
    let client_y = my as i32;
    let client_w = win_w as i32;
    let client_h = win_h as i32;
    let screen_w = orig_w as i32;
    let screen_h = orig_h as i32;

    if client_y < tb_h {
        return None;
    }
    let view_y = client_y - tb_h;
    let view_h = client_h - tb_h;
    if view_h <= 0 || client_w <= 0 || screen_w <= 0 || screen_h <= 0 {
        return None;
    }

    let mode = DISPLAY_MODE.load(Ordering::Relaxed);
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

pub fn inject_remote_mouse_button(
    mx: i16,
    my: i16,
    win_w: u16,
    win_h: u16,
    orig_w: u32,
    orig_h: u32,
    button: u8,
    is_down: bool,
) {
    if !is_remote_session_interactive() {
        return;
    }
    if let Some((rx, ry)) = map_client_to_screen(mx, my, win_w, win_h, orig_w, orig_h) {
        crate::p2p::send_remote_input(InputEvent::MouseMove { x: rx, y: ry });
        match button {
            1 => {
                let ev = if is_down {
                    InputEvent::MouseDown {
                        button: MouseButton::Left,
                    }
                } else {
                    InputEvent::MouseUp {
                        button: MouseButton::Left,
                    }
                };
                crate::p2p::send_remote_input(ev);
            }
            2 => {
                let ev = if is_down {
                    InputEvent::MouseDown {
                        button: MouseButton::Middle,
                    }
                } else {
                    InputEvent::MouseUp {
                        button: MouseButton::Middle,
                    }
                };
                crate::p2p::send_remote_input(ev);
            }
            3 => {
                let ev = if is_down {
                    InputEvent::MouseDown {
                        button: MouseButton::Right,
                    }
                } else {
                    InputEvent::MouseUp {
                        button: MouseButton::Right,
                    }
                };
                crate::p2p::send_remote_input(ev);
            }
            4 if is_down => {
                crate::p2p::send_remote_input(InputEvent::MouseWheel { delta: 120 });
            }
            5 if is_down => {
                crate::p2p::send_remote_input(InputEvent::MouseWheel { delta: -120 });
            }
            _ => {}
        }
    }
}

pub fn inject_remote_mouse_move(
    mx: i16,
    my: i16,
    win_w: u16,
    win_h: u16,
    orig_w: u32,
    orig_h: u32,
) {
    if !is_remote_session_interactive() {
        return;
    }
    if let Some((rx, ry)) = map_client_to_screen(mx, my, win_w, win_h, orig_w, orig_h) {
        crate::p2p::send_remote_input(InputEvent::MouseMove { x: rx, y: ry });
    }
}

pub fn inject_remote_key(keycode: u8, state: KeyButMask, is_down: bool) {
    if !is_remote_session_interactive() {
        return;
    }
    let is_ctrl = state.contains(KeyButMask::CONTROL);
    if is_down && is_ctrl && keycode == 55 {
        let files = crate::p2p::read_clipboard_files();
        if !files.is_empty() {
            for f in files {
                crate::p2p::send_file_transfer(f);
            }
            return;
        } else if let Some(text) = crate::p2p::read_clipboard_text() {
            if !text.is_empty() {
                crate::p2p::send_remote_clipboard(&text);
            }
        }
    }

    let ev = if is_down {
        InputEvent::KeyDown {
            key_code: keycode as u32,
        }
    } else {
        InputEvent::KeyUp {
            key_code: keycode as u32,
        }
    };
    crate::p2p::send_remote_input(ev);
}
