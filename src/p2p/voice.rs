//! Two-Way Real-Time Voice Intercom Network Queue
//!
//! Manages low-latency audio capture streaming and packet queuing between peers.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::Notify;
use crate::protocol::opcodes::{MAGIC_DSKY, PKT_VOICE_CHUNK};

static OUTGOING_VOICE_PACKETS: OnceLock<Mutex<VecDeque<Vec<u8>>>> = OnceLock::new();
static VOICE_NOTIFY: OnceLock<Arc<Notify>> = OnceLock::new();

pub fn voice_notify() -> &'static Arc<Notify> {
    VOICE_NOTIFY.get_or_init(|| Arc::new(Notify::new()))
}

fn voice_store() -> &'static Mutex<VecDeque<Vec<u8>>> {
    OUTGOING_VOICE_PACKETS.get_or_init(|| Mutex::new(VecDeque::with_capacity(32)))
}

pub fn push_outgoing_voice_chunk(channels: u8, sample_rate: u32, pcm: &[u8]) {
    let mut pkt = Vec::with_capacity(10 + pcm.len());
    pkt.extend_from_slice(MAGIC_DSKY);
    pkt.push(PKT_VOICE_CHUNK);
    pkt.push(channels);
    pkt.extend_from_slice(&sample_rate.to_le_bytes());
    pkt.extend_from_slice(pcm);
    if let Ok(mut q) = voice_store().lock() {
        if q.len() >= 64 {
            q.pop_front();
        }
        q.push_back(pkt);
    }
    voice_notify().notify_one();
}

pub fn pop_outgoing_voice_packet() -> Option<Vec<u8>> {
    voice_store().lock().ok().and_then(|mut q| q.pop_front())
}

pub fn clear_voice_queue() {
    if let Ok(mut q) = voice_store().lock() {
        q.clear();
    }
}
