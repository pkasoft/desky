use crate::protocol::InputEvent;

pub struct InputInjector;

impl InputInjector {
    pub fn new() -> Self {
        Self
    }

    pub fn handle_event(&self, event: &InputEvent) {
        crate::windows::server2022::handle_server_input_event(event);
    }
}
