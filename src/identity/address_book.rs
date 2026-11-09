//! Saved Address Book & Machine Nickname Management
//! Stores remembered remote workstations with friendly aliases, group tags,
//! and optional encrypted/saved credentials for instant 1-click connection.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddressBookEntry {
    pub id: String,                    // 9-digit Desky ID (e.g. "195 483 733")
    pub alias: String,                 // Friendly name (e.g. "Office Workstation")
    pub group: String,                 // Category/Tag (e.g. "Work", "Home", "Servers")
    #[serde(default)]
    pub saved_password: Option<String>,
    pub last_connected: u64,           // Unix timestamp
}

static ADDRESS_BOOK_CACHE: OnceLock<Mutex<Vec<AddressBookEntry>>> = OnceLock::new();

fn get_address_book_path() -> PathBuf {
    let dir = crate::platform::get_identity_dir();
    dir.join("address_book.json")
}

fn cache_lock() -> &'static Mutex<Vec<AddressBookEntry>> {
    ADDRESS_BOOK_CACHE.get_or_init(|| Mutex::new(load_from_disk()))
}

fn load_from_disk() -> Vec<AddressBookEntry> {
    let path = get_address_book_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(entries) = serde_json::from_str::<Vec<AddressBookEntry>>(&content) {
                return entries;
            }
        }
    }
    Vec::new()
}

pub fn load_address_book() -> Vec<AddressBookEntry> {
    cache_lock().lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn save_address_book(entries: &[AddressBookEntry]) {
    let path = get_address_book_path();
    if let Ok(mut g) = cache_lock().lock() {
        *g = entries.to_vec();
    }
    if let Ok(bytes) = serde_json::to_string_pretty(entries) {
        let _ = fs::write(&path, bytes);
        info!("Saved {} address book entries to {:?}", entries.len(), path);
    }
}

pub fn add_or_update_contact(id: &str, alias: &str, group: &str, password: Option<&str>) {
    let norm_id = crate::p2p::normalize_id(id);
    if norm_id.len() != 9 {
        return;
    }
    let formatted_id = format!("{} {} {}", &norm_id[0..3], &norm_id[3..6], &norm_id[6..9]);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut entries = load_address_book();
    if let Some(pos) = entries.iter().position(|e| crate::p2p::normalize_id(&e.id) == norm_id) {
        entries[pos].alias = alias.to_string();
        entries[pos].group = group.to_string();
        if let Some(p) = password {
            if !p.is_empty() {
                entries[pos].saved_password = Some(p.to_string());
            }
        }
        entries[pos].last_connected = now;
    } else {
        entries.push(AddressBookEntry {
            id: formatted_id,
            alias: alias.to_string(),
            group: if group.is_empty() { "Default".into() } else { group.to_string() },
            saved_password: password.filter(|p| !p.is_empty()).map(|p| p.to_string()),
            last_connected: now,
        });
    }
    save_address_book(&entries);
}

pub fn delete_contact(id: &str) {
    let norm_id = crate::p2p::normalize_id(id);
    let mut entries = load_address_book();
    entries.retain(|e| crate::p2p::normalize_id(&e.id) != norm_id);
    save_address_book(&entries);
}

pub fn find_contact(id: &str) -> Option<AddressBookEntry> {
    let norm_id = crate::p2p::normalize_id(id);
    load_address_book().into_iter().find(|e| crate::p2p::normalize_id(&e.id) == norm_id)
}

pub fn update_last_connected(id: &str) {
    let norm_id = crate::p2p::normalize_id(id);
    let mut entries = load_address_book();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if let Some(entry) = entries.iter_mut().find(|e| crate::p2p::normalize_id(&e.id) == norm_id) {
        entry.last_connected = now;
        save_address_book(&entries);
    }
}
