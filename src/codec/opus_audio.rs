//! Pure-Rust Opus Audio Codec Wrapper (48 kHz, 20 ms frames, PLC & Resampling)
//!
//! Compresses raw 16-bit PCM desktop audio (~1.5 Mbps at 48 kHz stereo) down to
//! ~96 kbps Opus frames (~240 bytes per 20 ms), fitting well below Ethernet MTU.

use anyhow::{anyhow, Result};
use opus1::{Application, Bitrate, Channels, OpusDecoder, OpusEncoder, SampleRate};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU16, Ordering};

pub const OPUS_SAMPLE_RATE: u32 = 48_000;
pub const OPUS_FRAME_SAMPLES: usize = 960; // 20 ms @ 48 kHz

static OPUS_TARGET_BITRATE_KBPS: AtomicU16 = AtomicU16::new(96);

pub fn set_opus_bitrate_kbps(kbps: u16) {
    OPUS_TARGET_BITRATE_KBPS.store(kbps.clamp(32, 192), Ordering::Relaxed);
}

pub fn get_opus_bitrate_kbps() -> u16 {
    OPUS_TARGET_BITRATE_KBPS.load(Ordering::Relaxed)
}

pub struct OpusAudioEncoder {
    encoder: OpusEncoder,
    pub out_channels: u16,
    in_sample_rate: u32,
    in_channels: u16,
    applied_bitrate_kbps: u16,
    pending: Vec<i16>,
    out_buf: Vec<u8>,
}

impl OpusAudioEncoder {
    pub fn new(in_sample_rate: u32, in_channels: u16) -> Result<Self> {
        Self::new_with_mode(in_sample_rate, in_channels, false)
    }

    pub fn new_voice(in_sample_rate: u32, in_channels: u16) -> Result<Self> {
        Self::new_with_mode(in_sample_rate, in_channels, true)
    }

    fn new_with_mode(in_sample_rate: u32, in_channels: u16, is_voice: bool) -> Result<Self> {
        let out_channels = if in_channels <= 1 { 1 } else { 2 };
        let ch_enum = if out_channels == 1 {
            Channels::Mono
        } else {
            Channels::Stereo
        };
        let app = if is_voice {
            Application::Voip
        } else {
            Application::Audio
        };

        let mut encoder = OpusEncoder::new(SampleRate::Hz48000, ch_enum, app)
            .map_err(|e| anyhow!("OpusEncoder init failed: {:?}", e))?;
        let target_kbps = get_opus_bitrate_kbps();
        if let Some(bps) = NonZeroU32::new(target_kbps as u32 * 1000) {
            encoder.set_bitrate(Bitrate::BitsPerSecond(bps));
        } else {
            encoder.set_bitrate(Bitrate::Auto);
        }
        let _ = encoder.set_complexity(5);

        Ok(Self {
            encoder,
            out_channels,
            in_sample_rate: in_sample_rate.max(8000),
            in_channels: in_channels.max(1),
            applied_bitrate_kbps: target_kbps,
            pending: Vec::with_capacity(OPUS_FRAME_SAMPLES * out_channels as usize * 2),
            out_buf: vec![0u8; 1275],
        })
    }

    pub fn matches_input(&self, sample_rate: u32, channels: u16) -> bool {
        self.in_sample_rate == sample_rate.max(8000) && self.in_channels == channels.max(1)
    }

    /// Converts incoming little-endian PCM16 bytes (at `in_sample_rate`, `in_channels`)
    /// into 48 kHz Mono/Stereo and returns all complete 20 ms Opus packets.
    pub fn encode_pcm_bytes(&mut self, pcm_bytes: &[u8]) -> Vec<Vec<u8>> {
        let in_ch = self.in_channels as usize;
        let out_ch = self.out_channels as usize;
        let sample_count = pcm_bytes.len() / 2;
        let in_frames = sample_count / in_ch;
        if in_frames == 0 {
            return Vec::new();
        }

        // Convert to normalized 1/2-channel i16 frames
        let mut normalized = Vec::with_capacity(in_frames * out_ch);
        for f in 0..in_frames {
            let base = f * in_ch * 2;
            if out_ch == 1 {
                let s = i16::from_le_bytes([pcm_bytes[base], pcm_bytes[base + 1]]);
                normalized.push(s);
            } else {
                let l = i16::from_le_bytes([pcm_bytes[base], pcm_bytes[base + 1]]);
                let r = if in_ch >= 2 {
                    i16::from_le_bytes([pcm_bytes[base + 2], pcm_bytes[base + 3]])
                } else {
                    l
                };
                normalized.push(l);
                normalized.push(r);
            }
        }

        // Linear resample to 48 kHz if the host audio device runs at another rate (e.g. 44.1 kHz)
        if self.in_sample_rate == OPUS_SAMPLE_RATE {
            self.pending.extend_from_slice(&normalized);
        } else {
            let out_frames = ((in_frames as u64 * OPUS_SAMPLE_RATE as u64)
                / self.in_sample_rate as u64)
                .max(1) as usize;
            let step = (in_frames as f64) / (out_frames as f64);
            for i in 0..out_frames {
                let pos = (i as f64) * step;
                let idx0 = (pos as usize).min(in_frames - 1);
                let idx1 = (idx0 + 1).min(in_frames - 1);
                let frac = (pos - idx0 as f64) as f32;
                for c in 0..out_ch {
                    let s0 = normalized[idx0 * out_ch + c] as f32;
                    let s1 = normalized[idx1 * out_ch + c] as f32;
                    self.pending.push((s0 + (s1 - s0) * frac) as i16);
                }
            }
        }

        let target_kbps = get_opus_bitrate_kbps();
        if target_kbps != self.applied_bitrate_kbps {
            if let Some(bps) = NonZeroU32::new(target_kbps as u32 * 1000) {
                self.encoder.set_bitrate(Bitrate::BitsPerSecond(bps));
            }
            self.applied_bitrate_kbps = target_kbps;
        }

        let frame_samples_total = OPUS_FRAME_SAMPLES * out_ch;
        let mut packets = Vec::new();
        while self.pending.len() >= frame_samples_total {
            if let Ok(len) = self.encoder.encode_i16(
                &self.pending[..frame_samples_total],
                OPUS_FRAME_SAMPLES,
                &mut self.out_buf,
            ) {
                if len > 0 {
                    packets.push(self.out_buf[..len].to_vec());
                }
            }
            self.pending.drain(..frame_samples_total);
        }

        packets
    }
}

