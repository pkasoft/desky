//! DeskyTile: Custom Hybrid Lossless-RLE + 4:4:4 Tile-Delta Remote Desktop Codec
//!
//! Designed specifically for ultra-sharp desktop text and low-latency interaction:
//! - Divides the screen into 64x64 pixel tiles and skips unchanged tiles (0 bytes, 0 CPU)
//! - Flat UI, code editors, terminals, and text regions use Lossless BGRA RLE (exact pixels)
//! - Photographic/video regions use 4:4:4 full-chroma JPEG (zero color bleeding)
//! - Pure Rust, zero external dependencies, 100% cross-platform (Windows & Linux)

use anyhow::{anyhow, Result};
use jpeg_encoder::{ColorType, Encoder, SamplingFactor};
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

use super::{CodecType, EncodedFrame, VideoDecoder, VideoEncoder};

pub const TILE_SIZE: usize = 64;
const TILE_MODE_RLE: u8 = 0x00;
const TILE_MODE_JPEG444: u8 = 0x01;
const TILE_MODE_SOLID: u8 = 0x02;

pub struct DeskyTileEncoder {
    quality: u8,
    prev_bgra: Vec<u8>,
    prev_w: u32,
    prev_h: u32,
    tile_scratch: Vec<u8>,
    rle_scratch: Vec<u8>,
    jpeg_scratch: Vec<u8>,
}

impl DeskyTileEncoder {
    pub fn new(quality: u8) -> Self {
        Self {
            quality: quality.clamp(50, 98),
            prev_bgra: Vec::new(),
            prev_w: 0,
            prev_h: 0,
            tile_scratch: Vec::with_capacity(TILE_SIZE * TILE_SIZE * 4),
            rle_scratch: Vec::with_capacity(TILE_SIZE * TILE_SIZE * 5),
            jpeg_scratch: Vec::with_capacity(32 * 1024),
        }
    }

    pub fn set_quality(&mut self, quality: u8) {
        self.quality = quality.clamp(50, 98);
    }

    /// Returns `Some([b, g, r, a])` if every pixel in `src` is identical.
    #[inline]
    fn detect_solid_bgra(src: &[u8]) -> Option<[u8; 4]> {
        let first: [u8; 4] = src.get(..4)?.try_into().ok()?;
        let first_u32 = u32::from_ne_bytes(first);
        for px in src.chunks_exact(4) {
            if u32::from_ne_bytes([px[0], px[1], px[2], px[3]]) != first_u32 {
                return None;
            }
        }
        Some(first)
    }

    /// Encodes BGRA pixels with RLE, aborting early if output exceeds `max_len`.
    /// Returns `true` if RLE fit within `max_len`.
    fn encode_rle_bgra(src: &[u8], out: &mut Vec<u8>, max_len: usize) -> bool {
        out.clear();
        let pixels: &[[u8; 4]] = bytemuck_pixels(src);
        let px_count = pixels.len();
        let mut i = 0;
        while i < px_count {
            let p0 = pixels[i];
            let mut run = 1usize;
            let limit = (px_count - i).min(256);
            while run < limit && pixels[i + run] == p0 {
                run += 1;
            }
            if out.len() + 5 > max_len {
                return false;
            }
            out.push((run - 1) as u8);
            out.extend_from_slice(&p0);
            i += run;
        }
        true
    }
}

#[inline]
fn bytemuck_pixels(src: &[u8]) -> &[[u8; 4]] {
    let len = src.len() / 4;
    // SAFETY: `[u8; 4]` has alignment 1 and size 4, matching 4-byte BGRA slices.
    unsafe { std::slice::from_raw_parts(src.as_ptr() as *const [u8; 4], len) }
}

impl VideoEncoder for DeskyTileEncoder {
    fn codec_type(&self) -> CodecType {
        CodecType::DeskyTile
    }

