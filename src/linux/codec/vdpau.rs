//! Linux VDPAU / NVIDIA GPU Hardware Video Acceleration Probe
//!
//! Checks whether the Linux system possesses NVIDIA proprietary driver hardware
//! video acceleration via VDPAU (`libvdpau.so.1`) or CUDA / NVDEC (`libcuda.so.1`).
//!
//! Provides dynamic runtime symbol resolution with zero compile-time dependencies,
//! working across all major distributions (Debian, Ubuntu, Fedora, RHEL, Arch, openSUSE).

use std::ffi::CStr;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::debug;

static VDPAU_CHECKED: AtomicBool = AtomicBool::new(false);
static VDPAU_AVAILABLE: AtomicBool = AtomicBool::new(false);

const VDPAU_CANDIDATES: &[&CStr] = &[
    c"libvdpau.so.1",
    c"libvdpau.so",
    c"libcuda.so.1",
    c"libcuda.so",
];

/// Probes whether NVIDIA hardware video acceleration (VDPAU / NVDEC) is installed on this Linux system
pub fn is_vdpau_hardware_available() -> bool {
    if VDPAU_CHECKED.load(Ordering::Relaxed) {
        return VDPAU_AVAILABLE.load(Ordering::Relaxed);
    }

    let mut found = false;
    unsafe {
        for &name in VDPAU_CANDIDATES {
            let handle = libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if !handle.is_null() {
                debug!("Linux NVIDIA GPU driver detected: {}", name.to_string_lossy());
                libc::dlclose(handle);
                found = true;
                break;
            }
        }
    }

    VDPAU_AVAILABLE.store(found, Ordering::Relaxed);
    VDPAU_CHECKED.store(true, Ordering::Relaxed);
    found
}
