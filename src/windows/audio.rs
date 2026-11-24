//! Real-Time Low-Latency WASAPI Loopback Audio Engine
//!
//! Captures system audio loopback directly from host speakers with < 20ms latency
//! and streams compact 16-bit PCM chunks over P2P. Client plays audio with zero drivers.

#![allow(unsafe_op_in_unsafe_fn)]

use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU8, Ordering};
use std::sync::Mutex;
use tracing::info;

use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eMultimedia, eRender, waveOutClose, waveOutOpen,
    waveOutPrepareHeader, waveOutReset, waveOutUnprepareHeader, waveOutWrite, HWAVEOUT,
    IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, MIDI_WAVE_OPEN_TYPE, WAVEFORMATEX,
    WAVEHDR, WAVE_FORMAT_PCM,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};

pub static CLIENT_AUDIO_MUTED: AtomicBool = AtomicBool::new(false);
pub static REMOTE_AUDIO_VOLUME_PCT: AtomicU8 = AtomicU8::new(100);
pub static NOISE_GATE_THRESHOLD: AtomicU16 = AtomicU16::new(40);

pub fn set_audio_volume_pct(pct: u8) {
    REMOTE_AUDIO_VOLUME_PCT.store(pct.min(100), Ordering::Relaxed);
}

pub fn get_audio_volume_pct() -> u8 {
    REMOTE_AUDIO_VOLUME_PCT.load(Ordering::Relaxed)
}

pub fn set_noise_gate_threshold(threshold: u16) {
    NOISE_GATE_THRESHOLD.store(threshold, Ordering::Relaxed);
}

pub fn get_noise_gate_threshold() -> u16 {
    NOISE_GATE_THRESHOLD.load(Ordering::Relaxed)
}

static AUDIO_PLAYER: Mutex<Option<AudioPlayer>> = Mutex::new(None);

/// Ensures Windows Audio (`Audiosrv`) and Audio Endpoint Builder services are running.
/// On Windows Server editions (2012, 2016, 2019, 2022), these services are often stopped/disabled by default.
pub fn ensure_windows_audio_services() {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    std::thread::spawn(|| {
        let _ = Command::new("sc")
            .args(["config", "AudioEndpointBuilder", "start=", "auto"])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
        let _ = Command::new("net")
            .args(["start", "AudioEndpointBuilder"])
            .creation_flags(CREATE_NO_WINDOW)
            .status();

        let _ = Command::new("sc")
            .args(["config", "Audiosrv", "start=", "auto"])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
        let _ = Command::new("net")
            .args(["start", "Audiosrv"])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    });
}

pub struct WasapiLoopbackCapturer {
    client: IAudioClient,
    capture_client: IAudioCaptureClient,
    pub channels: u16,
    pub sample_rate: u32,
    pub is_float: bool,
}