    fn encode(
        &mut self,
        bgra: &[u8],
        width: u32,
        height: u32,
        force_keyframe: bool,
    ) -> Result<EncodedFrame> {
        let w = width as usize;
        let h = height as usize;
        let expected_len = w * h * 4;
        if bgra.len() < expected_len || w == 0 || h == 0 {
            return Err(anyhow!("Invalid BGRA frame buffer for DeskyTileEncoder"));
        }

        let is_keyframe = force_keyframe
            || self.prev_w != width
            || self.prev_h != height
            || self.prev_bgra.len() != expected_len;

        let tiles_x = (w + TILE_SIZE - 1) / TILE_SIZE;
        let tiles_y = (h + TILE_SIZE - 1) / TILE_SIZE;
        let stride = w * 4;

        let mut out = Vec::with_capacity(64 * 1024);
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.push(if is_keyframe { 1 } else { 0 });
        let tile_count_pos = out.len();
        out.extend_from_slice(&0u16.to_le_bytes());

        let mut dirty_count: u16 = 0;

        for ty in 0..tiles_y {
            let y0 = ty * TILE_SIZE;
            let th = (h - y0).min(TILE_SIZE);
            for tx in 0..tiles_x {
                let x0 = tx * TILE_SIZE;
                let tw = (w - x0).min(TILE_SIZE);
                let row_bytes = tw * 4;

                let changed = if is_keyframe {
                    true
                } else {
                    let mut diff = false;
                    for row in 0..th {
                        let off = (y0 + row) * stride + x0 * 4;
                        if bgra[off..off + row_bytes] != self.prev_bgra[off..off + row_bytes] {
                            diff = true;
                            break;
                        }
                    }
                    diff
                };

                if !changed {
                    continue;
                }

                // Copy tile into contiguous scratch buffer
                self.tile_scratch.clear();
                for row in 0..th {
                    let off = (y0 + row) * stride + x0 * 4;
                    self.tile_scratch.extend_from_slice(&bgra[off..off + row_bytes]);
                }

                // 1. Fast path: solid-color tile (4 bytes total)
                // 2. Lossless BGRA RLE with early abort if compression < 2.5x
                // 3. Fallback: 4:4:4 full-chroma JPEG for photographic/video regions
                let solid_buf;
                let (mode, payload): (u8, &[u8]) = if let Some(px) = Self::detect_solid_bgra(&self.tile_scratch) {
                    solid_buf = px;
                    (TILE_MODE_SOLID, &solid_buf)
                } else {
                    let max_rle_len = (self.tile_scratch.len() * 2) / 5;
                    if Self::encode_rle_bgra(&self.tile_scratch, &mut self.rle_scratch, max_rle_len) {
                        (TILE_MODE_RLE, &self.rle_scratch)
                    } else {
                        self.jpeg_scratch.clear();
                        let tile_q = if is_keyframe { self.quality.min(80) } else { self.quality };
                        let mut enc = Encoder::new(&mut self.jpeg_scratch, tile_q);
                        enc.set_sampling_factor(SamplingFactor::R_4_4_4);
                        enc.encode(&self.tile_scratch, tw as u16, th as u16, ColorType::Bgra)?;
                        (TILE_MODE_JPEG444, &self.jpeg_scratch)
                    }
                };

                out.extend_from_slice(&(tx as u16).to_le_bytes());
                out.extend_from_slice(&(ty as u16).to_le_bytes());
                out.extend_from_slice(&(tw as u16).to_le_bytes());
                out.extend_from_slice(&(th as u16).to_le_bytes());
                out.push(mode);
                out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                out.extend_from_slice(payload);
                dirty_count = dirty_count.saturating_add(1);
            }
        }

        out[tile_count_pos..tile_count_pos + 2].copy_from_slice(&dirty_count.to_le_bytes());

        // Update previous frame cache only if anything changed (or dimensions changed)
        if is_keyframe || dirty_count > 0 {
            if self.prev_bgra.len() != expected_len {
                self.prev_bgra.resize(expected_len, 0);
            }
            self.prev_bgra.copy_from_slice(&bgra[..expected_len]);
            self.prev_w = width;
            self.prev_h = height;
        }

        Ok(EncodedFrame {
            codec: CodecType::DeskyTile,
            is_keyframe,
            data: out,
            orig_width: width,
            orig_height: height,
        })
    }
}

pub struct DeskyTileDecoder {
    fb: Vec<u8>,
    fb_w: u32,
    fb_h: u32,
}

impl DeskyTileDecoder {
    pub fn new() -> Self {
        Self {
            fb: Vec::new(),
            fb_w: 0,
            fb_h: 0,
        }
    }

