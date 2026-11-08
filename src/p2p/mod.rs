pub mod client;
pub mod host;
pub mod state;

pub use client::start_client_session;
pub use host::P2PManager;
pub use state::{
    get_active_quality_params, get_latest_remote_frame, get_pending_approval, get_quality_mode,
    get_server_ping_ms, get_session_bitrate_kbps, get_session_rtt_ms, get_session_status,
    is_server_online, read_clipboard_text, respond_to_pending_approval, send_file_transfer,
    send_remote_clipboard, send_remote_input, set_quality_mode, stop_client_session,
    update_client_preferences, SessionStatus,
};