impl WasapiLoopbackCapturer {
    pub fn new() -> Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            
            // Try eMultimedia first (media players, browsers, YouTube), fallback to eConsole
            let device: IMMDevice = match enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) {
                Ok(d) => d,
                Err(_) => match enumerator.GetDefaultAudioEndpoint(eRender, eConsole) {
                    Ok(d) => d,
                    Err(e) => {
                        ensure_windows_audio_services();
                        return Err(e.into());
                    }
                },
            };

            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;

            let mix_format_ptr = client.GetMixFormat()?;
            let mix_format = &*mix_format_ptr;

            let sample_rate = mix_format.nSamplesPerSec;
            let channels = mix_format.nChannels;
            let is_float = mix_format.wFormatTag == 3 || mix_format.wBitsPerSample == 32;

            // 60ms internal ring buffer prevents WASAPI discontinuities on OS scheduling hiccups
            let buffer_duration_hns = 600_000i64; // 60ms in 100ns units

            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                buffer_duration_hns,
                0,
                mix_format_ptr,
                None,
            )?;

            let capture_client: IAudioCaptureClient = client.GetService()?;
            client.Start()?;

            info!(
                "WASAPI Audio Loopback active: {}Hz, {} channels (float={})",
                sample_rate, channels, is_float
            );

            Ok(Self {
                client,
                capture_client,
                channels,
                sample_rate,
                is_float,
            })
        }
    }

    /// Reads next available captured PCM audio packet converted to standard 16-bit PCM
    pub fn read_packet(&self) -> Result<Option<Vec<u8>>> {
        unsafe {
            let mut data_ptr = std::ptr::null_mut();
            let mut num_frames_read = 0u32;
            let mut flags = 0u32;

            let res = self.capture_client.GetBuffer(
                &mut data_ptr,
                &mut num_frames_read,
                &mut flags,
                None,
                None,
            );

            if res.is_err() || num_frames_read == 0 || data_ptr.is_null() {
                return Ok(None);
            }

            // WASAPI Flag 0x2 = AUDCLNT_BUFFERFLAGS_SILENT
            if flags & 0x2 != 0 {
                let _ = self.capture_client.ReleaseBuffer(num_frames_read);
                return Ok(None);
            }

            let num_samples = num_frames_read as usize * self.channels as usize;
            let mut pcm16_bytes = Vec::with_capacity(num_samples * 2);
            let mut max_amp: i16 = 0;

            if self.is_float {
                let float_slice = std::slice::from_raw_parts(data_ptr as *const f32, num_samples);
                for &s in float_slice {
                    if s.is_nan() || s.is_infinite() {
                        pcm16_bytes.extend_from_slice(&0i16.to_le_bytes());
                        continue;
                    }
                    let clamped = s.clamp(-1.0, 1.0);
                    let val = (clamped * 32767.0) as i16;
                    let abs_val = val.saturating_abs();
                    if abs_val > max_amp {
                        max_amp = abs_val;
                    }
                    pcm16_bytes.extend_from_slice(&val.to_le_bytes());
                }
            } else {
                let i16_slice = std::slice::from_raw_parts(data_ptr as *const i16, num_samples);
                for &val in i16_slice {
                    let abs_val = val.saturating_abs();
                    if abs_val > max_amp {
                        max_amp = abs_val;
                    }
                    pcm16_bytes.extend_from_slice(&val.to_le_bytes());
                }
            }

            let _ = self.capture_client.ReleaseBuffer(num_frames_read);

            // Noise gate: if peak amplitude is below configured threshold,
            // treat as silence to eliminate background hiss while capturing speech/video
            let gate = NOISE_GATE_THRESHOLD.load(Ordering::Relaxed) as i16;
            if gate > 0 && max_amp < gate {
                return Ok(None);
            }

            Ok(Some(pcm16_bytes))
        }
    }
}

impl Drop for WasapiLoopbackCapturer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
            CoUninitialize();
        }
    }
}

pub struct WasapiMicCapturer {
    client: IAudioClient,
    capture_client: IAudioCaptureClient,
    pub channels: u16,
    pub sample_rate: u32,
    pub is_float: bool,
}

impl WasapiMicCapturer {
    pub fn new() -> Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

