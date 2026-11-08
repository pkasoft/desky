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
    eConsole, eRender, waveOutClose, waveOutOpen, waveOutPrepareHeader,
    waveOutUnprepareHeader, waveOutWrite, HWAVEOUT, IAudioCaptureClient, IAudioClient,
    IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_LOOPBACK, MIDI_WAVE_OPEN_TYPE, WAVEFORMATEX, WAVEHDR,
    WAVE_FORMAT_PCM,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};

pub static CLIENT_AUDIO_MUTED: AtomicBool = AtomicBool::new(false);

static AUDIO_PLAYER: Mutex<Option<AudioPlayer>> = Mutex::new(None);

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
            let device: IMMDevice = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;

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

            let num_samples = num_frames_read as usize * self.channels as usize;
            let mut pcm16_bytes = Vec::with_capacity(num_samples * 2);

            if flags & 1 != 0 {
                // Silence
                pcm16_bytes.resize(num_samples * 2, 0);
            } else if self.is_float {
                let float_slice = std::slice::from_raw_parts(data_ptr as *const f32, num_samples);
                for &s in float_slice {
                    let clamped = s.clamp(-1.0, 1.0);
                    let val = (clamped * 32767.0) as i16;
                    pcm16_bytes.extend_from_slice(&val.to_le_bytes());
                }
            } else {
                let byte_count = num_samples * 2;
                let slice = std::slice::from_raw_parts(data_ptr, byte_count);
                pcm16_bytes.extend_from_slice(slice);
            }

            let _ = self.capture_client.ReleaseBuffer(num_frames_read);
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

pub struct AudioPlayer {
    h_wave_out: HWAVEOUT,
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

            Ok(Self { h_wave_out })
        }
    }

    pub fn play_chunk(&self, pcm_bytes: &[u8]) {
        if pcm_bytes.is_empty() || CLIENT_AUDIO_MUTED.load(Ordering::Relaxed) {
            return;
        }

        unsafe {
            let mut header = Box::new(WAVEHDR {
                lpData: windows::core::PSTR(pcm_bytes.as_ptr() as *mut u8),
                dwBufferLength: pcm_bytes.len() as u32,
                ..Default::default()
            });

            let prep = waveOutPrepareHeader(
                self.h_wave_out,
                header.as_mut(),
                std::mem::size_of::<WAVEHDR>() as u32,
            );
            if prep == 0 {
                let _ = waveOutWrite(
                    self.h_wave_out,
                    header.as_mut(),
                    std::mem::size_of::<WAVEHDR>() as u32,
                );
                let h_raw = self.h_wave_out.0 as usize;
                let hdr_ptr = Box::into_raw(header) as usize;
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(60));
                    let h_out = HWAVEOUT(h_raw as *mut _);
                    let mut hdr = Box::from_raw(hdr_ptr as *mut WAVEHDR);
                    let _ = waveOutUnprepareHeader(h_out, hdr.as_mut(), std::mem::size_of::<WAVEHDR>() as u32);
                });
            }
        }
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        unsafe {
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
    if guard.is_none() {
        if let Ok(player) = AudioPlayer::new(sample_rate, channels) {
            *guard = Some(player);
        }
    }

    if let Some(player) = guard.as_ref() {
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

pub static INTERCOM_VOICE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Toggles technician two-way voice intercom stream state
pub fn toggle_intercom_voice() -> bool {
    let prev = INTERCOM_VOICE_ACTIVE.fetch_xor(true, Ordering::SeqCst);
    !prev
}

pub fn is_intercom_voice_active() -> bool {
    INTERCOM_VOICE_ACTIVE.load(Ordering::Relaxed)
}