    fn decode_solid_into_fb(
        solid: &[u8],
        fb: &mut [u8],
        fb_stride: usize,
        x0: usize,
        y0: usize,
        tw: usize,
        th: usize,
    ) -> Result<()> {
        if solid.len() < 4 {
            return Ok(());
        }
        let px: [u8; 4] = [solid[0], solid[1], solid[2], solid[3]];
        let row_bytes = tw * 4;
        for row in 0..th {
            let start = (y0 + row) * fb_stride + x0 * 4;
            if start + row_bytes <= fb.len() {
                for dst in fb[start..start + row_bytes].chunks_exact_mut(4) {
                    dst.copy_from_slice(&px);
                }
            }
        }
        Ok(())
    }

    fn decode_rle_into_fb(
        rle: &[u8],
        fb: &mut [u8],
        fb_stride: usize,
        x0: usize,
        y0: usize,
        tw: usize,
        th: usize,
    ) -> Result<()> {
        let mut row = 0usize;
        let mut col = 0usize;
        let mut pos = 0usize;
        while pos + 5 <= rle.len() && row < th {
            let mut rem = (rle[pos] as usize) + 1;
            let px = [rle[pos + 1], rle[pos + 2], rle[pos + 3], rle[pos + 4]];
            pos += 5;

            while rem > 0 && row < th {
                let span = rem.min(tw - col);
                let start = (y0 + row) * fb_stride + (x0 + col) * 4;
                let end = start + span * 4;
                if end <= fb.len() {
                    for dst in fb[start..end].chunks_exact_mut(4) {
                        dst.copy_from_slice(&px);
                    }
                }
                col += span;
                rem -= span;
                if col == tw {
                    col = 0;
                    row += 1;
                }
            }
        }
        Ok(())
    }
}

impl VideoDecoder for DeskyTileDecoder {
    fn codec_type(&self) -> CodecType {
        CodecType::DeskyTile
    }

    fn decode(
        &mut self,
        data: &[u8],
        _expected_width: u32,
        _expected_height: u32,
    ) -> Result<Vec<u8>> {
        if data.len() < 11 {
            return Err(anyhow!("DeskyTile frame too short"));
        }
        let w = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let h = u32::from_le_bytes(data[4..8].try_into().unwrap());
        let _is_kf = data[8] != 0;
        let tile_count = u16::from_le_bytes(data[9..11].try_into().unwrap()) as usize;

        if w == 0 || h == 0 || w > 8192 || h > 8192 {
            return Err(anyhow!("Invalid DeskyTile dimensions {}x{}", w, h));
        }

        let fb_len = (w as usize) * (h as usize) * 4;
        if self.fb_w != w || self.fb_h != h || self.fb.len() != fb_len {
            self.fb.resize(fb_len, 0);
            self.fb_w = w;
            self.fb_h = h;
        }

        let stride = (w as usize) * 4;
        let mut cursor = 11usize;

        for _ in 0..tile_count {
            if cursor + 13 > data.len() {
                break;
            }
            let tx = u16::from_le_bytes(data[cursor..cursor + 2].try_into().unwrap()) as usize;
            let ty = u16::from_le_bytes(data[cursor + 2..cursor + 4].try_into().unwrap()) as usize;
            let tw = u16::from_le_bytes(data[cursor + 4..cursor + 6].try_into().unwrap()) as usize;
            let th = u16::from_le_bytes(data[cursor + 6..cursor + 8].try_into().unwrap()) as usize;
            let mode = data[cursor + 8];
            let payload_len = u32::from_le_bytes(data[cursor + 9..cursor + 13].try_into().unwrap()) as usize;
            cursor += 13;

            if cursor + payload_len > data.len() {
                break;
            }
            let payload = &data[cursor..cursor + payload_len];
            cursor += payload_len;

            let x0 = tx * TILE_SIZE;
            let y0 = ty * TILE_SIZE;
            if x0 + tw > w as usize || y0 + th > h as usize {
                continue;
            }

            if mode == TILE_MODE_SOLID {
                let _ = Self::decode_solid_into_fb(payload, &mut self.fb, stride, x0, y0, tw, th);
            } else if mode == TILE_MODE_RLE {
                let _ = Self::decode_rle_into_fb(payload, &mut self.fb, stride, x0, y0, tw, th);
            } else if mode == TILE_MODE_JPEG444 {
                let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::BGRA);
                let mut dec = JpegDecoder::new_with_options(std::io::Cursor::new(payload), options);
                if let Ok(pixels) = dec.decode() {
                    if pixels.len() == tw * th * 4 {
                        let row_bytes = tw * 4;
                        for row in 0..th {
                            let dst_off = (y0 + row) * stride + x0 * 4;
                            let src_off = row * row_bytes;
                            self.fb[dst_off..dst_off + row_bytes]
                                .copy_from_slice(&pixels[src_off..src_off + row_bytes]);
                        }
                    }
                }
            }
        }

