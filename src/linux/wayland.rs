//! Linux Wayland Session Detection & PipeWire / Portal Diagnostics
//!
//! Detects native Wayland sessions (`WAYLAND_DISPLAY` / `XDG_SESSION_TYPE`),
//! probes `PipeWire` and `xdg-desktop-portal` availability, and checks XWayland
//! compatibility for hybrid desktop environments (Ubuntu, Fedora, Arch).

#![allow(dead_code)]

use std::process::Command;

#[derive(Debug, Clone)]
pub struct WaylandCapabilities {
    pub is_wayland: bool,
    pub has_xwayland: bool,
    pub has_pipewire: bool,
    pub has_portal: bool,
}

pub fn is_wayland_session() -> bool {
    let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let wayland_disp = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
    session_type.eq_ignore_ascii_case("wayland") || !wayland_disp.is_empty()
}

pub fn probe_wayland_capabilities() -> WaylandCapabilities {
    let is_wayland = is_wayland_session();
    let has_xwayland = !std::env::var("DISPLAY").unwrap_or_default().is_empty();
    let has_pipewire = Command::new("which")
        .arg("pw-record")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let has_portal = std::path::Path::new("/usr/libexec/xdg-desktop-portal").exists()
        || std::path::Path::new("/usr/lib/xdg-desktop-portal").exists()
        || Command::new("which")
            .arg("xdg-desktop-portal")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

    WaylandCapabilities {
        is_wayland,
        has_xwayland,
        has_pipewire,
        has_portal,
    }
}
