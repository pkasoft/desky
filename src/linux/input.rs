use crate::protocol::{InputEvent, MouseButton};
use std::sync::Mutex;
use x11rb::connection::Connection;
use x11rb::protocol::xtest;
use x11rb::rust_connection::RustConnection;

pub struct InputInjector {
    conn: Mutex<Option<(RustConnection, u32)>>,
}

impl InputInjector {
    pub fn new() -> Self {
        let initial = x11rb::connect(None)
            .ok()
            .map(|(c, screen_num)| {
                let root = c.setup().roots[screen_num].root;
                (c, root)
            });
        Self {
            conn: Mutex::new(initial),
        }
    }

    pub fn handle_event(&self, event: &InputEvent) {
        let Ok(mut guard) = self.conn.lock() else {
            return;
        };
        if guard.is_none() {
            if let Ok((c, screen_num)) = x11rb::connect(None) {
                let root = c.setup().roots[screen_num].root;
                *guard = Some((c, root));
            }
        }
        let Some((conn, root)) = guard.as_ref() else {
            return;
        };

        let ok = match event {
            InputEvent::MouseMove { x, y } => {
                xtest::fake_input(conn, 6, 0, 0, *root, *x as i16, *y as i16, 0).is_ok()
                    && conn.flush().is_ok()
            }
            InputEvent::MouseDown { button } => {
                let btn = match button {
                    MouseButton::Left => 1,
                    MouseButton::Middle => 2,
                    MouseButton::Right => 3,
                };
                xtest::fake_input(conn, 4, btn, 0, 0, 0, 0, 0).is_ok() && conn.flush().is_ok()
            }
            InputEvent::MouseUp { button } => {
                let btn = match button {
                    MouseButton::Left => 1,
                    MouseButton::Middle => 2,
                    MouseButton::Right => 3,
                };
                xtest::fake_input(conn, 5, btn, 0, 0, 0, 0, 0).is_ok() && conn.flush().is_ok()
            }
            InputEvent::MouseWheel { delta } => {
                let btn = if *delta > 0 { 4 } else { 5 };
                xtest::fake_input(conn, 4, btn, 0, 0, 0, 0, 0).is_ok()
                    && xtest::fake_input(conn, 5, btn, 0, 0, 0, 0, 0).is_ok()
                    && conn.flush().is_ok()
            }
            InputEvent::KeyDown { key_code } => {
                if let Some(kc) = win_vk_to_x11_keycode(*key_code) {
                    xtest::fake_input(conn, 2, kc, 0, 0, 0, 0, 0).is_ok()
                        && conn.flush().is_ok()
                } else {
                    true
                }
            }
            InputEvent::KeyUp { key_code } => {
                if let Some(kc) = win_vk_to_x11_keycode(*key_code) {
                    xtest::fake_input(conn, 3, kc, 0, 0, 0, 0, 0).is_ok()
                        && conn.flush().is_ok()
                } else {
                    true
                }
            }
        };

        if !ok {
            *guard = None;
        }
    }
}

fn win_vk_to_x11_keycode(vk: u32) -> Option<u8> {
    let kc = match vk {
        0x08 => 22,  // Backspace
        0x09 => 23,  // Tab
        0x0D => 36,  // Return / Enter
        0x10 | 0xA0 => 50, // Shift_L
        0xA1 => 62,  // Shift_R
        0x11 | 0xA2 => 37, // Control_L
        0xA3 => 105, // Control_R
        0x12 | 0xA4 => 64, // Alt_L
        0xA5 => 108, // Alt_R
        0x14 => 66,  // Caps_Lock
        0x1B => 9,   // Escape
        0x20 => 65,  // Space
        0x21 => 112, // Page_Up
        0x22 => 117, // Page_Down
        0x23 => 115, // End
        0x24 => 110, // Home
        0x25 => 113, // Left
        0x26 => 111, // Up
        0x27 => 114, // Right
        0x28 => 116, // Down
        0x2D => 118, // Insert
        0x2E => 119, // Delete
        0x30 => 19,  // 0
        0x31..=0x39 => (vk - 0x31 + 10) as u8, // 1..9
        0x41 => 38,  // A
        0x42 => 56,  // B
        0x43 => 54,  // C
        0x44 => 40,  // D
        0x45 => 26,  // E
        0x46 => 41,  // F
        0x47 => 42,  // G
        0x48 => 43,  // H
        0x49 => 31,  // I
        0x4A => 44,  // J
        0x4B => 45,  // K
        0x4C => 46,  // L
        0x4D => 58,  // M
        0x4E => 57,  // N
        0x4F => 32,  // O
        0x50 => 33,  // P
        0x51 => 24,  // Q
        0x52 => 27,  // R
        0x53 => 39,  // S
        0x54 => 28,  // T
        0x55 => 30,  // U
        0x56 => 55,  // V
        0x57 => 25,  // W
        0x58 => 53,  // X
        0x59 => 29,  // Y
        0x5A => 52,  // Z
        0x5B => 133, // Super_L (Win)
        0x70..=0x79 => (vk - 0x70 + 67) as u8, // F1..F10
        0x7A => 95,  // F11
        0x7B => 96,  // F12
        0xBA => 47,  // ;:
        0xBB => 21,  // =+
        0xBC => 59,  // ,<
        0xBD => 20,  // -_
        0xBE => 60,  // .>
        0xBF => 61,  // /?
        0xC0 => 49,  // `~
        0xDB => 34,  // [{
        0xDC => 51,  // \|
        0xDD => 35,  // ]}
        0xDE => 48,  // '"
        _ => return None,
    };
    Some(kc)
}

