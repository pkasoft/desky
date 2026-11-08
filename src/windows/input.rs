use crate::protocol::InputEvent;
use crate::windows::WindowsPlatform;

pub struct InputInjector;

impl InputInjector {
    pub fn new() -> Self {
        Self
    }

    pub fn handle_event(&self, event: &InputEvent) {
        match super::get_platform() {
            WindowsPlatform::Win10_11 => super::win10_11::input::handle_event(event),
            WindowsPlatform::Server2022 => super::server_2022::input::handle_event(event),
            WindowsPlatform::Server2016_2019 => super::server_2016_2019::input::handle_event(event),
            WindowsPlatform::Server2012 => super::server_2012::input::handle_event(event),
            WindowsPlatform::Win7_8 => super::win7_8::input::handle_event(event),
        }
    }
}
