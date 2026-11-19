use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientIdentity {
    pub machine_id: String,
    pub desky_id: Option<String>,
    #[serde(default)]
    pub recent_sessions: Vec<String>,
}

fn get_identity_path() -> PathBuf {
    let dir = crate::platform::get_identity_dir();
    dir.join("identity.json")
}

pub fn load_or_create_identity() -> ClientIdentity {
    let path = get_identity_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(id) = serde_json::from_str::<ClientIdentity>(&content) {
                return id;
            }
        }
    }

    // Platform-delegated machine GUID
    let machine_id = crate::platform::get_machine_id();

    let ident = ClientIdentity {
        machine_id,
        desky_id: None,
        recent_sessions: Vec::new(),
    };

    if let Ok(bytes) = serde_json::to_string_pretty(&ident) {
        let _ = fs::write(&path, bytes);
        info!("Created new persistent client identity at {:?}", path);
    }

    ident
}

pub fn save_assigned_desky_id(desky_id: &str) {
    let path = get_identity_path();
    let mut ident = load_or_create_identity();
    ident.desky_id = Some(desky_id.to_string());
    if let Ok(bytes) = serde_json::to_string_pretty(&ident) {
        let _ = fs::write(&path, bytes);
        info!("Saved assigned permanent Desky ID '{}' to {:?}", desky_id, path);
    }
}

pub fn save_recent_session(target_id: &str) {
    let digits: String = target_id.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() != 9 {
        return;
    }
    let display = format!("{} {} {}", &digits[0..3], &digits[3..6], &digits[6..9]);
    let path = get_identity_path();
    let mut ident = load_or_create_identity();
    super::address_book::update_last_connected(&display);
    ident.recent_sessions.retain(|id| id != &display && id != &digits);
    ident.recent_sessions.insert(0, display);
    if ident.recent_sessions.len() > 12 {
        ident.recent_sessions.truncate(12);
    }
    if let Ok(bytes) = serde_json::to_string_pretty(&ident) {
        let _ = fs::write(&path, bytes);
    }
}

pub fn get_recent_sessions() -> Vec<String> {
    load_or_create_identity().recent_sessions
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppPreferences {
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
    #[serde(default)]
    pub start_on_boot: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            close_to_tray: true,
            start_on_boot: false,
        }
    }
}

fn get_preferences_path() -> PathBuf {
    crate::platform::get_identity_dir().join("preferences.json")
}

pub fn load_preferences() -> AppPreferences {
    let path = get_preferences_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(prefs) = serde_json::from_str::<AppPreferences>(&content) {
                return prefs;
            }
        }
    }
    AppPreferences::default()
}

pub fn save_preferences(prefs: &AppPreferences) {
    let path = get_preferences_path();
    if let Ok(bytes) = serde_json::to_string_pretty(prefs) {
        let _ = fs::write(&path, bytes);
    }
}

pub fn is_started_in_tray() -> bool {
    std::env::args().any(|a| a == "--tray" || a == "--minimized" || a == "--background")
}
