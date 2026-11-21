//! Fast, Zero-Overhead AVI / Motion-JPEG Session Recorder
//! Direct container muxing for incoming remote JPEG frames with 0% CPU re-encoding overhead.

use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::info;

#[derive(Debug, Clone)]
pub enum RecordedFrame {
    Jpeg(Vec<u8>),
    Bgra {
        data: Vec<u8>,
        width: u32,
        height: u32,
    },
}

static IS_RECORDING: AtomicBool = AtomicBool::new(false);
static RECORD_FRAMES: AtomicU32 = AtomicU32::new(0);
static RECORD_START_TIME: AtomicU64 = AtomicU64::new(0);
static RECORDER_SENDER: Mutex<Option<SyncSender<RecordedFrame>>> = Mutex::new(None);
static LAST_SAVED_INFO: Mutex<Option<(PathBuf, u64, u32, Duration)>> = Mutex::new(None);

pub static RECORD_CAPTURE_REMOTE_AUDIO: AtomicBool = AtomicBool::new(true);
pub static RECORD_CAPTURE_LOCAL_AUDIO: AtomicBool = AtomicBool::new(false);
pub static RECORD_QUALITY_SCALE: AtomicU8 = AtomicU8::new(0); // 0 = Native (100%), 1 = 75%, 2 = 50%
pub static RECORD_AUTO_START: AtomicBool = AtomicBool::new(false);

pub fn is_remote_audio_recording_enabled() -> bool {
    RECORD_CAPTURE_REMOTE_AUDIO.load(Ordering::Relaxed)
}

pub fn set_remote_audio_recording_enabled(enabled: bool) {
    RECORD_CAPTURE_REMOTE_AUDIO.store(enabled, Ordering::Relaxed);
}

pub fn is_local_audio_recording_enabled() -> bool {
    RECORD_CAPTURE_LOCAL_AUDIO.load(Ordering::Relaxed)
}

pub fn set_local_audio_recording_enabled(enabled: bool) {
    RECORD_CAPTURE_LOCAL_AUDIO.store(enabled, Ordering::Relaxed);
}

pub fn get_recording_quality_scale() -> u8 {
    RECORD_QUALITY_SCALE.load(Ordering::Relaxed)
}

pub fn set_recording_quality_scale(scale: u8) {
    RECORD_QUALITY_SCALE.store(scale.min(2), Ordering::Relaxed);
}

pub fn is_auto_record_enabled() -> bool {
    RECORD_AUTO_START.load(Ordering::Relaxed)
}

pub fn set_auto_record_enabled(enabled: bool) {
    RECORD_AUTO_START.store(enabled, Ordering::Relaxed);
}

pub fn get_recording_save_dir() -> PathBuf {
    dirs_fallback_videos()
}

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
    let (tx, rx) = sync_channel::<RecordedFrame>(90);

    if let Ok(mut guard) = RECORDER_SENDER.lock() {
        *guard = Some(tx);
    }
    if let Ok(mut guard) = LAST_SAVED_INFO.lock() {
        *guard = None;
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
            let _ = tx.try_send(RecordedFrame::Jpeg(jpeg_bytes.to_vec()));
        }
    }
}