pub struct OpusAudioDecoder {
    decoder: OpusDecoder,
    channels: u16,
    last_seq: Option<u16>,
    pcm_i16_scratch: Vec<i16>,
}

impl OpusAudioDecoder {
    pub fn new(channels: u16) -> Result<Self> {
        let ch = if channels <= 1 { 1 } else { 2 };
        let ch_enum = if ch == 1 {
            Channels::Mono
        } else {
            Channels::Stereo
        };
        let decoder = OpusDecoder::new(SampleRate::Hz48000, ch_enum)
            .map_err(|e| anyhow!("OpusDecoder init failed: {:?}", e))?;
        Ok(Self {
            decoder,
            channels: ch,
            last_seq: None,
            pcm_i16_scratch: vec![0i16; OPUS_FRAME_SAMPLES * ch as usize],
        })
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Decodes an incoming `PKT_AUDIO_OPUS` frame, handling sequence ordering and PLC.
    /// Returns little-endian 48 kHz PCM16 bytes ready for `AudioPlayer`.
    pub fn decode_packet(&mut self, seq: u16, opus_bytes: &[u8]) -> Vec<u8> {
        let mut out_bytes = Vec::new();
        let ch = self.channels as usize;
        let needed = OPUS_FRAME_SAMPLES * ch;
        if self.pcm_i16_scratch.len() < needed {
            self.pcm_i16_scratch.resize(needed, 0);
        }

        if let Some(prev) = self.last_seq {
            let diff = seq.wrapping_sub(prev);
            if diff == 0 || diff > 32768 {
                // Duplicate or late out-of-order packet -> drop
                return out_bytes;
            }
            // Conceal up to 3 lost packets via Opus PLC
            let lost = (diff - 1).min(3);
            for _ in 0..lost {
                if let Ok(samples) =
                    self.decoder
                        .decode_i16(&[], OPUS_FRAME_SAMPLES, &mut self.pcm_i16_scratch)
                {
                    for &s in &self.pcm_i16_scratch[..samples * ch] {
                        out_bytes.extend_from_slice(&s.to_le_bytes());
                    }
                }
            }
        }
        self.last_seq = Some(seq);

        if let Ok(samples) =
            self.decoder
                .decode_i16(opus_bytes, OPUS_FRAME_SAMPLES, &mut self.pcm_i16_scratch)
        {
            out_bytes.reserve(samples * ch * 2);
            for &s in &self.pcm_i16_scratch[..samples * ch] {
                out_bytes.extend_from_slice(&s.to_le_bytes());
            }
        }
        out_bytes
    }
}

/// Builds a framed `DSKY` Opus audio/voice packet (`PKT_AUDIO_OPUS` or `PKT_VOICE_OPUS`).
pub fn build_opus_packet(
    opcode: u8,
    channels: u8,
    seq: u16,
    sample_rate: u32,
    opus_frame: &[u8],
) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(12 + opus_frame.len());
    pkt.extend_from_slice(crate::protocol::MAGIC_DSKY);
    pkt.push(opcode);
    pkt.push(channels);
    pkt.extend_from_slice(&seq.to_le_bytes());
    pkt.extend_from_slice(&sample_rate.to_le_bytes());
    pkt.extend_from_slice(opus_frame);
    pkt
}

/// Decodes a `PKT_AUDIO_OPUS` or `PKT_VOICE_OPUS` payload using a shared decoder slot.
/// Returns `(sample_rate, channels, pcm_bytes)` on success.
pub fn decode_opus_packet_with_slot(
    slot: &std::sync::Mutex<Option<OpusAudioDecoder>>,
    slice: &[u8],
) -> Option<(u32, u16, Vec<u8>)> {
    if slice.len() <= 12 {
        return None;
    }
    let channels = (slice[5] as u16).clamp(1, 2);
    let seq = u16::from_le_bytes(slice[6..8].try_into().ok()?);
    let sample_rate = u32::from_le_bytes(slice[8..12].try_into().ok()?);
    let opus_payload = &slice[12..];

    let mut guard = slot.lock().ok()?;
    let need_init = guard
        .as_ref()
        .map(|d| d.channels() != channels)
        .unwrap_or(true);
    if need_init {
        *guard = OpusAudioDecoder::new(channels).ok();
    }
    let pcm = guard.as_mut()?.decode_packet(seq, opus_payload);
    if pcm.is_empty() {
        None
    } else {
        Some((sample_rate, channels, pcm))
    }
}
