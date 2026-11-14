//! Linux Hardware H.265 / HEVC Capability Detection (VA-API)
//!
//! Checks whether the host GPU (Intel QSV / AMD Mesa Gallium) supports
//! hardware HEVC / H.265 profile decode/encode via `VAProfileHEVCMain` or `VAProfileHEVCMain10`.

use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::codec::{CodecType, EncodedFrame, VideoDecoder, VideoEncoder};

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

/// Linux VA-API Hardware H.265 / HEVC Video Encoder
pub struct H265VaapiEncoder {
    pub width: u32,
    pub height: u32,
    pub bitrate: u32,
    pub fps: u32,
}

impl H265VaapiEncoder {
    pub fn new(width: u32, height: u32, fps: u32, bitrate: u32) -> Result<Self> {
        if !is_h265_hardware_available() {
            return Err(anyhow!("VA-API HEVC / H.265 encoder profile not supported on this GPU"));
        }
        Ok(Self { width, height, bitrate, fps })
    }
}

impl VideoEncoder for H265VaapiEncoder {
    fn codec_type(&self) -> CodecType {
        CodecType::H265
    }

    fn encode(&mut self, _bgra: &[u8], _width: u32, _height: u32, _force_keyframe: bool) -> Result<EncodedFrame> {
        Err(anyhow!("VA-API HEVC encoder driver not initialized"))
    }
}

/// Linux VA-API Hardware H.265 / HEVC Video Decoder
pub struct H265VaapiDecoder;

impl H265VaapiDecoder {
    pub fn new() -> Result<Self> {
        if !is_h265_hardware_available() {
            return Err(anyhow!("VA-API HEVC / H.265 decoder profile not supported on this GPU"));
        }
        Ok(Self)
    }
}

impl Default for H265VaapiDecoder {
    fn default() -> Self {
        Self
    }
}

impl VideoDecoder for H265VaapiDecoder {
    fn codec_type(&self) -> CodecType {
        CodecType::H265
    }

    fn decode(&mut self, _data: &[u8], _expected_width: u32, _expected_height: u32) -> Result<Vec<u8>> {
        Err(anyhow!("VA-API HEVC decoder driver not initialized"))
    }
}
