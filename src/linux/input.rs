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
                xtest::fake_input(conn, 2, *key_code as u8, 0, 0, 0, 0, 0).is_ok()
                    && conn.flush().is_ok()
            }
            InputEvent::KeyUp { key_code } => {
                xtest::fake_input(conn, 3, *key_code as u8, 0, 0, 0, 0, 0).is_ok()
                    && conn.flush().is_ok()
            }
        };

        if !ok {
            *guard = None;
        }
    }
}
