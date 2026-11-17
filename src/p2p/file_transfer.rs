//! Peer-to-Peer Chunked File Transfer Subsystem
//!
//! Manages reliable bidirectional file transfer with progress tracking,
//! chunk micro-pacing, and selective ARQ NACK retransmission.

#![allow(dead_code)]

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::Notify;
use tracing::info;

use crate::protocol::opcodes::{MAGIC_DSKY, PKT_FILE_CHUNK, PKT_FILE_FINISH, PKT_FILE_START};

static OUTGOING_FILE_PACKETS: OnceLock<Mutex<VecDeque<Vec<u8>>>> = OnceLock::new();
static FILE_NOTIFY: OnceLock<Arc<Notify>> = OnceLock::new();
static ACTIVE_OUTGOING_FILE: OnceLock<Mutex<Option<(String, Vec<u8>)>>> = OnceLock::new();

#[derive(Clone, Default)]
pub struct TransferProgress {
    pub is_sending: bool,
    pub file_name: String,
    pub current_bytes: u64,
    pub total_bytes: u64,
    pub percentage: u32,
    pub is_complete: bool,
}

static ACTIVE_TRANSFER_PROGRESS: OnceLock<Mutex<TransferProgress>> = OnceLock::new();

pub fn get_transfer_progress() -> TransferProgress {
    let store = ACTIVE_TRANSFER_PROGRESS.get_or_init(|| Mutex::new(TransferProgress::default()));
    if let Ok(guard) = store.lock() {
        guard.clone()
    } else {
        TransferProgress::default()
    }
}