            // Try eCommunications first (headset / mic), fallback to eConsole
            let device: IMMDevice = match enumerator.GetDefaultAudioEndpoint(eCapture, eCommunications) {
                Ok(d) => d,
                Err(_) => match enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) {
                    Ok(d) => d,
                    Err(e) => {
                        ensure_windows_audio_services();
                        return Err(e.into());
                    }
                },
            };

            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;

            let mix_format_ptr = client.GetMixFormat()?;
            let mix_format = &*mix_format_ptr;

            let sample_rate = mix_format.nSamplesPerSec;
            let channels = mix_format.nChannels;
            let is_float = mix_format.wFormatTag == 3 || mix_format.wBitsPerSample == 32;

            let buffer_duration_hns = 200_000i64; // 20ms in 100ns units

            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                0,
                buffer_duration_hns,
                0,
                mix_format_ptr,
                None,
            )?;

            let capture_client: IAudioCaptureClient = client.GetService()?;
            client.Start()?;

            info!(
                "WASAPI Microphone Capture active: {}Hz, {} channels (float={})",
                sample_rate, channels, is_float
            );

            Ok(Self {
                client,
                capture_client,
                channels,
                sample_rate,
                is_float,
            })
        }
    }

    pub fn read_packet(&self) -> Result<Option<Vec<u8>>> {
        unsafe {
            let mut data_ptr = std::ptr::null_mut();
            let mut num_frames_read = 0u32;
            let mut flags = 0u32;

            let res = self.capture_client.GetBuffer(
                &mut data_ptr,
                &mut num_frames_read,
                &mut flags,
                None,
                None,
            );

            if res.is_err() || num_frames_read == 0 || data_ptr.is_null() {
                return Ok(None);
            }

            // WASAPI Flag 0x2 = AUDCLNT_BUFFERFLAGS_SILENT
            if flags & 0x2 != 0 {
                let _ = self.capture_client.ReleaseBuffer(num_frames_read);
                return Ok(None);
            }

            let num_samples = num_frames_read as usize * self.channels as usize;
            let mut pcm16_bytes = Vec::with_capacity(num_samples * 2);
            let mut max_amp: i16 = 0;

            if self.is_float {
                let float_slice = std::slice::from_raw_parts(data_ptr as *const f32, num_samples);
                for &s in float_slice {
                    if s.is_nan() || s.is_infinite() {
                        pcm16_bytes.extend_from_slice(&0i16.to_le_bytes());
                        continue;
                    }
                    let clamped = s.clamp(-1.0, 1.0);
                    let val = (clamped * 32767.0) as i16;
                    let abs_val = val.saturating_abs();
                    if abs_val > max_amp {
                        max_amp = abs_val;
                    }
                    pcm16_bytes.extend_from_slice(&val.to_le_bytes());
                }
            } else {
                let i16_slice = std::slice::from_raw_parts(data_ptr as *const i16, num_samples);
                for &val in i16_slice {
                    let abs_val = val.saturating_abs();
                    if abs_val > max_amp {
                        max_amp = abs_val;
                    }
                    pcm16_bytes.extend_from_slice(&val.to_le_bytes());
                }
            }

            let _ = self.capture_client.ReleaseBuffer(num_frames_read);

            // Adaptive noise gate for microphone: ignore ambient line noise / room hiss below 60
            if max_amp < 60 {
                return Ok(None);
            }

            Ok(Some(pcm16_bytes))
        }
    }
}

impl Drop for WasapiMicCapturer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
            CoUninitialize();
        }
    }
}

struct AudioBlock {
    header: WAVEHDR,
    buffer: Vec<u8>,
    prepared: bool,
}

pub struct AudioPlayer {
    h_wave_out: HWAVEOUT,
    blocks: Vec<Box<AudioBlock>>,
    pending_pcm: Vec<u8>,
    last_push: std::time::Instant,
    pub sample_rate: u32,
    pub channels: u16,
}

unsafe impl Send for AudioPlayer {}
unsafe impl Sync for AudioPlayer {}

