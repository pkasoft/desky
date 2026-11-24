//! Linux Desktop Audio Capture & Playback (PulseAudio / PipeWire)
//!
//! Provides zero-C-dependency 48kHz stereo S16LE audio streaming on Linux
//! via `parec` / `pw-record` (capture) and `pacat` / `pw-play` (playback).

use std::io::{Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

pub static CLIENT_AUDIO_MUTED: AtomicBool = AtomicBool::new(false);
pub static INTERCOM_VOICE_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static HOST_MIC_ACTIVE: AtomicBool = AtomicBool::new(false);
static MIC_THREAD_SPAWNED: AtomicBool = AtomicBool::new(false);

pub struct LinuxAudioCapturer {
    child: Child,
    stdout: ChildStdout,
}

impl LinuxAudioCapturer {
    pub fn start() -> anyhow::Result<Self> {
        // Prefer PulseAudio monitor capture (`parec`), fallback to PipeWire (`pw-record`)
        let child_res = Command::new("parec")
            .args([
                "--format=s16le",
                "--rate=48000",
                "--channels=2",
                "--latency-msec=20",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .or_else(|_| {
                Command::new("pw-record")
                    .args(["--format=s16", "--rate=48000", "--channels=2", "-"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
            });

        let mut child = child_res?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Audio capturer stdout unavailable"))?;
        Ok(Self { child, stdout })
    }

    pub fn read_pcm_chunk(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.stdout.read(buf)
    }
}

impl Drop for LinuxAudioCapturer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct LinuxAudioPlayer {
    child: Child,
    stdin: ChildStdin,
}

impl LinuxAudioPlayer {
    pub fn start() -> anyhow::Result<Self> {
        let child_res = Command::new("pacat")
            .args([
                "--format=s16le",
                "--rate=48000",
                "--channels=2",
                "--latency-msec=20",
            ])
            .stdin(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .or_else(|_| {
                Command::new("pw-play")
                    .args(["--format=s16", "--rate=48000", "--channels=2", "-"])
                    .stdin(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
            });

        let mut child = child_res?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Audio player stdin unavailable"))?;
        Ok(Self { child, stdin })
    }

    pub fn play_pcm_chunk(&mut self, pcm: &[u8]) -> std::io::Result<()> {
        self.stdin.write_all(pcm)
    }
}

impl Drop for LinuxAudioPlayer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

static LINUX_AUDIO_PLAYER: OnceLock<Mutex<Option<LinuxAudioPlayer>>> = OnceLock::new();
static LINUX_VOICE_PLAYER: OnceLock<Mutex<Option<LinuxAudioPlayer>>> = OnceLock::new();

fn play_into_player(slot: &OnceLock<Mutex<Option<LinuxAudioPlayer>>>, pcm: &[u8]) {
    if pcm.is_empty() {
        return;
    }
    let store = slot.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = store.lock() {
        if guard.is_none() {
            *guard = LinuxAudioPlayer::start().ok();
        }
        if let Some(ref mut player) = *guard {
            if player.play_pcm_chunk(pcm).is_err() {
                *guard = None;
            }
        }
    }
}

pub fn play_remote_audio_chunk(pcm: &[u8]) {
    if CLIENT_AUDIO_MUTED.load(Ordering::Relaxed) {
        return;
    }
    play_into_player(&LINUX_AUDIO_PLAYER, pcm);
}

pub fn play_remote_voice_chunk(pcm: &[u8]) {
    play_into_player(&LINUX_VOICE_PLAYER, pcm);
}

pub fn toggle_audio_mute() -> bool {
    let prev = CLIENT_AUDIO_MUTED.fetch_xor(true, Ordering::SeqCst);
    !prev
}

pub fn is_audio_muted() -> bool {
    CLIENT_AUDIO_MUTED.load(Ordering::Relaxed)
}

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

pub fn start_mic_capture_loop() {
    if MIC_THREAD_SPAWNED.swap(true, Ordering::SeqCst) {
        return;
    }

    std::thread::spawn(|| {
        let mut capturer_opt: Option<LinuxAudioCapturer> = None;
        let mut pcm_buf = [0u8; 3840]; // 20 ms @ 48 kHz stereo S16LE

        loop {
            let active = INTERCOM_VOICE_ACTIVE.load(Ordering::Relaxed)
                || HOST_MIC_ACTIVE.load(Ordering::Relaxed);
            if !active {
                if capturer_opt.is_some() {
                    capturer_opt = None;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
                continue;
            }

            if capturer_opt.is_none() {
                match LinuxAudioCapturer::start() {
                    Ok(c) => capturer_opt = Some(c),
                    Err(_) => {
                        std::thread::sleep(std::time::Duration::from_secs(1));
                        continue;
                    }
                }
            }

            if let Some(ref mut capturer) = capturer_opt {
                match capturer.read_pcm_chunk(&mut pcm_buf) {
                    Ok(n) if n > 0 => {
                        crate::p2p::push_outgoing_voice_chunk(2, 48000, &pcm_buf[..n]);
                    }
                    _ => {
                        capturer_opt = None;
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }
                }
            }
        }
    });
}
