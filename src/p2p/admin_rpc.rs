//! Remote Administration RPC Protocol & Chunked Messaging
//!
//! Encapsulates administrative RPC queries (Process list, Services, System Stats,
//! Session termination) with chunking and packet reassembly.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tokio::sync::Notify;

use crate::protocol::messages::{AdminReq, AdminResp};
use crate::protocol::opcodes::{MAGIC_DSKY, PKT_ADMIN_REQ};

struct AdminReassemblyBuffer {
    total_chunks: u16,
    received_count: u16,
    chunks: Vec<Option<Vec<u8>>>,
    created_at: Instant,
}

static INCOMING_ADMIN_REQ_REASSEMBLY: OnceLock<Mutex<HashMap<u32, AdminReassemblyBuffer>>> = OnceLock::new();
static INCOMING_ADMIN_RESP_REASSEMBLY: OnceLock<Mutex<HashMap<u32, AdminReassemblyBuffer>>> = OnceLock::new();

static NEXT_ADMIN_MSG_ID: AtomicU32 = AtomicU32::new(1);
static OUTGOING_ADMIN: OnceLock<Mutex<Vec<Vec<u8>>>> = OnceLock::new();
static ADMIN_NOTIFY: OnceLock<Arc<Notify>> = OnceLock::new();
static LATEST_ADMIN_RESP: OnceLock<Mutex<Option<AdminResp>>> = OnceLock::new();

pub fn create_chunked_admin_packets(pkt_type: u8, msg_id: u32, payload: &[u8]) -> Vec<Vec<u8>> {
    let chunk_size = 1024;
    let total_chunks = ((payload.len() + chunk_size - 1) / chunk_size).max(1) as u16;
    let mut packets = Vec::with_capacity(total_chunks as usize);

    for chunk_idx in 0..total_chunks {
        let start = (chunk_idx as usize) * chunk_size;
        let end = ((chunk_idx as usize + 1) * chunk_size).min(payload.len());
        let chunk_data = if start < payload.len() { &payload[start..end] } else { &[] };

        let mut pkt = Vec::with_capacity(13 + chunk_data.len());
        pkt.extend_from_slice(MAGIC_DSKY);
        pkt.push(pkt_type);
        pkt.extend_from_slice(&msg_id.to_le_bytes());
        pkt.extend_from_slice(&chunk_idx.to_le_bytes());
        pkt.extend_from_slice(&total_chunks.to_le_bytes());
        pkt.extend_from_slice(chunk_data);
        packets.push(pkt);
    }
    packets
}

pub fn send_admin_request(req: AdminReq) {
    if let Ok(bytes) = serde_json::to_vec(&req) {
        let msg_id = NEXT_ADMIN_MSG_ID.fetch_add(1, Ordering::Relaxed);
        let pkts = create_chunked_admin_packets(PKT_ADMIN_REQ, msg_id, &bytes);
        let store = OUTGOING_ADMIN.get_or_init(|| Mutex::new(Vec::new()));
        if let Ok(mut q) = store.lock() {
            q.extend(pkts);
        }
        let notify = ADMIN_NOTIFY.get_or_init(|| Arc::new(Notify::new()));
        notify.notify_waiters();
    }
}

pub fn pop_outgoing_admin_packet() -> Option<Vec<u8>> {
    let store = OUTGOING_ADMIN.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut q) = store.lock() {
        if !q.is_empty() {
            return Some(q.remove(0));
        }
    }
    None
}

pub fn admin_notify() -> &'static Arc<Notify> {
    ADMIN_NOTIFY.get_or_init(|| Arc::new(Notify::new()))
}

pub fn store_admin_response(resp: AdminResp) {
    let slot = LATEST_ADMIN_RESP.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = slot.lock() {
        *g = Some(resp);
    }
}

pub fn take_admin_response() -> Option<AdminResp> {
    let slot = LATEST_ADMIN_RESP.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = slot.lock() {
        g.take()
    } else {
        None
    }
}

pub fn feed_admin_req_chunk(msg_id: u32, chunk_idx: u16, total_chunks: u16, data: &[u8]) -> Option<AdminReq> {
    feed_chunk(&INCOMING_ADMIN_REQ_REASSEMBLY, msg_id, chunk_idx, total_chunks, data)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

pub fn feed_admin_resp_chunk(msg_id: u32, chunk_idx: u16, total_chunks: u16, data: &[u8]) -> Option<AdminResp> {
    feed_chunk(&INCOMING_ADMIN_RESP_REASSEMBLY, msg_id, chunk_idx, total_chunks, data)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn feed_chunk(
    store: &'static OnceLock<Mutex<HashMap<u32, AdminReassemblyBuffer>>>,
    msg_id: u32,
    chunk_idx: u16,
    total_chunks: u16,
    data: &[u8],
) -> Option<Vec<u8>> {
    let map_lock = store.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = map_lock.lock().ok()?;

    map.retain(|_, b| b.created_at.elapsed().as_secs() < 15);

    let buf = map.entry(msg_id).or_insert_with(|| AdminReassemblyBuffer {
        total_chunks,
        received_count: 0,
        chunks: vec![None; total_chunks as usize],
        created_at: Instant::now(),
    });

    if (chunk_idx as usize) < buf.chunks.len() && buf.chunks[chunk_idx as usize].is_none() {
        buf.chunks[chunk_idx as usize] = Some(data.to_vec());
        buf.received_count += 1;
    }

    if buf.received_count >= buf.total_chunks {
        let mut full = Vec::new();
        for c in buf.chunks.iter().flatten() {
            full.extend_from_slice(c);
        }
        map.remove(&msg_id);
        Some(full)
    } else {
        None
    }
}
