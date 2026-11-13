//! Cross-Platform SIMD-Accelerated Motion-JPEG Codec
//!
//! Serves as the primary reliable fallback when hardware H.264 / H.265
//! acceleration is unavailable or unsupported on the host/client machine.

use anyhow::{anyhow, Result};
use super::{CodecType, EncodedFrame, VideoDecoder, VideoEncoder};

/// High-speed SIMD JPEG encoder with configurable quality and chroma subsampling
pub struct MjpegEncoder {
    pub quality: u8,
    pub use_chroma_444: bool,
    buffer: Vec<u8>,
}

impl MjpegEncoder {
    pub fn new(quality: u8, use_chroma_444: bool) -> Self {
        Self {
            quality: quality.clamp(30, 95),
            use_chroma_444,
            buffer: Vec::with_capacity(131_072), // 128 KB initial reservation
        }
    }

    pub fn set_quality(&mut self, quality: u8) {
        self.quality = quality.clamp(30, 95);
    }

    pub fn set_chroma_444(&mut self, chroma_444: bool) {
        self.use_chroma_444 = chroma_444;
    }
}

impl Default for MjpegEncoder {
    fn default() -> Self {
        Self::new(76, true)
    }
}

impl VideoEncoder for MjpegEncoder {
    fn codec_type(&self) -> CodecType {
        CodecType::Mjpeg
    }

    fn encode(
        &mut self,
        bgra: &[u8],
        width: u32,
        height: u32,
        _force_keyframe: bool,
    ) -> Result<EncodedFrame> {
        if width == 0 || height == 0 {
            return Err(anyhow!("Invalid frame dimensions: {}x{}", width, height));
        }

        self.buffer.clear();
        let mut encoder = jpeg_encoder::Encoder::new(&mut self.buffer, self.quality);
        encoder.set_sampling_factor(if self.use_chroma_444 {
            jpeg_encoder::SamplingFactor::R_4_4_4
        } else {
            jpeg_encoder::SamplingFactor::R_4_2_0
        });

        encoder.encode(
            bgra,
            width as u16,
            height as u16,
            jpeg_encoder::ColorType::Bgra,
        )?;

        Ok(EncodedFrame {
            codec: CodecType::Mjpeg,
            is_keyframe: true, // In Motion-JPEG, every frame is an intra-frame (keyframe)
            data: self.buffer.clone(),
            orig_width: width,
            orig_height: height,
        })
    }
}

/// Fast SIMD JPEG decoder powered by `zune-jpeg`
pub struct MjpegDecoder {
    output_bgra: Vec<u8>,
}

impl MjpegDecoder {
    pub fn new() -> Self {
        Self {
            output_bgra: Vec::with_capacity(1920 * 1080 * 4),
        }
    }
}

impl Default for MjpegDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoDecoder for MjpegDecoder {
    fn codec_type(&self) -> CodecType {
        CodecType::Mjpeg
    }

    fn decode(
        &mut self,
        data: &[u8],
        expected_width: u32,
        expected_height: u32,
    ) -> Result<Vec<u8>> {
        if data.is_empty() {
            return Err(anyhow!("Empty JPEG data buffer"));
        }

        let mut decoder = zune_jpeg::JpegDecoder::new(data);
        let rgb = decoder.decode().map_err(|e| anyhow!("JPEG decode error: {:?}", e))?;
        let info = decoder.info().ok_or_else(|| anyhow!("Missing JPEG header metadata"))?;

        let w = info.width as usize;
        let h = info.height as usize;
        let pixel_count = w * h;
        let required_bytes = pixel_count * 4;

        if pixel_count == 0 || rgb.len() < pixel_count * 3 {
            return Err(anyhow!("Corrupt JPEG dimensions or payload"));
        }

        if self.output_bgra.len() != required_bytes {
            self.output_bgra.resize(required_bytes, 0);
        }

        let src = rgb.as_slice();
        let dst = self.output_bgra.as_mut_slice();

        let mut s = 0;
        let mut d = 0;
        while s + 3 <= src.len() && d + 4 <= dst.len() {
            dst[d] = src[s + 2];     // B
            dst[d + 1] = src[s + 1]; // G
            dst[d + 2] = src[s];     // R
            dst[d + 3] = 255;        // A
            s += 3;
            d += 4;
        }

        let _ = (expected_width, expected_height);
        Ok(self.output_bgra.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mjpeg_encode_decode_roundtrip() {
        let width: u32 = 64;
        let height: u32 = 64;
        let mut bgra = vec![0u8; (width * height * 4) as usize];
        // Fill with dummy color pattern: Blue=200, Green=100, Red=50, Alpha=255
        for chunk in bgra.chunks_exact_mut(4) {
            chunk[0] = 200; // B
            chunk[1] = 100; // G
            chunk[2] = 50;  // R
            chunk[3] = 255; // A
        }

        let mut encoder = MjpegEncoder::new(75, true);
        let frame = encoder.encode(&bgra, width, height, false).expect("Encode failed");
        assert_eq!(frame.codec, CodecType::Mjpeg);
        assert!(!frame.data.is_empty());
        assert!(frame.is_keyframe);

        let mut decoder = MjpegDecoder::new();
        let decoded_bgra = decoder.decode(&frame.data, width, height).expect("Decode failed");
        assert_eq!(decoded_bgra.len(), (width * height * 4) as usize);

        // JPEG is lossy, but decoded pixels should be close to original within tolerance
        let b = decoded_bgra[0] as i32;
        let g = decoded_bgra[1] as i32;
        let r = decoded_bgra[2] as i32;
        let a = decoded_bgra[3];

        assert!((b - 200).abs() < 15, "Blue channel mismatch: got {}", b);
        assert!((g - 100).abs() < 15, "Green channel mismatch: got {}", g);
        assert!((r - 50).abs() < 15, "Red channel mismatch: got {}", r);
        assert_eq!(a, 255, "Alpha channel should be 255");
    }
}
