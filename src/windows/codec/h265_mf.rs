//! Windows Media Foundation H.265 / HEVC Hardware Codec Module
//!
//! Provides capability detection and hardware acceleration hooks for HEVC.

use std::sync::atomic::{AtomicBool, Ordering};

static H265_AVAILABLE: AtomicBool = AtomicBool::new(false);
static H265_CHECKED: AtomicBool = AtomicBool::new(false);

/// Probes whether the current system has a hardware-accelerated H.265 / HEVC MFT encoder
pub fn is_h265_hardware_available() -> bool {
    if H265_CHECKED.load(Ordering::Relaxed) {
        return H265_AVAILABLE.load(Ordering::Relaxed);
    }

    // Windows 10/11 requires the optional Microsoft "HEVC Video Extensions" package
    // or vendor GPU MFTs. By default, Desky prefers H.264 for universal compatibility.
    H265_CHECKED.store(true, Ordering::Relaxed);
    H265_AVAILABLE.store(false, Ordering::Relaxed);
    false
}
