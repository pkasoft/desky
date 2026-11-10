//! Windows User Interface Module (Win32 Native Canvas & Multi-Window Architecture)

pub mod admin;
pub mod host_widget;
pub mod launcher;
pub mod progress;
pub mod remote_desk_edit;
pub mod theme;
pub mod viewport;

pub use host_widget::sync_host_session_ui;

pub use launcher::{
    append_status_log, prompt_incoming_request, request_main_redraw, set_assigned_desky_id,
    NativeWin32App, LOCAL_DESKY_ID,
};
pub use progress::trigger_transfer_progress_window;
