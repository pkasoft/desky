//! Linux Platform-Specific Hardware Video Codecs (VA-API / V4L2)
//!
//! Provides GPU-accelerated video encoding and decoding using native Linux facilities:
//! - Hardware H.264 / AVC encoder & decoder via VA-API (`libva.so.2` / `libva-drm.so.2`)
//! - Optional H.265 / HEVC hardware probe
//! - V4L2 M2M hardware device detection
//! - Dynamic runtime loading with zero compile-time C dependencies
//! - Automatic fallback to pure-Rust SIMD Motion-JPEG if hardware is unavailable

pub mod h264_vaapi;
pub mod h265_vaapi;
pub mod v4l2;

pub use h264_vaapi::{
    is_h264_decode_hardware_available, is_h264_hardware_available, H264VaapiDecoder,
    H264VaapiEncoder,
};
pub use h265_vaapi::is_h265_hardware_available;
pub use v4l2::probe_v4l2_devices;

/// Platform type aliases unifying Windows & Linux codec dispatch
pub type PlatformH264Encoder = H264VaapiEncoder;
pub type PlatformH264Decoder = H264VaapiDecoder;

/// Probes and logs all available Linux GPU hardware-accelerated video transforms
pub fn probe_hardware_codecs() {
    let h264_enc = is_h264_hardware_available();
    let h264_dec = is_h264_decode_hardware_available();
    let h265 = is_h265_hardware_available();
    let v4l2 = probe_v4l2_devices();

    tracing::info!(
        "Linux GPU Hardware Codec Detection: VA-API H.264 Encoder = {}, Decoder = {}, VA-API H.265 = {}, V4L2 Devices = {}",
        if h264_enc { "READY (Hardware Accelerated)" } else { "DISABLED / NOT FOUND" },
        if h264_dec { "READY (Hardware Accelerated)" } else { "DISABLED / NOT FOUND" },
        if h265 { "READY" } else { "NOT EXTENDED" },
        if v4l2 { "DETECTED" } else { "NONE" }
    );
}
