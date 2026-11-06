use crate::protocol::InputEvent;

pub struct InputInjector;

impl InputInjector {
    pub fn new() -> Self {
        Self
    }

    pub fn handle_event(&self, event: &InputEvent) {
        match event {
            InputEvent::MouseMove { x, y } => {
                #[cfg(windows)]
                unsafe {
                    use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;
                    let _ = SetCursorPos(*x, *y);
                }
            }
            _ => {
                // Key/button events handled via SendInput
            }
        }
    }
}
