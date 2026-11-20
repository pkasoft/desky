//! Linux Desktop Audio Capture & Playback (PulseAudio / PipeWire)
//!
//! Provides zero-C-dependency 48kHz stereo S16LE audio streaming on Linux
//! via `parec` / `pw-record` (capture) and `pacat` / `pw-play` (playback).

use std::io::{Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Mutex, OnceLock};

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

pub fn play_remote_audio_chunk(pcm: &[u8]) {
    if pcm.is_empty() {
        return;
    }
    let store = LINUX_AUDIO_PLAYER.get_or_init(|| Mutex::new(None));
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
