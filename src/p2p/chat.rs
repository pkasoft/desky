//! In-Session Peer-to-Peer Encrypted Chat Engine
//! Manages bidirectional text messages, unread notification badges, and chat history.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct ChatMessage {
    #[allow(dead_code)]
    pub id: u64,
    pub sender: String,
    pub is_me: bool,
    pub text: String,
    pub timestamp_str: String,
}

static CHAT_MESSAGES: OnceLock<Mutex<Vec<ChatMessage>>> = OnceLock::new();
static UNREAD_COUNT: AtomicU32 = AtomicU32::new(0);
static CHAT_OPEN: AtomicBool = AtomicBool::new(false);
static OUTGOING_CHAT_QUEUE: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn chat_store() -> &'static Mutex<Vec<ChatMessage>> {
    CHAT_MESSAGES.get_or_init(|| Mutex::new(Vec::new()))
}

fn outgoing_store() -> &'static Mutex<Vec<String>> {
    OUTGOING_CHAT_QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn is_chat_open() -> bool {
    CHAT_OPEN.load(Ordering::Relaxed)
}

pub fn set_chat_open(open: bool) {
    CHAT_OPEN.store(open, Ordering::Relaxed);
    if open {
        UNREAD_COUNT.store(0, Ordering::Relaxed);
    }
}

pub fn toggle_chat() -> bool {
    let next = !CHAT_OPEN.load(Ordering::Relaxed);
    set_chat_open(next);
    next
}

pub fn get_unread_count() -> u32 {
    UNREAD_COUNT.load(Ordering::Relaxed)
}

pub fn mark_chat_read() {
    UNREAD_COUNT.store(0, Ordering::Relaxed);
}

pub fn get_chat_messages() -> Vec<ChatMessage> {
    chat_store().lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn clear_chat_history() {
    if let Ok(mut g) = chat_store().lock() {
        g.clear();
    }
    UNREAD_COUNT.store(0, Ordering::Relaxed);
}

fn current_time_str() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let hours = (now / 3600) % 24;
    let mins = (now / 60) % 60;
    format!("{:02}:{:02}", hours, mins)
}

/// Enqueues a message to be sent over the P2P connection and adds to local history
pub fn send_chat_message(text: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let msg = ChatMessage {
        id,
        sender: "You".to_string(),
        is_me: true,
        text: trimmed.to_string(),
        timestamp_str: current_time_str(),
    };

    if let Ok(mut g) = chat_store().lock() {
        g.push(msg);
    }
    if let Ok(mut out) = outgoing_store().lock() {
        out.push(trimmed.to_string());
    }
}

/// Receives an incoming remote chat message
pub fn receive_chat_message(sender_label: &str, text: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let msg = ChatMessage {
        id,
        sender: sender_label.to_string(),
        is_me: false,
        text: trimmed.to_string(),
        timestamp_str: current_time_str(),
    };

    if let Ok(mut g) = chat_store().lock() {
        g.push(msg);
    }

    if !CHAT_OPEN.load(Ordering::Relaxed) {
        UNREAD_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

/// Pulls queued outgoing messages for transmission across the network
pub fn pull_outgoing_messages() -> Vec<String> {
    if let Ok(mut out) = outgoing_store().lock() {
        out.drain(..).collect()
    } else {
        Vec::new()
    }
}
