//! Windows Server 2012 & 2012 R2 Input Injection Engine
//!
//! Provides reliable low-latency mouse and keyboard injection for Windows Server 2012 / R2
//! virtual desktop environments, VM consoles, and remote sessions.

use crate::protocol::{InputEvent, MouseButton};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, mouse_event, KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
};
use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;

pub fn handle_event(event: &InputEvent) {
    unsafe {
        match event {
            InputEvent::MouseMove { x, y } => {
                let geom = super::get_server_desktop_geometry();
                let target_x = geom.x + (*x).clamp(0, geom.width - 1);
                let target_y = geom.y + (*y).clamp(0, geom.height - 1);

                // 1. Direct hardware cursor placement
                let _ = SetCursorPos(target_x, target_y);

                // 2. Virtual desktop coordinate queue injection
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
