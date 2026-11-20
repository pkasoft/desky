use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

/// Stream Buffer & Memory Cache preset (in Megabytes):
/// - Tier 0: 128 MB (Light / Low RAM)
/// - Tier 1: 256 MB (Standard - Default)
/// - Tier 2: 512 MB (High Stability / Packet Jitter Smoothing)
/// - Tier 3: 1024 MB (Max Buffer / Loss-tolerant)
pub static CLIENT_BUFFER_SIZE_MB: AtomicU32 = AtomicU32::new(256);

/// Get the current client stream buffer size in Megabytes.
pub fn get_client_buffer_size_mb() -> u32 {
    CLIENT_BUFFER_SIZE_MB.load(Ordering::Relaxed)
}

/// Get the segment index (0..=3) corresponding to the active buffer preset.
pub fn get_client_buffer_tier() -> usize {
    match get_client_buffer_size_mb() {
        128 => 0,
        256 => 1,
        512 => 2,
        1024 => 3,
        _ => 1,
    }
}

/// Set the stream buffer preset by segment index (0..=3).
pub fn set_client_buffer_tier(idx: usize) {
    let mb = match idx {
        0 => 128,
        1 => 256,
        2 => 512,
        3 => 1024,
        _ => 256,
    };
    CLIENT_BUFFER_SIZE_MB.store(mb, Ordering::Relaxed);
}

/// Returns the dynamic reassembly buffer constraints: `(max_partial_frames, max_frame_age)`
/// tailored to smooth packet jitter and prevent video decoder starvation / black screen.
pub fn get_buffer_reassembly_limits() -> (usize, Duration) {
    match get_client_buffer_size_mb() {
        128 => (16, Duration::from_millis(800)),
        256 => (32, Duration::from_millis(1500)),
        512 => (64, Duration::from_millis(2500)),
        1024 => (128, Duration::from_millis(4000)),
        _ => (32, Duration::from_millis(1500)),
    }
}
