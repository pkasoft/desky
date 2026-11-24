//! Two-Way Real-Time Voice Intercom Network Queue
//!
//! Manages low-latency audio capture streaming and packet queuing between peers.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::Notify;
use crate::codec::opus_audio::{build_opus_packet, OpusAudioEncoder, OPUS_SAMPLE_RATE};
use crate::protocol::opcodes::{MAGIC_DSKY, PKT_VOICE_CHUNK, PKT_VOICE_OPUS};

static OUTGOING_VOICE_PACKETS: OnceLock<Mutex<VecDeque<Vec<u8>>>> = OnceLock::new();
static VOICE_NOTIFY: OnceLock<Arc<Notify>> = OnceLock::new();
static VOICE_OPUS_ENC: OnceLock<Mutex<Option<OpusAudioEncoder>>> = OnceLock::new();
static VOICE_SEQ: AtomicU16 = AtomicU16::new(0);

pub fn voice_notify() -> &'static Arc<Notify> {
    VOICE_NOTIFY.get_or_init(|| Arc::new(Notify::new()))
}

fn voice_store() -> &'static Mutex<VecDeque<Vec<u8>>> {
    OUTGOING_VOICE_PACKETS.get_or_init(|| Mutex::new(VecDeque::with_capacity(32)))
}

fn enqueue_voice_packet(pkt: Vec<u8>) {
    if let Ok(mut q) = voice_store().lock() {
        if q.len() >= 64 {
            q.pop_front();
        }
        q.push_back(pkt);
    }
    voice_notify().notify_one();
}

pub fn push_outgoing_voice_chunk(channels: u8, sample_rate: u32, pcm: &[u8]) {
    let enc_store = VOICE_OPUS_ENC.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = enc_store.lock() {
        let need_init = guard
            .as_ref()
            .map(|e| !e.matches_input(sample_rate, channels as u16))
            .unwrap_or(true);
        if need_init {
            *guard = OpusAudioEncoder::new_voice(sample_rate, channels as u16).ok();
        }
        if let Some(ref mut enc) = *guard {
            let out_ch = enc.out_channels as u8;
            for opus_frame in enc.encode_pcm_bytes(pcm) {
                let seq = VOICE_SEQ.fetch_add(1, Ordering::Relaxed);
                let pkt = build_opus_packet(
                    PKT_VOICE_OPUS,
                    out_ch,
                    seq,
                    OPUS_SAMPLE_RATE,
                    &opus_frame,
                );
                enqueue_voice_packet(pkt);
            }
            return;
        }
    }

    let mut pkt = Vec::with_capacity(10 + pcm.len());
    pkt.extend_from_slice(MAGIC_DSKY);
    pkt.push(PKT_VOICE_CHUNK);
    pkt.push(channels);
    pkt.extend_from_slice(&sample_rate.to_le_bytes());
    pkt.extend_from_slice(pcm);
    enqueue_voice_packet(pkt);
}

pub fn pop_outgoing_voice_packet() -> Option<Vec<u8>> {
    voice_store().lock().ok().and_then(|mut q| q.pop_front())
}

pub fn clear_voice_queue() {
    if let Ok(mut q) = voice_store().lock() {
        q.clear();
    }
}