pub fn update_transfer_progress(is_sending: bool, file_name: &str, current: u64, total: u64, is_complete: bool) {
    let pct = if total > 0 {
        ((current as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as u32
    } else {
        0
    };
    let store = ACTIVE_TRANSFER_PROGRESS.get_or_init(|| Mutex::new(TransferProgress::default()));
    if let Ok(mut guard) = store.lock() {
        *guard = TransferProgress {
            is_sending,
            file_name: file_name.to_string(),
            current_bytes: current,
            total_bytes: total,
            percentage: pct,
            is_complete,
        };
    }
}

pub fn send_file_transfer(path: std::path::PathBuf) {
    std::thread::spawn(move || {
        if let Ok(data) = std::fs::read(&path) {
            let file_name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "transferred_file.dat".into());
            let name_bytes = file_name.as_bytes();
            let total_size = data.len() as u64;

            crate::platform::ui::trigger_transfer_progress_window();

            update_transfer_progress(true, &file_name, 0, total_size, false);

            let cache_store = ACTIVE_OUTGOING_FILE.get_or_init(|| Mutex::new(None));
            if let Ok(mut cg) = cache_store.lock() {
                *cg = Some((file_name.clone(), data.clone()));
            }

            let store = OUTGOING_FILE_PACKETS.get_or_init(|| Mutex::new(VecDeque::new()));
            let notify = FILE_NOTIFY.get_or_init(|| Arc::new(Notify::new()));

            // 1. Send PKT_FILE_START: [MAGIC_DSKY, PKT_FILE_START, 8B size, 2B name_len, name_bytes]
            let mut start_pkt = Vec::with_capacity(5 + 8 + 2 + name_bytes.len());
            start_pkt.extend_from_slice(MAGIC_DSKY);
            start_pkt.push(PKT_FILE_START);
            start_pkt.extend_from_slice(&total_size.to_le_bytes());
            start_pkt.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            start_pkt.extend_from_slice(name_bytes);

            if let Ok(mut q) = store.lock() {
                q.push_back(start_pkt);
            }
            notify.notify_waiters();

            // 2. Send chunks (1200 bytes each) with smooth micro-pacing
            let chunk_size = 1200;
            let mut chunk_idx: u32 = 0;
            let mut sent_bytes: u64 = 0;
            for chunk in data.chunks(chunk_size) {
                let mut chunk_pkt = Vec::with_capacity(5 + 4 + chunk.len());
                chunk_pkt.extend_from_slice(MAGIC_DSKY);
                chunk_pkt.push(PKT_FILE_CHUNK);
                chunk_pkt.extend_from_slice(&chunk_idx.to_le_bytes());
                chunk_pkt.extend_from_slice(chunk);

                if let Ok(mut q) = store.lock() {
                    q.push_back(chunk_pkt);
                }
                notify.notify_waiters();

                sent_bytes += chunk.len() as u64;
                chunk_idx += 1;

                if chunk_idx % 24 == 0 {
                    update_transfer_progress(true, &file_name, sent_bytes, total_size, false);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }

            // 3. Send PKT_FILE_FINISH
            let mut finish_pkt = Vec::with_capacity(5);
            finish_pkt.extend_from_slice(MAGIC_DSKY);
            finish_pkt.push(PKT_FILE_FINISH);

            if let Ok(mut q) = store.lock() {
                q.push_back(finish_pkt);
            }
            notify.notify_waiters();

            update_transfer_progress(true, &file_name, total_size, total_size, true);
            info!("Transmitting file '{}' ({} bytes, {} chunks)...", file_name, total_size, chunk_idx);
        }
    });
}

pub fn pop_outgoing_file_packet() -> Option<Vec<u8>> {
    let store = OUTGOING_FILE_PACKETS.get_or_init(|| Mutex::new(VecDeque::new()));
    if let Ok(mut q) = store.lock() {
        return q.pop_front();
    }
    None
}

pub fn file_notify() -> &'static Arc<Notify> {
    FILE_NOTIFY.get_or_init(|| Arc::new(Notify::new()))
}

pub fn handle_outgoing_file_nack(missing_chunks: &[u32]) {
    let cache_store = ACTIVE_OUTGOING_FILE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache_store.lock() {
        if let Some((_file_name, data)) = guard.as_ref() {
            let store = OUTGOING_FILE_PACKETS.get_or_init(|| Mutex::new(VecDeque::new()));
            let notify = FILE_NOTIFY.get_or_init(|| Arc::new(Notify::new()));
            let chunk_size = 1200;

            if let Ok(mut q) = store.lock() {
                for &idx in missing_chunks {
                    let start = (idx as usize) * chunk_size;
                    if start < data.len() {
                        let end = (start + chunk_size).min(data.len());
                        let chunk = &data[start..end];
                        let mut chunk_pkt = Vec::with_capacity(5 + 4 + chunk.len());
                        chunk_pkt.extend_from_slice(MAGIC_DSKY);
                        chunk_pkt.push(PKT_FILE_CHUNK);
                        chunk_pkt.extend_from_slice(&idx.to_le_bytes());
                        chunk_pkt.extend_from_slice(chunk);
                        q.push_back(chunk_pkt);
                    }
                }
                let mut finish_pkt = Vec::with_capacity(5);
                finish_pkt.extend_from_slice(MAGIC_DSKY);
                finish_pkt.push(PKT_FILE_FINISH);
                q.push_back(finish_pkt);
            }
            notify.notify_waiters();
            tracing::info!("🔄 Selective ARQ: Re-transmitted {} missing file chunks", missing_chunks.len());
        }
    }
}

#[derive(Default)]
pub struct IncomingFileTransfer {
    pub file_name: String,
    pub total_size: u64,
    pub received_bytes: u64,
    pub handle: Option<std::fs::File>,
    pub save_path: std::path::PathBuf,
    pub received_chunks: HashSet<u32>,
    pub total_chunks: u32,
}

static INCOMING_FILE: OnceLock<Mutex<Option<IncomingFileTransfer>>> = OnceLock::new();

pub fn handle_incoming_file_start(total_size: u64, file_name: &str) {
    let mut safe_name = file_name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_' || *c == ' ' || *c == '(' || *c == ')' || *c == '[' || *c == ']')
        .collect::<String>();
    if safe_name.is_empty() {
        safe_name = "received_file.dat".into();
    }

    let dest_dir = crate::platform::get_desktop_dir();
    let _ = std::fs::create_dir_all(&dest_dir);
    let target_path = dest_dir.join(&safe_name);
    let total_chunks = ((total_size + 1199) / 1200) as u32;

    if let Ok(file) = std::fs::File::create(&target_path) {
        let _ = file.set_len(total_size);
        let store = INCOMING_FILE.get_or_init(|| Mutex::new(None));
        if let Ok(mut guard) = store.lock() {
            *guard = Some(IncomingFileTransfer {
                file_name: safe_name.clone(),
                total_size,
                received_bytes: 0,
                handle: Some(file),
                save_path: target_path.clone(),
                received_chunks: HashSet::with_capacity(total_chunks as usize),
                total_chunks,
            });
        }
        crate::platform::ui::trigger_transfer_progress_window();

        update_transfer_progress(false, &safe_name, 0, total_size, false);
        info!("📁 Incoming file '{}' ({} KB, {} chunks) starting -> {:?}", safe_name, total_size / 1024, total_chunks, target_path);
    }
}

pub fn handle_incoming_file_chunk(chunk_idx: u32, data: &[u8]) {
    use std::io::{Seek, SeekFrom, Write};
    let store = INCOMING_FILE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = store.lock() {
        if let Some(transfer) = guard.as_mut() {
            if !transfer.received_chunks.contains(&chunk_idx) {
                transfer.received_chunks.insert(chunk_idx);
                transfer.received_bytes += data.len() as u64;
                if let Some(file) = transfer.handle.as_mut() {
                    let offset = (chunk_idx as u64) * 1200;
                    let _ = file.seek(SeekFrom::Start(offset));
                    let _ = file.write_all(data);
                }
                update_transfer_progress(false, &transfer.file_name, transfer.received_bytes, transfer.total_size, false);
            }
        }
    }
}

pub fn handle_incoming_file_finish() -> Option<Vec<u32>> {
    use std::io::Write;
    let store = INCOMING_FILE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = store.lock() {
        if let Some(transfer) = guard.as_mut() {
            if (transfer.received_chunks.len() as u32) < transfer.total_chunks {
                let mut missing = Vec::new();
                for idx in 0..transfer.total_chunks {
                    if !transfer.received_chunks.contains(&idx) {
                        missing.push(idx);
                        if missing.len() >= 64 {
                            break;
                        }
                    }
                }
                if !missing.is_empty() {
                    tracing::warn!("⚠️ File transfer missing {} chunks, requesting Selective NACK retransmission...", missing.len());
                    return Some(missing);
                }
            }
        }

        if let Some(mut transfer) = guard.take() {
            if let Some(mut file) = transfer.handle.take() {
                let _ = file.flush();
            }
            update_transfer_progress(false, &transfer.file_name, transfer.total_size, transfer.total_size, true);

            crate::platform::notify_file_received(&transfer.save_path);
            super::clipboard::write_clipboard_files(&[transfer.save_path.clone()]);

            info!("✅ File transfer complete: '{}' ({} KB) saved at {:?}", transfer.file_name, transfer.total_size / 1024, transfer.save_path);
        }
    }
    None
}
