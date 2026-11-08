//! Windows OS Platform Modules & Runtime Architecture
//!
//! Subdivided by OS family:
//! - `win10_11`: Windows 10 & Windows 11
//! - `server_2022`: Windows Server 2022 & 2025
//! - `server_2016_2019`: Windows Server 2016 & 2019
//! - `server_2012`: Windows Server 2012 & 2012 R2
//! - `win7_8`: Windows 7, Windows 8 & 8.1

pub mod admin;
pub mod capture;
pub mod clipboard;
pub mod input;
pub mod server_2012;
pub mod server_2016_2019;
pub mod server_2022;
pub mod system;
pub mod ui;
pub mod win10_11;
pub mod win7_8;

pub use capture::capture_screen_frame;
pub use clipboard::{
    is_clipboard_recently_synced, mark_clipboard_synced, read_clipboard_files,
    read_clipboard_text, write_clipboard_files, write_clipboard_text,
};
pub use input::InputInjector;
pub use system::{get_desktop_dir, get_identity_dir, get_machine_id, notify_file_received};
pub use ui::set_assigned_desky_id;

use std::sync::OnceLock;
use tracing::info;
use windows::core::s;
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsPlatform {
    Win10_11,
    Server2022,
    Server2016_2019,
    Server2012,
    Win7_8,
}

static CURRENT_PLATFORM: OnceLock<WindowsPlatform> = OnceLock::new();

#[repr(C)]
#[allow(non_snake_case)]
struct RTL_OSVERSIONINFOEXW {
    dwOSVersionInfoSize: u32,
    dwMajorVersion: u32,
    dwMinorVersion: u32,
    dwBuildNumber: u32,
    dwPlatformId: u32,
    szCSDVersion: [u16; 128],
    wServicePackMajor: u16,
    wServicePackMinor: u16,
    wSuiteMask: u16,
    wProductType: u8,
    wReserved: u8,
}

type RtlGetVersionFn = unsafe extern "system" fn(*mut RTL_OSVERSIONINFOEXW) -> i32;

/// Detects the running Windows kernel version and platform edition
pub fn get_platform() -> WindowsPlatform {
    *CURRENT_PLATFORM.get_or_init(|| {
        unsafe {
            if let Ok(ntdll) = GetModuleHandleA(s!("ntdll.dll")) {
                if let Some(proc) = GetProcAddress(ntdll, s!("RtlGetVersion")) {
                    let rtl_get_version: RtlGetVersionFn = std::mem::transmute(proc);
                    let mut os_info: RTL_OSVERSIONINFOEXW = std::mem::zeroed();
                    os_info.dwOSVersionInfoSize = std::mem::size_of::<RTL_OSVERSIONINFOEXW>() as u32;

                    if rtl_get_version(&mut os_info) == 0 {
                        let major = os_info.dwMajorVersion;
                        let minor = os_info.dwMinorVersion;
                        let build = os_info.dwBuildNumber;
                        let is_server = os_info.wProductType != 1; // VER_NT_WORKSTATION = 1

                        info!(
                            "🔎 [OS DETECT] Windows Kernel {}.{} (Build {}) - Type: {}",
                            major, minor, build, if is_server { "Server" } else { "Workstation" }
                        );

                        if major >= 10 {
                            if is_server {
                                if major >= 10 && build >= 20348 {
                                    return WindowsPlatform::Server2022;
                                } else if major >= 10 {
                                    return WindowsPlatform::Server2016_2019;
                                } else if major == 6 && (minor == 2 || minor == 3) {
                                    return WindowsPlatform::Server2012;
                                } else {
                                    return WindowsPlatform::Server2022;
                                }
                            } else {
                                if major >= 10 {
                                    return WindowsPlatform::Win10_11;
                                } else {
                                    return WindowsPlatform::Win7_8;
                                }
                            }
                        } else if major == 6 {
                            if minor == 2 || minor == 3 {
                                return WindowsPlatform::Server2012;
                            }
                            return WindowsPlatform::Win7_8;
                        }
                    }
                }
            }
        }
        WindowsPlatform::Win10_11
    })
}

pub fn run() -> anyhow::Result<()> {
    let platform = get_platform();
    info!("🚀 Running Desky on Platform Engine: {:?}", platform);

    match platform {
        WindowsPlatform::Win10_11 => win10_11::init(),
        WindowsPlatform::Server2022 => server_2022::init(),
        WindowsPlatform::Server2016_2019 => server_2016_2019::init(),
        WindowsPlatform::Server2012 => server_2012::init(),
        WindowsPlatform::Win7_8 => win7_8::init(),
    }

    let ident = crate::identity::load_or_create_identity();
    tracing::info!(
        "Loaded Client Identity: Machine GUID = {}, Cached ID = {:?}",
        ident.machine_id, ident.desky_id
    );

    let vds_server = std::env::var("DESKY_VDS_SERVER")
        .unwrap_or_else(|_| "131.222.194.92:21116".into());
    tracing::info!("Connecting P2P Engine to VDS Matchmaker: {}", vds_server);

    let p2p = crate::p2p::P2PManager::new(ident.machine_id, ident.desky_id, vds_server);
    p2p.start_host_registration();

    tracing::info!("Spawning native Win32 window and starting message pump...");
    let app = ui::NativeWin32App::new()?;
    app.run_message_loop();
    Ok(())
}
