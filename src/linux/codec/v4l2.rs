//! Linux Video4Linux2 (V4L2) M2M Hardware Codec Detection
//!
//! Checks whether the Linux system possesses stateful or stateless V4L2
//! hardware codec devices (`/dev/video10`, `/dev/video11`, etc.) commonly found
//! on ARM, embedded Linux, and Raspberry Pi platforms.

use std::fs;
use tracing::debug;

/// Scans `/dev/` for potential V4L2 hardware video codec devices
pub fn probe_v4l2_devices() -> bool {
    let mut found = false;
    if let Ok(entries) = fs::read_dir("/dev") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("video") {
                // e.g. /dev/video10, /dev/video11 (typical M2M encoder/decoder nodes)
                debug!("Detected V4L2 video node: /dev/{}", name_str);
                found = true;
            }
        }
    }
    found
}
