pub mod address_book;
pub mod storage;
pub mod unattended;

pub use address_book::*;
pub use storage::{
    get_recent_sessions, is_started_in_tray, load_or_create_identity, load_preferences,
    save_assigned_desky_id, save_preferences, save_recent_session,
};
pub use unattended::*;
