//! Windows User Interface Module (Win32 Native Canvas & Multi-Window Architecture)

pub mod admin;
pub mod launcher;
pub mod progress;
pub mod theme;
pub mod viewport;

pub use launcher::{
    append_status_log, prompt_incoming_request, request_main_redraw, set_assigned_desky_id,
    NativeWin32App,
};
pub use progress::trigger_transfer_progress_window;
