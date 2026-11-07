use crate::protocol::{InputEvent, MouseButton};
use x11rb::connection::Connection;
use x11rb::protocol::xtest;

pub struct InputInjector;

impl InputInjector {
    pub fn new() -> Self {
        Self
    }

    pub fn handle_event(&self, event: &InputEvent) {
        match event {
            InputEvent::MouseMove { x, y } => {
                if let Ok((conn, screen_num)) = x11rb::connect(None) {
                    let root = conn.setup().roots[screen_num].root;
                    let _ = xtest::fake_input(&conn, 6, 0, 0, root, *x as i16, *y as i16, 0); // 6 = MotionNotify
                    let _ = conn.flush();
                }
            }
            InputEvent::MouseDown { button } => {
                if let Ok((conn, _)) = x11rb::connect(None) {
                    let btn = match button {
                        MouseButton::Left => 1,
                        MouseButton::Middle => 2,
                        MouseButton::Right => 3,
                    };
                    let _ = xtest::fake_input(&conn, 4, btn, 0, 0, 0, 0, 0); // 4 = ButtonPress
                    let _ = conn.flush();
                }
            }
            InputEvent::MouseUp { button } => {
                if let Ok((conn, _)) = x11rb::connect(None) {
                    let btn = match button {
                        MouseButton::Left => 1,
                        MouseButton::Middle => 2,
                        MouseButton::Right => 3,
                    };
                    let _ = xtest::fake_input(&conn, 5, btn, 0, 0, 0, 0, 0); // 5 = ButtonRelease
                    let _ = conn.flush();
                }
            }
            InputEvent::MouseWheel { delta } => {
                if let Ok((conn, _)) = x11rb::connect(None) {
                    let btn = if *delta > 0 { 4 } else { 5 }; // Wheel Up = 4, Wheel Down = 5
                    let _ = xtest::fake_input(&conn, 4, btn, 0, 0, 0, 0, 0);
                    let _ = xtest::fake_input(&conn, 5, btn, 0, 0, 0, 0, 0);
                    let _ = conn.flush();
                }
            }
            InputEvent::KeyDown { key_code } => {
                if let Ok((conn, _)) = x11rb::connect(None) {
                    let _ = xtest::fake_input(&conn, 2, *key_code as u8, 0, 0, 0, 0, 0); // 2 = KeyPress
                    let _ = conn.flush();
                }
            }
            InputEvent::KeyUp { key_code } => {
                if let Ok((conn, _)) = x11rb::connect(None) {
                    let _ = xtest::fake_input(&conn, 3, *key_code as u8, 0, 0, 0, 0, 0); // 3 = KeyRelease
                    let _ = conn.flush();
                }
            }
        }
    }
}
