//! Remote Input Injection & Coordinate Mapping Engine (Linux X11)
//!
//! Mirrors `src/windows/ui/viewport/input_injector.rs` by translating viewport
//! coordinates and forwarding mouse/keyboard events to the remote peer.

use std::sync::atomic::Ordering;
use x11rb::protocol::xproto::KeyButMask;

use super::tabs::ACTIVE_TAB_INDEX;
use super::toolbar::TOOLBAR_HEIGHT;
use crate::protocol::{InputEvent, MouseButton};

#[inline]
pub fn is_remote_session_interactive() -> bool {
    ACTIVE_TAB_INDEX.load(Ordering::Relaxed) > 0
}

pub fn map_client_to_screen(
    mx: i16,
    my: i16,
    win_w: u16,
    win_h: u16,
    orig_w: u32,
    orig_h: u32,
) -> Option<(i32, i32)> {
    let tb_h = TOOLBAR_HEIGHT as i16;
    if my < tb_h || orig_w == 0 || orig_h == 0 {
        return None;
    }
    let view_w = win_w as f32;
    let view_h = win_h.saturating_sub(TOOLBAR_HEIGHT) as f32;
    let scale = (view_w / orig_w as f32).min(view_h / orig_h as f32);
    let render_w = orig_w as f32 * scale;
    let render_h = orig_h as f32 * scale;
    let off_x = (view_w - render_w) / 2.0;
    let off_y = (view_h - render_h) / 2.0;

    let rel_x = (mx as f32) - off_x;
    let rel_y = (my - tb_h) as f32 - off_y;
    if rel_x >= 0.0 && rel_x < render_w && rel_y >= 0.0 && rel_y < render_h {
        let sx = ((rel_x / scale) as i32).clamp(0, orig_w as i32 - 1);
        let sy = ((rel_y / scale) as i32).clamp(0, orig_h as i32 - 1);
        Some((sx, sy))
    } else {
        None
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
        if let Some(text) = crate::p2p::read_clipboard_text() {
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
