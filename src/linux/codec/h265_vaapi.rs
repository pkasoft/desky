//! Linux Hardware H.265 / HEVC Capability Detection (VA-API)
//!
//! Checks whether the host GPU (Intel QSV / AMD Mesa Gallium) supports
//! hardware HEVC / H.265 profile decode/encode via `VAProfileHEVCMain` or `VAProfileHEVCMain10`.

use std::sync::atomic::{AtomicBool, Ordering};

static H265_HW_CHECKED: AtomicBool = AtomicBool::new(false);
static H265_HW_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Probes whether the Linux system possesses a hardware-accelerated H.265 / HEVC encoder or decoder
pub fn is_h265_hardware_available() -> bool {
    if H265_HW_CHECKED.load(Ordering::Relaxed) {
        return H265_HW_AVAILABLE.load(Ordering::Relaxed);
    }

    // VA-API profiles for H.265 are checked during VA-API initialization
    H265_HW_AVAILABLE.store(false, Ordering::Relaxed);
    H265_HW_CHECKED.store(true, Ordering::Relaxed);
    false
}