/// Push incoming uncompressed BGRA frame to the background AVI writer (encoded to JPEG asynchronously)
pub fn push_bgra_frame(bgra: &[u8], width: u32, height: u32) {
    if !is_recording() || bgra.is_empty() || width == 0 || height == 0 {
        return;
    }
    let expected_len = (width as usize) * (height as usize) * 4;
    if bgra.len() < expected_len {
        return;
    }
    if let Ok(guard) = RECORDER_SENDER.lock() {
        if let Some(ref tx) = *guard {
            let _ = tx.try_send(RecordedFrame::Bgra {
                data: bgra[..expected_len].to_vec(),
                width,
                height,
            });
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

const HEADER_SIZE: usize = 2048;

fn run_recorder_writer(
    path: PathBuf,
    file: &mut File,
    rx: Receiver<RecordedFrame>,
    mut width: u32,
    mut height: u32,
    fps: u32,
) {
    let start_time = Instant::now();
    let mut frames_written = 0u32;
    let mut index_entries = Vec::<AviIndexEntry>::new();

    // 1. Allocate exact placeholder header space (will be rewritten at the end)
    let zero_buf = vec![0u8; HEADER_SIZE];
    let _ = file.write_all(&zero_buf);

    // movi LIST starts at byte (HEADER_SIZE - 12) = 2036
    // 'movi' FOURCC starts at byte (HEADER_SIZE - 4) = 2044
    // First data chunk starts exactly at byte HEADER_SIZE = 2048
    let movi_fourcc_pos = (HEADER_SIZE - 4) as u32;

    let mut jpeg_buf = Vec::with_capacity(256 * 1024);
    let mut last_jpeg = Vec::<u8>::new();
    let frame_interval = Duration::from_micros(1_000_000 / fps.max(1) as u64);

    // 2. Receive and write frames (duplicate last frame at target FPS during idle periods)
    loop {
        let frame_opt = match rx.recv_timeout(frame_interval) {
            Ok(f) => Some(f),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if !IS_RECORDING.load(Ordering::Relaxed) {
                    break;
                }
                if last_jpeg.is_empty() {
                    continue;
                }
                None
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };

        let frame_jpeg: &[u8] = if let Some(ref frame) = frame_opt {
            match frame {
                RecordedFrame::Jpeg(bytes) => {
                    if bytes.is_empty() {
                        continue;
                    }
                    // Dynamically detect frame resolution from actual JPEG headers
                    if frames_written == 0 || width == 0 || height == 0 {
                        if let Some((w, h)) = parse_jpeg_dimensions(bytes) {
                            if w > 0 && h > 0 {
                                width = w;
                                height = h;
                            }
                        }
                    }
                    last_jpeg.clear();
                    last_jpeg.extend_from_slice(bytes);
                    last_jpeg.as_slice()
                }
                RecordedFrame::Bgra { data, width: w, height: h } => {
                    let expected_len = (*w as usize) * (*h as usize) * 4;
                    if data.len() < expected_len || *w == 0 || *h == 0 {
                        continue;
                    }
                    if frames_written == 0 || width == 0 || height == 0 {
                        width = *w;
                        height = *h;
                    }
                    jpeg_buf.clear();
                    let (quality, sampling) = match RECORD_QUALITY_SCALE.load(Ordering::Relaxed) {
                        0 => (95, jpeg_encoder::SamplingFactor::R_4_4_4),
                        1 => (88, jpeg_encoder::SamplingFactor::R_4_2_2),
                        _ => (78, jpeg_encoder::SamplingFactor::R_4_2_0),
                    };
                    let mut encoder = jpeg_encoder::Encoder::new(&mut jpeg_buf, quality);
                    encoder.set_sampling_factor(sampling);
                    if let Err(e) = encoder.encode(
                        &data[..expected_len],
                        *w as u16,
                        *h as u16,
                        jpeg_encoder::ColorType::Bgra,
                    ) {
                        tracing::warn!("Recorder failed to encode BGRA frame to JPEG: {}", e);
                        continue;
                    }
                    last_jpeg.clear();
                    last_jpeg.extend_from_slice(&jpeg_buf);
                    last_jpeg.as_slice()
                }
            }
        } else {
            last_jpeg.as_slice()
        };

        if frame_jpeg.is_empty() {
            continue;
        }

        let chunk_pos = match file.seek(SeekFrom::Current(0)) {
            Ok(p) => p as u32,
            Err(_) => break,
        };

        let chunk_fourcc = *b"00dc";
        let chunk_len = frame_jpeg.len() as u32;

        let _ = file.write_all(&chunk_fourcc);
        let _ = file.write_all(&chunk_len.to_le_bytes());
        let _ = file.write_all(frame_jpeg);

        // Pad to word boundary (2 bytes)
        if chunk_len % 2 != 0 {
            let _ = file.write_all(&[0u8]);
        }

        // Standard AVI 1.0 RIFF index entry: offset is relative to the 'movi' FOURCC
        index_entries.push(AviIndexEntry {
            fcc: chunk_fourcc,
            flags: 0x10, // AVIIF_KEYFRAME
            offset: chunk_pos.saturating_sub(movi_fourcc_pos),
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
        .saturating_sub(HEADER_SIZE as u32)
        .saturating_sub(idx1_len + 8);

    let duration = start_time.elapsed();
    let elapsed_secs = duration.as_secs_f64();
    let actual_fps = if elapsed_secs > 0.5 && frames_written > 1 {
        let calc = (frames_written as f64 / elapsed_secs).round() as u32;
        calc.clamp(1, 60)
    } else {
        fps.clamp(1, 60)
    };

    // 4. Seek back and write standardized AVI headers filling all 2048 reserved bytes
    let _ = file.seek(SeekFrom::Start(0));

    let hdr = generate_avi_headers(
        total_file_len as u32,
        movi_data_len,
        width.max(320),
        height.max(240),
        actual_fps,
        frames_written,
    );

    let _ = file.write_all(&hdr);
    let _ = file.flush();

    info!(
        "Session recording saved successfully: {} ({} frames, {} KB, {:.1}s, {}x{} @ {}fps)",
        path.display(),
        frames_written,
        total_file_len / 1024,
        duration.as_secs_f32(),
        width,
        height,
        actual_fps
    );

    if let Ok(mut guard) = LAST_SAVED_INFO.lock() {
        *guard = Some((path, total_file_len, frames_written, duration));
    }
}

fn generate_avi_headers(
    total_len: u32,
    movi_data_len: u32,
    width: u32,
    height: u32,
    fps: u32,
    total_frames: u32,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_SIZE);
    let riff_size = total_len.saturating_sub(8);
    let us_per_frame = if fps > 0 { 1_000_000 / fps } else { 33_333 };
    let buffer_size = (width * height * 3).max(1024 * 1024);

    // RIFF 'AVI ' (12 bytes)
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&riff_size.to_le_bytes());
    buf.extend_from_slice(b"AVI ");

    // LIST 'hdrl'
    let hdrl_start = buf.len();
    buf.extend_from_slice(b"LIST");
    buf.extend_from_slice(&0u32.to_le_bytes()); // placeholder
    buf.extend_from_slice(b"hdrl");

    // 'avih' (MainAVIHeader, 56 bytes)
    buf.extend_from_slice(b"avih");
    buf.extend_from_slice(&56u32.to_le_bytes());
    buf.extend_from_slice(&us_per_frame.to_le_bytes()); // dwMicroSecPerFrame
    let max_bytes_per_sec = (buffer_size as u64 * fps as u64).min(u32::MAX as u64) as u32;
    buf.extend_from_slice(&max_bytes_per_sec.to_le_bytes()); // dwMaxBytesPerSec
    buf.extend_from_slice(&0u32.to_le_bytes()); // dwPaddingGranularity
    buf.extend_from_slice(&0x910u32.to_le_bytes()); // dwFlags (AVIF_HASINDEX | AVIF_ISINTERLEAVED | AVIF_TRUSTCKTYPE)
    buf.extend_from_slice(&total_frames.to_le_bytes()); // dwTotalFrames
    buf.extend_from_slice(&0u32.to_le_bytes()); // dwInitialFrames
    buf.extend_from_slice(&1u32.to_le_bytes()); // dwStreams
    buf.extend_from_slice(&buffer_size.to_le_bytes()); // dwSuggestedBufferSize
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
    buf.extend_from_slice(&buffer_size.to_le_bytes()); // dwSuggestedBufferSize
    buf.extend_from_slice(&10000u32.to_le_bytes()); // dwQuality
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

    // Patch hdrl size (must include 'hdrl' FOURCC and all child chunks)
    let hdrl_len = (buf.len() - hdrl_start - 8) as u32;
    buf[hdrl_start + 4..hdrl_start + 8].copy_from_slice(&hdrl_len.to_le_bytes());

    // JUNK padding chunk to bring buffer up to exact target before LIST movi
    let movi_header_size = 12usize; // LIST (4) + size (4) + movi (4)
    let target_movi_pos = HEADER_SIZE - movi_header_size;
    if buf.len() < target_movi_pos {
        let pad_chunk_total = target_movi_pos - buf.len();
        if pad_chunk_total >= 8 {
            let junk_payload_len = (pad_chunk_total - 8) as u32;
            buf.extend_from_slice(b"JUNK");
            buf.extend_from_slice(&junk_payload_len.to_le_bytes());
            buf.resize(target_movi_pos, 0u8);
        } else {
            buf.resize(target_movi_pos, 0u8);
        }
    }

    // LIST 'movi' (12 bytes) ending exactly at byte HEADER_SIZE (2048)
    let movi_size = movi_data_len + 4;
    buf.extend_from_slice(b"LIST");
    buf.extend_from_slice(&movi_size.to_le_bytes());
    buf.extend_from_slice(b"movi");

    // Ensure entire buffer is exactly HEADER_SIZE
    buf.resize(HEADER_SIZE, 0u8);
    buf
}

/// Parse JPEG width and height from SOF markers (SOF0, SOF1, SOF2)
pub fn parse_jpeg_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 9 <= data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        // 0xC0 = SOF0 (Baseline), 0xC1 = SOF1 (Extended), 0xC2 = SOF2 (Progressive)
        if marker == 0xC0 || marker == 0xC1 || marker == 0xC2 {
            let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
            if w > 0 && h > 0 {
                return Some((w, h));
            }
        }
        // End of image or start of scan: no more headers
        if marker == 0xD9 || marker == 0xDA {
            break;
        }
        if i + 4 > data.len() {
            break;
        }
        let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if seg_len < 2 {
            break;
        }
        i += 2 + seg_len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_jpeg_dimensions() {
        // Construct minimal synthetic JPEG stream with SOI and SOF0
        let mut jpeg = Vec::new();
        jpeg.extend_from_slice(&[0xFF, 0xD8]); // SOI
        jpeg.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]); // APP0 (16 bytes)
        jpeg.extend_from_slice(&[0u8; 14]);
        // SOF0 (0xFF, 0xC0, len=17, precision=8, height=1080, width=1920, components=3)
        jpeg.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
        jpeg.extend_from_slice(&1080u16.to_be_bytes());
        jpeg.extend_from_slice(&1920u16.to_be_bytes());
        jpeg.extend_from_slice(&[0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01]);
        jpeg.extend_from_slice(&[0xFF, 0xD9]); // EOI

        let dims = parse_jpeg_dimensions(&jpeg);
        assert_eq!(dims, Some((1920, 1080)));
    }

    #[test]
    fn test_avi_header_generation() {
        let hdr = generate_avi_headers(100_000, 80_000, 1920, 1080, 30, 150);
        assert_eq!(hdr.len(), HEADER_SIZE);

        // Check RIFF header
        assert_eq!(&hdr[0..4], b"RIFF");
        assert_eq!(&hdr[8..12], b"AVI ");

        // Check LIST hdrl
        assert_eq!(&hdr[12..16], b"LIST");
        let hdrl_size = u32::from_le_bytes(hdr[16..20].try_into().unwrap());
        assert_eq!(hdrl_size, 192); // 4 (hdrl) + 64 (avih) + 124 (LIST strl)
        assert_eq!(&hdr[20..24], b"hdrl");

        // Check avih
        assert_eq!(&hdr[24..28], b"avih");

        // Check LIST strl
        assert_eq!(&hdr[88..92], b"LIST");
        let strl_size = u32::from_le_bytes(hdr[92..96].try_into().unwrap());
        assert_eq!(strl_size, 116); // 4 (strl) + 64 (strh) + 48 (strf)
        assert_eq!(&hdr[96..100], b"strl");

        // Check strh
        assert_eq!(&hdr[100..104], b"strh");
        assert_eq!(&hdr[108..112], b"vids");
        assert_eq!(&hdr[112..116], b"MJPG");

        // Check strf
        assert_eq!(&hdr[164..168], b"strf");

        // Check JUNK chunk before LIST movi
        // LIST movi starts at HEADER_SIZE - 12 = 2036
        assert_eq!(&hdr[2036..2040], b"LIST");
        let movi_size = u32::from_le_bytes(hdr[2040..2044].try_into().unwrap());
        assert_eq!(movi_size, 80_000 + 4);
        assert_eq!(&hdr[2044..2048], b"movi");

        // Check JUNK chunk tag exists right after strf
        let junk_tag_idx = 212; // 12 (RIFF) + 200 (hdrl)
        assert_eq!(&hdr[junk_tag_idx..junk_tag_idx + 4], b"JUNK");
    }

    #[test]
    fn test_bgra_to_jpeg_encoding() {
        let w = 64u16;
        let h = 48u16;
        let bgra = vec![128u8; (w as usize) * (h as usize) * 4];
        let mut jpeg_buf = Vec::new();
        let mut encoder = jpeg_encoder::Encoder::new(&mut jpeg_buf, 80);
        encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_2_0);
        let res = encoder.encode(&bgra, w, h, jpeg_encoder::ColorType::Bgra);
        assert!(res.is_ok());
        assert!(!jpeg_buf.is_empty());
        let dims = parse_jpeg_dimensions(&jpeg_buf);
        assert_eq!(dims, Some((64, 48)));
    }
}