impl AudioPlayer {
    pub fn new(sample_rate: u32, channels: u16) -> Result<Self> {
        unsafe {
            let wfx = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_PCM as u16,
                nChannels: channels,
                nSamplesPerSec: sample_rate,
                nAvgBytesPerSec: sample_rate * channels as u32 * 2,
                nBlockAlign: channels * 2,
                wBitsPerSample: 16,
                cbSize: 0,
            };

            let mut h_wave_out = HWAVEOUT::default();
            let res = waveOutOpen(
                Some(&mut h_wave_out),
                0xFFFFFFFF, // WAVE_MAPPER
                &wfx,
                None,
                None,
                MIDI_WAVE_OPEN_TYPE(0),
            );

            if res != 0 {
                anyhow::bail!("Failed to open waveOut device: {}", res);
            }

            // Pre-allocate 16 reusable audio blocks with 16KB buffers
            let mut blocks = Vec::with_capacity(16);
            for _ in 0..16 {
                blocks.push(Box::new(AudioBlock {
                    header: WAVEHDR::default(),
                    buffer: vec![0u8; 16384],
                    prepared: false,
                }));
            }

            Ok(Self {
                h_wave_out,
                blocks,
                pending_pcm: Vec::with_capacity(16384),
                last_push: std::time::Instant::now(),
                sample_rate,
                channels,
            })
        }
    }

    pub fn play_chunk(&mut self, pcm_bytes: &[u8]) {
        if pcm_bytes.is_empty() || CLIENT_AUDIO_MUTED.load(Ordering::Relaxed) {
            return;
        }

        let frame_align = (self.channels as usize * 2).max(2);
        // Coalesce small MTU-sized network packets into ~20ms waveOut blocks to avoid micro-stutters
        let target_block_bytes = ((self.sample_rate as usize * frame_align) / 50).max(frame_align);

        // If there was a long silence gap (> 120ms), discard stale partial buffer
        if self.last_push.elapsed().as_millis() > 120 {
            self.pending_pcm.clear();
        }
        self.last_push = std::time::Instant::now();
        self.pending_pcm.extend_from_slice(pcm_bytes);

        // Cap pending buffer at ~120ms to prevent latency buildup
        let max_pending = target_block_bytes * 6;
        if self.pending_pcm.len() > max_pending {
            let drop_bytes = ((self.pending_pcm.len() - max_pending) / frame_align) * frame_align;
            self.pending_pcm.drain(..drop_bytes);
        }

        if self.pending_pcm.len() < target_block_bytes {
            return;
        }

        let flush_len = (self.pending_pcm.len() / frame_align) * frame_align;
        if flush_len == 0 {
            return;
        }

        unsafe {
            // 1. Reclaim any completed blocks
            let mut active_blocks = 0usize;
            for block in &mut self.blocks {
                if block.prepared {
                    if block.header.dwFlags & 1 != 0 {
                        // WHDR_DONE
                        let _ = waveOutUnprepareHeader(
                            self.h_wave_out,
                            &mut block.header,
                            std::mem::size_of::<WAVEHDR>() as u32,
                        );
                        block.prepared = false;
                    } else {
                        active_blocks += 1;
                    }
                }
            }

            // Prevent unbounded latency (> ~240ms queued) by dropping excess if queue is full
            if active_blocks >= 12 {
                self.pending_pcm.drain(..flush_len);
                return;
            }

            // 2. Find an available (unprepared) block
            let mut target_idx = None;
            for (idx, block) in self.blocks.iter().enumerate() {
                if !block.prepared {
                    target_idx = Some(idx);
                    break;
                }
            }

            if target_idx.is_none() && self.blocks.len() < 24 {
                self.blocks.push(Box::new(AudioBlock {
                    header: WAVEHDR::default(),
                    buffer: vec![0u8; flush_len.max(8192)],
                    prepared: false,
                }));
                target_idx = Some(self.blocks.len() - 1);
            }

            let Some(block_idx) = target_idx else {
                self.pending_pcm.drain(..flush_len);
                return;
            };

            let block = &mut self.blocks[block_idx];
            if block.buffer.len() < flush_len {
                block.buffer.resize(flush_len, 0);
            }
            block.buffer[..flush_len].copy_from_slice(&self.pending_pcm[..flush_len]);
            let vol = REMOTE_AUDIO_VOLUME_PCT.load(Ordering::Relaxed);
            if vol < 100 {
                for sample in block.buffer[..flush_len].chunks_exact_mut(2) {
                    let s = i16::from_le_bytes([sample[0], sample[1]]);
                    let scaled = ((s as i32 * vol as i32) / 100) as i16;
                    sample.copy_from_slice(&scaled.to_le_bytes());
                }
            }
            self.pending_pcm.drain(..flush_len);

            block.header.lpData = windows::core::PSTR(block.buffer.as_mut_ptr());
            block.header.dwBufferLength = flush_len as u32;
            block.header.dwBytesRecorded = 0;
            block.header.dwUser = 0;
            block.header.dwFlags = 0;
            block.header.dwLoops = 0;

            let prep = waveOutPrepareHeader(
                self.h_wave_out,
                &mut block.header,
                std::mem::size_of::<WAVEHDR>() as u32,
            );
            if prep == 0 {
                block.prepared = true;
                let _ = waveOutWrite(
                    self.h_wave_out,
                    &mut block.header,
                    std::mem::size_of::<WAVEHDR>() as u32,
                );
            }
        }
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        unsafe {
            let _ = waveOutReset(self.h_wave_out);
            for block in &mut self.blocks {
                if block.prepared {
                    let _ = waveOutUnprepareHeader(
                        self.h_wave_out,
                        &mut block.header,
                        std::mem::size_of::<WAVEHDR>() as u32,
                    );
                    block.prepared = false;
                }
            }
            let _ = waveOutClose(self.h_wave_out);
        }
    }
}