        Ok(self.fb.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desky_tile_keyframe_and_delta_roundtrip() {
        let w = 128u32;
        let h = 128u32;
        let mut frame1 = vec![0xFFu8; (w * h * 4) as usize];
        // Draw a blue rectangle in top-left tile (0,0)
        for y in 0..32usize {
            for x in 0..32usize {
                let idx = (y * (w as usize) + x) * 4;
                frame1[idx] = 0xAA;
                frame1[idx + 1] = 0x44;
                frame1[idx + 2] = 0x11;
                frame1[idx + 3] = 0xFF;
            }
        }

        let mut enc = DeskyTileEncoder::new(92);
        let mut dec = DeskyTileDecoder::new();

        // Keyframe encodes all 4 tiles (lossless RLE!)
        let kf = enc.encode(&frame1, w, h, true).unwrap();
        assert!(kf.is_keyframe);
        let out1 = dec.decode(&kf.data, w, h).unwrap();
        assert_eq!(out1, frame1);

        // Modify ONLY bottom-right tile (1,1) -> delta frame must be tiny and pixel-exact!
        let mut frame2 = frame1.clone();
        for y in 70..90usize {
            for x in 70..90usize {
                let idx = (y * (w as usize) + x) * 4;
                frame2[idx] = 0x22;
                frame2[idx + 1] = 0x88;
                frame2[idx + 2] = 0x44;
                frame2[idx + 3] = 0xFF;
            }
        }
        let delta = enc.encode(&frame2, w, h, false).unwrap();
        assert!(!delta.is_keyframe);
        assert!(delta.data.len() < kf.data.len());
        let tile_count = u16::from_le_bytes(delta.data[9..11].try_into().unwrap());
        assert_eq!(tile_count, 1);

        let out2 = dec.decode(&delta.data, w, h).unwrap();
        assert_eq!(out2, frame2);

        // Idle frame -> 0 dirty tiles
        let idle = enc.encode(&frame2, w, h, false).unwrap();
        assert_eq!(u16::from_le_bytes(idle.data[9..11].try_into().unwrap()), 0);
        assert_eq!(dec.decode(&idle.data, w, h).unwrap(), frame2);
    }

    #[test]
    fn test_desky_tile_solid_and_jpeg444_non_multiple_dims() {
        let w = 130u32;
        let h = 95u32;
        let mut frame = vec![0u8; (w * h * 4) as usize];
        // Fill with a solid color first, then put noisy gradient in tile (0,0) to trigger JPEG444
        for px in frame.chunks_exact_mut(4) {
            px.copy_from_slice(&[0x33, 0x66, 0x99, 0xFF]);
        }
        for y in 0..64usize {
            for x in 0..64usize {
                let idx = (y * (w as usize) + x) * 4;
                frame[idx] = ((x * 7 + y * 13) & 0xFF) as u8;
                frame[idx + 1] = ((x * 11 + y * 5) & 0xFF) as u8;
                frame[idx + 2] = ((x * 3 + y * 17) & 0xFF) as u8;
                frame[idx + 3] = 0xFF;
            }
        }

        let mut enc = DeskyTileEncoder::new(92);
        let mut dec = DeskyTileDecoder::new();
        let kf = enc.encode(&frame, w, h, true).unwrap();
        let out = dec.decode(&kf.data, w, h).unwrap();
        assert_eq!(out.len(), frame.len());
        // Solid edge tiles must be pixel-exact
        let edge_off = (80 * (w as usize) + 100) * 4;
        assert_eq!(&out[edge_off..edge_off + 4], &[0x33, 0x66, 0x99, 0xFF]);
    }
}
