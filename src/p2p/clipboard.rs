//! Bidirectional Real-Time Clipboard Synchronization Network Queue
//!
//! Synchronizes clipboard text changes between Host and Client peers with
//! FNV-1a hash deduplication and UDP payload protection.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::Notify;
use tracing::info;

use crate::protocol::opcodes::{MAGIC_DSKY, PKT_CLIPBOARD};
use super::state::{
    host_std_socket_store, host_subscribers_store, resolve_server_addr,
    wrap_relay_packet, AuthState,
};

pub static LAST_SYNCED_CLIPBOARD_HASH: AtomicU64 = AtomicU64::new(0);

static OUTGOING_CLIPBOARD: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static CLIPBOARD_NOTIFY: OnceLock<Arc<Notify>> = OnceLock::new();

pub fn compute_clipboard_hash(text: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in text.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub fn send_remote_clipboard(text: &str) {
    if text.is_empty() {
        return;
    }
    // Protect against UDP oversized datagrams (>60KB)
    let safe_text = if text.len() > 60_000 {
        let mut end = 60_000;
        while !text.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &text[..end]
    } else {
        text
    };

    let hash = compute_clipboard_hash(safe_text);
    let last = LAST_SYNCED_CLIPBOARD_HASH.load(Ordering::Relaxed);
    if hash == last {
        return;
    }
    LAST_SYNCED_CLIPBOARD_HASH.store(hash, Ordering::Relaxed);

    // 1. Queue for client session (Client -> Remote Host)
    let store = OUTGOING_CLIPBOARD.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut q) = store.lock() {
        q.push(safe_text.to_string());
    }
    let notify = CLIPBOARD_NOTIFY.get_or_init(|| Arc::new(Notify::new()));
    notify.notify_waiters();

    // 2. Broadcast immediately if this machine is a Host with active client subscribers (Host -> Remote Client)
    let mut host_clients = Vec::new();
    if let Ok(map) = host_subscribers_store().lock() {
        for sub in map.values() {
            if sub.auth_state == AuthState::Approved {
                host_clients.push((sub.addr, sub.lan_addr, sub.relay_id.clone(), sub.is_direct));
            }
        }
    }
    if !host_clients.is_empty() {
        if let Ok(socket_opt) = host_std_socket_store().lock() {
            if let Some(sock) = socket_opt.as_ref() {
                let text_bytes = safe_text.as_bytes();
                let mut pkt = Vec::with_capacity(5 + text_bytes.len());
                pkt.extend_from_slice(MAGIC_DSKY);
                pkt.push(PKT_CLIPBOARD);
                pkt.extend_from_slice(text_bytes);

                let server_addr = resolve_server_addr();
                for (addr, lan_addr, relay_id, is_direct) in host_clients {
                    if is_direct || addr != server_addr {
                        let _ = sock.send_to(&pkt, addr);
                        if let Some(l_addr) = lan_addr {
                            if l_addr != addr {
                                let _ = sock.send_to(&pkt, l_addr);
                            }
                        }
                    }
                    if !relay_id.is_empty() {
                        let relay = wrap_relay_packet(&relay_id, &pkt);
                        let _ = sock.send_to(&relay, server_addr);
                    }
                }
                info!(
                    "📋 [HOST] Broadcasted clipboard text ({} chars) to client(s)",
                    safe_text.chars().count()
                );
            }
        }
    }
}

pub fn pop_outgoing_clipboard() -> Option<String> {
    let store = OUTGOING_CLIPBOARD.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut q) = store.lock() {
        if !q.is_empty() {
            return Some(q.remove(0));
        }
    }
    None
}

pub fn clipboard_notify() -> &'static Arc<Notify> {
    CLIPBOARD_NOTIFY.get_or_init(|| Arc::new(Notify::new()))
}

pub fn write_clipboard_files(paths: &[std::path::PathBuf]) {
    crate::platform::write_clipboard_files(paths);
}

pub fn write_clipboard_text(text: &str) {
    crate::platform::write_clipboard_text(text);
}

pub fn read_clipboard_text() -> Option<String> {
    crate::platform::read_clipboard_text()
}

pub fn read_clipboard_files() -> Vec<std::path::PathBuf> {
    crate::platform::read_clipboard_files()
}
