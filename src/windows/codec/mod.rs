//! Windows Platform-Specific Hardware Video Codecs (Media Foundation / MFT)
//!
//! Provides GPU-accelerated video encoding and decoding using native Windows Media Foundation:
//! - Hardware H.264 / AVC encoder & decoder (NVIDIA NVENC, Intel QuickSync, AMD AMF)
//! - Optional H.265 / HEVC hardware probe
//! - Zero external binaries or DLLs required (uses built-in System32 MFTs)

pub mod h264_mf;
pub mod h265_mf;

pub use h264_mf::{H264MfDecoder, H264MfEncoder, is_h264_hardware_available};
pub use h265_mf::is_h265_hardware_available;

/// Platform type aliases unifying Windows & Linux codec dispatch
pub type PlatformH264Encoder = H264MfEncoder;
pub type PlatformH264Decoder = H264MfDecoder;

/// Probes and logs all available GPU hardware-accelerated video transforms
pub fn probe_hardware_codecs() {
    let h264 = is_h264_hardware_available();
    let h265 = is_h265_hardware_available();
    tracing::info!(
        "GPU Hardware Codec Detection: H.264 MFT = {}, H.265 MFT = {}",
        if h264 { "READY (Hardware Accelerated)" } else { "DISABLED / NOT FOUND" },
        if h265 { "READY" } else { "NOT EXTENDED" }
    );
}
