//! Real-Time Low-Latency WASAPI Loopback Audio Engine
//!
//! Captures system audio loopback directly from host speakers with < 20ms latency
//! and streams compact 16-bit PCM chunks over P2P. Client plays audio with zero drivers.

#![allow(unsafe_op_in_unsafe_fn)]

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
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

            // 20ms buffer duration for ultra-low latency
            let buffer_duration_hns = 200_000i64; // 20ms in 100ns units

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

            // Noise gate: if peak amplitude is below line-noise threshold (40 / -58dB),
            // treat as silence to eliminate background hiss while capturing quiet speech and video playback
            if max_amp < 40 {
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
                sample_rate,
                channels,
            })
        }
    }

    pub fn play_chunk(&mut self, pcm_bytes: &[u8]) {
        if pcm_bytes.is_empty() || CLIENT_AUDIO_MUTED.load(Ordering::Relaxed) {
            return;
        }

        unsafe {
            // 1. Reclaim any completed blocks
            for block in &mut self.blocks {
                if block.prepared && (block.header.dwFlags & 1 != 0) {
                    // WHDR_DONE
                    let _ = waveOutUnprepareHeader(
                        self.h_wave_out,
                        &mut block.header,
                        std::mem::size_of::<WAVEHDR>() as u32,
                    );
                    block.prepared = false;
                }
            }

            // 2. Find an available (unprepared) block or allocate one if pool is not saturated
            let mut target_idx = None;
            for (idx, block) in self.blocks.iter().enumerate() {
                if !block.prepared {
                    target_idx = Some(idx);
                    break;
                }
            }

            if target_idx.is_none() && self.blocks.len() < 32 {
                self.blocks.push(Box::new(AudioBlock {
                    header: WAVEHDR::default(),
                    buffer: vec![0u8; pcm_bytes.len().max(8192)],
                    prepared: false,
                }));
                target_idx = Some(self.blocks.len() - 1);
            }

            // If still saturated, reclaim the oldest block to prevent buffer queue buildup
            let block_idx = match target_idx {
                Some(i) => i,
                None => {
                    let first = &mut self.blocks[0];
                    if first.prepared {
                        let _ = waveOutUnprepareHeader(
                            self.h_wave_out,
                            &mut first.header,
                            std::mem::size_of::<WAVEHDR>() as u32,
                        );
                        first.prepared = false;
                    }
                    0
                }
            };

            let block = &mut self.blocks[block_idx];
            if block.buffer.len() < pcm_bytes.len() {
                block.buffer.resize(pcm_bytes.len(), 0);
            }
            block.buffer[..pcm_bytes.len()].copy_from_slice(pcm_bytes);

            block.header.lpData = windows::core::PSTR(block.buffer.as_mut_ptr());
            block.header.dwBufferLength = pcm_bytes.len() as u32;
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
