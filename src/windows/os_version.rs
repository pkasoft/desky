//! Windows Operating System Version Detection Engine

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
