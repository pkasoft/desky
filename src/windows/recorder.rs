#![allow(dead_code)]
//! Fast, Zero-Overhead AVI / Motion-JPEG Session Recorder
//! Direct container muxing for incoming remote JPEG frames with 0% CPU re-encoding overhead.

use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::info;

static IS_RECORDING: AtomicBool = AtomicBool::new(false);
static RECORD_FRAMES: AtomicU32 = AtomicU32::new(0);
static RECORD_START_TIME: AtomicU64 = AtomicU64::new(0);
static RECORDER_SENDER: Mutex<Option<Sender<Vec<u8>>>> = Mutex::new(None);
static LAST_SAVED_INFO: Mutex<Option<(PathBuf, u64, u32, Duration)>> = Mutex::new(None);

struct AviIndexEntry {
    fcc: [u8; 4],
    flags: u32,
    offset: u32,
    size: u32,
}

pub fn is_recording() -> bool {
    IS_RECORDING.load(Ordering::Relaxed)
}

pub fn recording_elapsed() -> Duration {
    if !is_recording() {
        return Duration::ZERO;
    }
    let start_ms = RECORD_START_TIME.load(Ordering::Relaxed);
    if start_ms == 0 {
        return Duration::ZERO;
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    Duration::from_millis(now_ms.saturating_sub(start_ms))
}

pub fn recording_frames_count() -> u32 {
    RECORD_FRAMES.load(Ordering::Relaxed)
}

/// Start session recording to an AVI video file
pub fn start_recording(width: u32, height: u32, fps: u32) -> Result<PathBuf, String> {
    if IS_RECORDING.swap(true, Ordering::SeqCst) {
        return Err("Recording is already in progress".into());
    }

    let save_dir = dirs_fallback_videos();
    let _ = create_dir_all(&save_dir);

    let timestamp = chrono_timestamp();
    let file_name = format!("Desky_Recording_{}.avi", timestamp);
    let file_path = save_dir.join(&file_name);

    let mut file = match OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&file_path)
    {
        Ok(f) => f,
        Err(e) => {
            IS_RECORDING.store(false, Ordering::SeqCst);
            return Err(format!("Failed to create video file: {}", e));
        }
    };

    let target_fps = fps.clamp(15, 120);
    let (tx, rx) = channel::<Vec<u8>>();

    if let Ok(mut guard) = RECORDER_SENDER.lock() {
        *guard = Some(tx);
    }

    let start_epoch_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    RECORD_START_TIME.store(start_epoch_ms, Ordering::Relaxed);
    RECORD_FRAMES.store(0, Ordering::Relaxed);

    let path_clone = file_path.clone();
    std::thread::spawn(move || {
        run_recorder_writer(file_path, &mut file, rx, width, height, target_fps);
    });

    info!("Session recording started: {}", path_clone.display());
    Ok(path_clone)
}

/// Push incoming JPEG frame to the background AVI writer
pub fn push_frame(jpeg_bytes: &[u8]) {
    if !is_recording() || jpeg_bytes.is_empty() {
        return;
    }
    if let Ok(guard) = RECORDER_SENDER.lock() {
        if let Some(ref tx) = *guard {
            let _ = tx.send(jpeg_bytes.to_vec());
        }
    }
}

