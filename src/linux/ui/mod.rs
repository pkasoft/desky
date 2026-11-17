//! Linux User Interface Module (X11 Native Canvas & Multi-Window Architecture)
//!
//! Mirrors `src/windows/ui/` modular organization:
//! - `theme.rs`: pkasoft.com White Theme styling, X11 GCs, core font loader & drawing helpers
//! - `launcher.rs`: Responsive Home page cards, layout geometry, status logs & approval popup
//! - `remote_desk_edit.rs`: Remote Desk 9-digit ID input buffer, formatting & keyboard handler
//! - `host_widget.rs`: Active host session banner & client disconnect control
//! - `progress.rs`: File transfer progress notification state
//! - `admin.rs`: Remote administration view & process controls
//! - `viewport/`: Tabbed remote stream canvas, window event loop, toolbar & input injection

pub mod admin;
pub mod host_widget;
pub mod launcher;
pub mod progress;
pub mod remote_desk_edit;
pub mod theme;
pub mod viewport;

#[allow(unused_imports)]
pub use host_widget::sync_host_session_ui;
#[allow(unused_imports)]
pub use launcher::{
    append_status_log, bring_main_window_to_front, prompt_incoming_request, request_main_redraw,
    set_assigned_desky_id, spawn_linux_approval_popup, LinuxApp, LOCAL_DESKY_ID,
};
pub use progress::trigger_transfer_progress_window;