/// Plays a received remote audio chunk on the client machine
pub fn play_remote_audio_chunk(sample_rate: u32, channels: u16, pcm_bytes: &[u8]) {
    if CLIENT_AUDIO_MUTED.load(Ordering::Relaxed) {
        return;
    }

    let mut guard = AUDIO_PLAYER.lock().unwrap();
    let need_reopen = match guard.as_ref() {
        Some(p) => p.sample_rate != sample_rate || p.channels != channels,
        None => true,
    };

    if need_reopen {
        *guard = None;
        if let Ok(player) = AudioPlayer::new(sample_rate, channels) {
            *guard = Some(player);
        }
    }

    if let Some(player) = guard.as_mut() {
        player.play_chunk(pcm_bytes);
    }
}

/// Toggles client remote audio mute state
pub fn toggle_audio_mute() -> bool {
    let prev = CLIENT_AUDIO_MUTED.fetch_xor(true, Ordering::SeqCst);
    !prev
}

pub fn is_audio_muted() -> bool {
    CLIENT_AUDIO_MUTED.load(Ordering::Relaxed)
}

static VOICE_PLAYER: Mutex<Option<AudioPlayer>> = Mutex::new(None);

/// Plays a received remote voice chunk on the peer machine (independent from system audio)
pub fn play_remote_voice_chunk(sample_rate: u32, channels: u16, pcm_bytes: &[u8]) {
    let mut guard = VOICE_PLAYER.lock().unwrap();
    let need_reopen = match guard.as_ref() {
        Some(p) => p.sample_rate != sample_rate || p.channels != channels,
        None => true,
    };

    if need_reopen {
        *guard = None;
        if let Ok(player) = AudioPlayer::new(sample_rate, channels) {
            *guard = Some(player);
        }
    }

    if let Some(player) = guard.as_mut() {
        player.play_chunk(pcm_bytes);
    }
}

pub static INTERCOM_VOICE_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static HOST_MIC_ACTIVE: AtomicBool = AtomicBool::new(false);
static MIC_THREAD_SPAWNED: AtomicBool = AtomicBool::new(false);

/// Toggles technician two-way voice intercom stream state
pub fn toggle_intercom_voice() -> bool {
    let prev = INTERCOM_VOICE_ACTIVE.fetch_xor(true, Ordering::SeqCst);
    let now = !prev;
    if now {
        start_mic_capture_loop();
    }
    now
}

pub fn is_intercom_voice_active() -> bool {
    INTERCOM_VOICE_ACTIVE.load(Ordering::Relaxed)
}

/// Toggles host floating widget microphone state
pub fn toggle_host_mic() -> bool {
    let prev = HOST_MIC_ACTIVE.fetch_xor(true, Ordering::SeqCst);
    let now = !prev;
    if now {
        start_mic_capture_loop();
    }
    now
}

pub fn is_host_mic_active() -> bool {
    HOST_MIC_ACTIVE.load(Ordering::Relaxed)
}

/// Starts the background microphone capture worker thread that samples PCM
/// whenever either technician or host microphone is active.
pub fn start_mic_capture_loop() {
    if MIC_THREAD_SPAWNED.swap(true, Ordering::SeqCst) {
        return;
    }

    std::thread::spawn(|| {
        let mut capturer_opt: Option<WasapiMicCapturer> = None;

        loop {
            let active = INTERCOM_VOICE_ACTIVE.load(Ordering::Relaxed) || HOST_MIC_ACTIVE.load(Ordering::Relaxed);
            if !active {
                if capturer_opt.is_some() {
                    capturer_opt = None; // Release microphone device when unmuted
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
                continue;
            }

            if capturer_opt.is_none() {
                match WasapiMicCapturer::new() {
                    Ok(c) => capturer_opt = Some(c),
                    Err(e) => {
                        tracing::warn!("Failed to initialize WASAPI mic capture: {}", e);
                        std::thread::sleep(std::time::Duration::from_secs(1));
                        continue;
                    }
                }
            }

            if let Some(capturer) = capturer_opt.as_ref() {
                match capturer.read_packet() {
                    Ok(Some(pcm)) => {
                        crate::p2p::push_outgoing_voice_chunk(
                            capturer.channels as u8,
                            capturer.sample_rate,
                            &pcm,
                        );
                    }
                    Ok(None) => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(e) => {
                        tracing::warn!("WASAPI mic capture read error: {}", e);
                        capturer_opt = None;
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }
                }
            }
        }
    });
}
