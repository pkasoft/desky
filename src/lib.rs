pub mod codec;
pub mod config;
pub mod identity;
pub mod media;
pub mod p2p;
pub mod protocol;
pub use media::recorder;
pub mod ui;

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "windows")]
pub use windows as platform;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub use linux as platform;