/// Stop the session recording and finalize the AVI video headers
pub fn stop_recording() -> Option<(PathBuf, u64, u32, Duration)> {
    if !IS_RECORDING.swap(false, Ordering::SeqCst) {
        return None;
    }

    if let Ok(mut guard) = RECORDER_SENDER.lock() {
        let _ = guard.take(); // Drops Sender, which signals writer thread loop to finish
    }

    // Wait briefly for background thread to write index and finalize headers
    let start_wait = Instant::now();
    while start_wait.elapsed() < Duration::from_millis(1500) {
        if let Ok(guard) = LAST_SAVED_INFO.lock() {
            if let Some(ref info) = *guard {
                return Some(info.clone());
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    if let Ok(guard) = LAST_SAVED_INFO.lock() {
        guard.clone()
    } else {
        None
    }
}

fn dirs_fallback_videos() -> PathBuf {
    if let Some(home) = dirs_home() {
        let v = home.join("Videos").join("Desky_Recordings");
        return v;
    }
    std::env::temp_dir().join("Desky_Recordings")
}

fn dirs_home() -> Option<PathBuf> {
    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        return Some(PathBuf::from(user_profile));
    }
    if let Ok(home) = std::env::var("HOME") {
        return Some(PathBuf::from(home));
    }
    None
}

fn chrono_timestamp() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Compact format: YYYYMMDD_HHMMSS
    let secs_per_day = 86400;
    let days = now / secs_per_day;
    let rem_secs = now % secs_per_day;
    let hours = rem_secs / 3600;
    let minutes = (rem_secs % 3600) / 60;
    let seconds = rem_secs % 60;
    format!("D{:05}_{:02}{:02}{:02}", days, hours, minutes, seconds)
}

fn run_recorder_writer(
    path: PathBuf,
    file: &mut File,
    rx: Receiver<Vec<u8>>,
    width: u32,
    height: u32,
    fps: u32,
) {
    let start_time = Instant::now();
    let mut frames_written = 0u32;
    let mut index_entries = Vec::<AviIndexEntry>::new();

    // 1. Placeholder headers (will be rewritten at the end)
    let header_size = 2048usize;
    let zero_buf = vec![0u8; header_size];
    let _ = file.write_all(&zero_buf);

    let movi_offset = header_size as u32;

    // 2. Receive and write frames
    while let Ok(frame_jpeg) = rx.recv() {
        if frame_jpeg.is_empty() {
            continue;
        }

        let current_pos = match file.seek(SeekFrom::Current(0)) {
            Ok(p) => p as u32,
            Err(_) => break,
        };

        let chunk_fourcc = *b"00dc";
        let chunk_len = frame_jpeg.len() as u32;

        let _ = file.write_all(&chunk_fourcc);
        let _ = file.write_all(&chunk_len.to_le_bytes());
        let _ = file.write_all(&frame_jpeg);

        // Pad to word boundary (2 bytes)
        if chunk_len % 2 != 0 {
            let _ = file.write_all(&[0u8]);
        }

        index_entries.push(AviIndexEntry {
            fcc: chunk_fourcc,
            flags: 0x10, // AVIIF_KEYFRAME
            offset: current_pos.saturating_sub(movi_offset),
            size: chunk_len,
        });

        frames_written += 1;
        RECORD_FRAMES.store(frames_written, Ordering::Relaxed);
    }

    // 3. Write 'idx1' index table
    let idx1_fourcc = *b"idx1";
    let idx1_len = (index_entries.len() * 16) as u32;
    let _ = file.write_all(&idx1_fourcc);
    let _ = file.write_all(&idx1_len.to_le_bytes());

    for entry in &index_entries {
        let _ = file.write_all(&entry.fcc);
        let _ = file.write_all(&entry.flags.to_le_bytes());
        let _ = file.write_all(&entry.offset.to_le_bytes());
        let _ = file.write_all(&entry.size.to_le_bytes());
    }

    let total_file_len = file.seek(SeekFrom::Current(0)).unwrap_or(0);
    let movi_data_len = (total_file_len as u32)
        .saturating_sub(movi_offset)
        .saturating_sub(idx1_len + 8);

    // 4. Seek back and write correct AVI headers
    let _ = file.seek(SeekFrom::Start(0));

    let mut hdr = Vec::with_capacity(header_size);
    write_avi_headers(
        &mut hdr,
        total_file_len as u32,
        movi_data_len,
        width,
        height,
        fps,
        frames_written,
    );

    let _ = file.write_all(&hdr);
    let _ = file.flush();

    let duration = start_time.elapsed();
    info!(
        "Session recording saved successfully: {} ({} frames, {} KB, {:.1}s)",
        path.display(),
        frames_written,
        total_file_len / 1024,
        duration.as_secs_f32()
    );

    if let Ok(mut guard) = LAST_SAVED_INFO.lock() {
        *guard = Some((path, total_file_len, frames_written, duration));
    }
}

fn write_avi_headers(
    buf: &mut Vec<u8>,
    total_len: u32,
    movi_data_len: u32,
    width: u32,
    height: u32,
    fps: u32,
    total_frames: u32,
) {
    let riff_size = total_len.saturating_sub(8);
    let us_per_frame = if fps > 0 { 1_000_000 / fps } else { 33_333 };

    // RIFF 'AVI '
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&riff_size.to_le_bytes());
    buf.extend_from_slice(b"AVI ");

    // LIST 'hdrl'
    let hdrl_content_start = buf.len() + 8;
    buf.extend_from_slice(b"LIST");
    buf.extend_from_slice(&0u32.to_le_bytes()); // placeholder
    buf.extend_from_slice(b"hdrl");

    // 'avih' (MainAVIHeader, 56 bytes)
    buf.extend_from_slice(b"avih");
    buf.extend_from_slice(&56u32.to_le_bytes());
    buf.extend_from_slice(&us_per_frame.to_le_bytes()); // dwMicroSecPerFrame
    buf.extend_from_slice(&0u32.to_le_bytes()); // dwMaxBytesPerSec
    buf.extend_from_slice(&0u32.to_le_bytes()); // dwPaddingGranularity
    buf.extend_from_slice(&0x910u32.to_le_bytes()); // dwFlags (AVIF_HASINDEX | AVIF_ISINTERLEAVED | AVIF_TRUSTCKTYPE)
    buf.extend_from_slice(&total_frames.to_le_bytes()); // dwTotalFrames
    buf.extend_from_slice(&0u32.to_le_bytes()); // dwInitialFrames
    buf.extend_from_slice(&1u32.to_le_bytes()); // dwStreams
    buf.extend_from_slice(&0u32.to_le_bytes()); // dwSuggestedBufferSize
    buf.extend_from_slice(&width.to_le_bytes()); // dwWidth
    buf.extend_from_slice(&height.to_le_bytes()); // dwHeight
    buf.extend_from_slice(&[0u8; 16]); // dwReserved

    // LIST 'strl'
    let strl_start = buf.len();
    buf.extend_from_slice(b"LIST");
    buf.extend_from_slice(&0u32.to_le_bytes()); // placeholder
    buf.extend_from_slice(b"strl");

    // 'strh' (AVIStreamHeader, 56 bytes)
    buf.extend_from_slice(b"strh");
    buf.extend_from_slice(&56u32.to_le_bytes());
    buf.extend_from_slice(b"vids"); // fccType
    buf.extend_from_slice(b"MJPG"); // fccHandler
    buf.extend_from_slice(&0u32.to_le_bytes()); // dwFlags
    buf.extend_from_slice(&0u16.to_le_bytes()); // wPriority
    buf.extend_from_slice(&0u16.to_le_bytes()); // wLanguage
    buf.extend_from_slice(&0u32.to_le_bytes()); // dwInitialFrames
    buf.extend_from_slice(&1u32.to_le_bytes()); // dwScale
    buf.extend_from_slice(&fps.to_le_bytes()); // dwRate
    buf.extend_from_slice(&0u32.to_le_bytes()); // dwStart
    buf.extend_from_slice(&total_frames.to_le_bytes()); // dwLength
    buf.extend_from_slice(&0u32.to_le_bytes()); // dwSuggestedBufferSize
    buf.extend_from_slice(&0u32.to_le_bytes()); // dwQuality
    buf.extend_from_slice(&0u32.to_le_bytes()); // dwSampleSize
    buf.extend_from_slice(&0u16.to_le_bytes()); // rcFrame.left
    buf.extend_from_slice(&0u16.to_le_bytes()); // rcFrame.top
    buf.extend_from_slice(&(width as u16).to_le_bytes()); // rcFrame.right
    buf.extend_from_slice(&(height as u16).to_le_bytes()); // rcFrame.bottom

    // 'strf' (BITMAPINFOHEADER, 40 bytes)
    buf.extend_from_slice(b"strf");
    buf.extend_from_slice(&40u32.to_le_bytes());
    buf.extend_from_slice(&40u32.to_le_bytes()); // biSize
    buf.extend_from_slice(&(width as i32).to_le_bytes()); // biWidth
    buf.extend_from_slice(&(height as i32).to_le_bytes()); // biHeight
    buf.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    buf.extend_from_slice(&24u16.to_le_bytes()); // biBitCount
    buf.extend_from_slice(b"MJPG"); // biCompression
    buf.extend_from_slice(&(width * height * 3).to_le_bytes()); // biSizeImage
    buf.extend_from_slice(&0i32.to_le_bytes()); // biXPelsPerMeter
    buf.extend_from_slice(&0i32.to_le_bytes()); // biYPelsPerMeter
    buf.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    buf.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant

    // Patch strl size
    let strl_len = (buf.len() - strl_start - 8) as u32;
    buf[strl_start + 4..strl_start + 8].copy_from_slice(&strl_len.to_le_bytes());

    // Patch hdrl size
    let hdrl_len = (buf.len() - hdrl_content_start) as u32;
    buf[hdrl_content_start - 4..hdrl_content_start].copy_from_slice(&hdrl_len.to_le_bytes());

    // LIST 'movi'
    let movi_size = movi_data_len + 4;
    buf.extend_from_slice(b"LIST");
    buf.extend_from_slice(&movi_size.to_le_bytes());
    buf.extend_from_slice(b"movi");
}
