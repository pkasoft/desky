use crate::protocol::InputEvent;
use crate::windows::WindowsPlatform;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, MapVirtualKeyW, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    MAPVK_VK_TO_VSC,
};

#[inline]
fn is_extended_vk(vk: u32) -> bool {
    matches!(
        vk,
        0x21..=0x28 // PageUp, PageDown, End, Home, Left, Up, Right, Down
            | 0x2D  // Insert
            | 0x2E  // Delete
            | 0x5B  // LWin
            | 0x5C  // RWin
            | 0x5D  // Apps
            | 0x6F  // Divide
            | 0x90  // NumLock
            | 0xA3  // RControl
            | 0xA5  // RMenu (Right Alt)
    )
}

/// Injects a virtual key press or release with its hardware scan code and extended-key flag
/// so Windows shell shortcuts (including Alt+Tab) work reliably on the remote host.
pub fn inject_key_event(key_code: u32, is_down: bool) {
    unsafe {
        let scan = MapVirtualKeyW(key_code, MAPVK_VK_TO_VSC) as u8;
        let mut flags = KEYBD_EVENT_FLAGS::default();
        if !is_down {
            flags |= KEYEVENTF_KEYUP;
        }
        if is_extended_vk(key_code) {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        keybd_event(key_code as u8, scan, flags, 0);
    }
}

pub struct InputInjector;

impl InputInjector {
    pub fn new() -> Self {
        Self
    }

    pub fn handle_event(&self, event: &InputEvent) {
        super::desktop::sync_thread_desktop_to_input();
        match super::get_platform() {
            WindowsPlatform::Win10_11 => super::win10_11::input::handle_event(event),
            WindowsPlatform::Server2022 => super::server_2022::input::handle_event(event),
            WindowsPlatform::Server2016_2019 => super::server_2016_2019::input::handle_event(event),
            WindowsPlatform::Server2012 => super::server_2012::input::handle_event(event),
            WindowsPlatform::Win7_8 => super::win7_8::input::handle_event(event),
        }
    }
}
