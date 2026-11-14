//! Linux Platform-Specific Hardware Video Codecs (VA-API / V4L2 / VDPAU)
//!
//! Provides GPU-accelerated video encoding and decoding using native Linux facilities
//! with cross-distribution dynamic loading (Debian, Ubuntu, Fedora, RHEL, Arch, openSUSE, Alpine):
//! - Hardware H.264 / AVC encoder & decoder via VA-API (`libva.so.2/1/so` and `libva-drm.so.2/1/so`)
//! - Optional H.265 / HEVC hardware probe
//! - V4L2 M2M hardware device detection (Raspberry Pi, ARM SBCs, Rockchip)
//! - NVIDIA proprietary driver hardware detection (VDPAU / NVDEC)
//! - Dynamic runtime loading with zero compile-time C dependencies
//! - Automatic fallback to pure-Rust SIMD Motion-JPEG if hardware is unavailable

pub mod h264_vaapi;
pub mod h265_vaapi;
pub mod v4l2;
pub mod vdpau;

pub use h264_vaapi::{
    is_h264_decode_hardware_available, is_h264_hardware_available, H264VaapiDecoder,
    H264VaapiEncoder,
};
pub use h265_vaapi::{is_h265_hardware_available, H265VaapiDecoder, H265VaapiEncoder};
pub use v4l2::probe_v4l2_devices;
pub use vdpau::is_vdpau_hardware_available;

/// Platform type aliases unifying Windows & Linux codec dispatch
pub type PlatformH264Encoder = H264VaapiEncoder;
pub type PlatformH264Decoder = H264VaapiDecoder;
pub type PlatformH265Encoder = H265VaapiEncoder;
pub type PlatformH265Decoder = H265VaapiDecoder;

/// Probes and logs all available Linux GPU hardware-accelerated video transforms
pub fn probe_hardware_codecs() {
    let h264_enc = is_h264_hardware_available();
    let h264_dec = is_h264_decode_hardware_available();
    let h265 = is_h265_hardware_available();
    let v4l2 = probe_v4l2_devices();
    let vdpau = is_vdpau_hardware_available();

    tracing::info!(
        "Linux GPU Codecs: VA-API H.264 (Enc: {}, Dec: {}), H.265: {}, NVIDIA: {}, V4L2: {}",
        if h264_enc { "READY" } else { "NONE" },
        if h264_dec { "READY" } else { "NONE" },
        if h265 { "READY" } else { "NOT EXTENDED" },
        if vdpau { "DETECTED (VDPAU/CUDA)" } else { "NONE" },
        if v4l2 { "DETECTED" } else { "NONE" }
    );
}
