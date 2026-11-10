//! Windows 10 & Windows 11 Input Injection

use crate::protocol::{InputEvent, MouseButton};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, mouse_event, KEYEVENTF_KEYUP, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_WHEEL,
};
use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;

pub fn handle_event(event: &InputEvent) {
    unsafe {
        match event {
            InputEvent::MouseMove { x, y } => {
                let off_x = crate::windows::dxgi_capture::get_active_monitor_offset_x();
                let off_y = crate::windows::dxgi_capture::get_active_monitor_offset_y();
                let _ = SetCursorPos(*x + off_x, *y + off_y);
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
