//! Desky Unified Video Codec Architecture
//!
//! Provides a clean, cross-platform video codec abstraction:
//! - Standard traits: `VideoEncoder`, `VideoDecoder`, `EncodedFrame`, and `CodecType`.
//! - Unified cross-platform Motion-JPEG implementation (`src/codec/mjpeg.rs`).
//! - OS-specific hardware-accelerated codecs are isolated in their respective OS platform modules
//!   (e.g., `src/windows/codec/` for Windows Media Foundation MFT hardware acceleration).

pub mod mjpeg;

/// Codec identifiers used across the wire protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CodecType {
    /// SIMD-accelerated baseline Motion-JPEG (always-available fallback)
    Mjpeg = 0x01,
    /// Hardware-accelerated H.264 / AVC video stream
    H264 = 0x02,
    /// Hardware-accelerated H.265 / HEVC video stream
    H265 = 0x03,
}

impl CodecType {
    pub fn from_u8(val: u8) -> Self {
        match val {
            0x02 => CodecType::H264,
            0x03 => CodecType::H265,
            _ => CodecType::Mjpeg,
        }
    }
}

/// A compressed video frame ready for network packetization
#[derive(Debug, Clone)]
pub struct EncodedFrame {
    pub codec: CodecType,
    pub is_keyframe: bool,
    pub data: Vec<u8>,
    pub orig_width: u32,
    pub orig_height: u32,
}

/// Unified interface for screen frame video encoders
pub trait VideoEncoder: Send {
    fn codec_type(&self) -> CodecType;

    /// Encodes a raw BGRA32 frame buffer into compressed video bitstream bytes.
    fn encode(
        &mut self,
        bgra: &[u8],
        width: u32,
        height: u32,
        force_keyframe: bool,
    ) -> anyhow::Result<EncodedFrame>;
}

/// Unified interface for screen frame video decoders
pub trait VideoDecoder: Send {
    fn codec_type(&self) -> CodecType;

    /// Decodes compressed video bitstream bytes into raw BGRA32 pixel data.
    fn decode(
        &mut self,
        data: &[u8],
        expected_width: u32,
        expected_height: u32,
    ) -> anyhow::Result<Vec<u8>>;
}
