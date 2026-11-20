//! Windows Session 0 Service, UAC Input-Desktop Switching & SAS (Ctrl+Alt+Del)
//!
//! Enables Desky to run as a Windows System Service (`--service`):
//! - Registers with the Windows Service Control Manager (SCM)
//! - Switches capture/input threads to the active input desktop (`Winlogon` / UAC / `Default`)
//! - Dispatches Secure Attention Sequence (`Ctrl+Alt+Del`) via `sas.dll!SendSAS`

#![allow(unsafe_op_in_unsafe_fn, dead_code)]

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use tracing::info;
use windows::core::{s, w, PCWSTR};
use windows::Win32::Foundation::{FreeLibrary, GetLastError, HMODULE};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows::Win32::System::Services::{
    RegisterServiceCtrlHandlerExW, SetServiceStatus, StartServiceCtrlDispatcherW,
    SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP, SERVICE_CONTROL_SHUTDOWN,
    SERVICE_CONTROL_STOP, SERVICE_RUNNING, SERVICE_START_PENDING, SERVICE_STATUS,
    SERVICE_STATUS_HANDLE, SERVICE_STOPPED, SERVICE_STOP_PENDING, SERVICE_TABLE_ENTRYW,
    SERVICE_WIN32_OWN_PROCESS,
};
use windows::Win32::System::StationsAndDesktops::{
    CloseDesktop, OpenInputDesktop, SetThreadDesktop, DESKTOP_ACCESS_FLAGS, DESKTOP_CONTROL_FLAGS,
};
use windows::Win32::System::Threading::GetCurrentProcessId;

static SERVICE_RUNNING_FLAG: AtomicBool = AtomicBool::new(false);
static SERVICE_STATUS_HANDLE_VAL: AtomicIsize = AtomicIsize::new(0);

/// Returns true if the current process is running inside Windows Session 0 (Service context).
pub fn is_running_in_session_zero() -> bool {
    unsafe {
        let pid = GetCurrentProcessId();
        let mut session_id: u32 = 1;
        if let Ok(k32) = GetModuleHandleW(w!("kernel32.dll")) {
            if let Some(p) = GetProcAddress(k32, s!("ProcessIdToSessionId")) {
                let func: extern "system" fn(u32, *mut u32) -> i32 = std::mem::transmute(p);
                if func(pid, &mut session_id) != 0 {
                    return session_id == 0;
                }
            }
        }
    }
    false
}

/// Switches the calling thread to whichever desktop currently receives user input
/// (e.g., `Default`, `Winlogon` lock screen, or secure UAC `Consent` desktop).
pub fn switch_thread_to_input_desktop() -> bool {
    unsafe {
        // DESKTOP_ALL_ACCESS (0x000F_01FF)
        if let Ok(hdesk) = OpenInputDesktop(
            DESKTOP_CONTROL_FLAGS(0),
            false,
            DESKTOP_ACCESS_FLAGS(0x000F_01FF),
        ) {
            let ok = SetThreadDesktop(hdesk).is_ok();
            let _ = CloseDesktop(hdesk);
            return ok;
        }
    }
    false
}

/// Dispatches Secure Attention Sequence (`Ctrl+Alt+Del`) via `sas.dll!SendSAS(FALSE)`.
pub fn send_sas_ctrl_alt_del() -> bool {
    unsafe {
        let Ok(hmod): Result<HMODULE, _> = LoadLibraryW(w!("sas.dll")) else {
            return false;
        };
        let sent = if let Some(proc_addr) = GetProcAddress(hmod, s!("SendSAS")) {
            let send_sas: extern "system" fn(i32) = std::mem::transmute(proc_addr);
            send_sas(0); // FALSE = called from service
            true
        } else {
            false
        };
        let _ = FreeLibrary(hmod);
        sent
    }
}

/// Installs Desky as an automatic Windows System Service (`DeskyService`).
pub fn install_windows_service() -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    let bin_path = format!("\"{}\" --service", exe.to_string_lossy());
    let out = std::process::Command::new("sc.exe")
        .args([
            "create",
            "DeskyService",
            &format!("binPath= {}", bin_path),
            "start=",
            "auto",
            "DisplayName=",
            "Desky Remote Desktop Service",
        ])
        .output()?;
    if !out.status.success() {
        anyhow::bail!(
            "sc.exe create failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let _ = std::process::Command::new("sc.exe")
        .args(["start", "DeskyService"])
        .output();
    info!("Installed and started Windows Service 'DeskyService'");
    Ok(())
}

/// Stops and removes `DeskyService`.
pub fn uninstall_windows_service() -> anyhow::Result<()> {
    let _ = std::process::Command::new("sc.exe")
        .args(["stop", "DeskyService"])
        .output();
    let out = std::process::Command::new("sc.exe")
        .args(["delete", "DeskyService"])
        .output()?;
    if !out.status.success() {
        anyhow::bail!(
            "sc.exe delete failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    info!("Uninstalled Windows Service 'DeskyService'");
    Ok(())
}

unsafe extern "system" fn service_ctrl_handler(
    control: u32,
    _event_type: u32,
    _event_data: *mut std::ffi::c_void,
    _context: *mut std::ffi::c_void,
) -> u32 {
    let h_raw = SERVICE_STATUS_HANDLE_VAL.load(Ordering::Relaxed);
    let handle = SERVICE_STATUS_HANDLE(h_raw as *mut _);
    match control {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
            SERVICE_RUNNING_FLAG.store(false, Ordering::SeqCst);
            let mut status = SERVICE_STATUS {
                dwServiceType: SERVICE_WIN32_OWN_PROCESS,
                dwCurrentState: SERVICE_STOP_PENDING,
                ..Default::default()
            };
            let _ = SetServiceStatus(handle, &mut status);
            0
        }
        _ => 0,
    }
}

unsafe extern "system" fn service_main(_argc: u32, _argv: *mut windows::core::PWSTR) {
    let svc_name = w!("DeskyService");
    let Ok(handle) = RegisterServiceCtrlHandlerExW(svc_name, Some(service_ctrl_handler), None) else {
        return;
    };
    SERVICE_STATUS_HANDLE_VAL.store(handle.0 as isize, Ordering::SeqCst);

    let mut status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: SERVICE_START_PENDING,
        ..Default::default()
    };
    let _ = SetServiceStatus(handle, &mut status);

    SERVICE_RUNNING_FLAG.store(true, Ordering::SeqCst);
    status.dwCurrentState = SERVICE_RUNNING;
    status.dwControlsAccepted = SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN;
    let _ = SetServiceStatus(handle, &mut status);

    // Start P2P host engine in Session 0 service context
    let ident = crate::identity::load_or_create_identity();
    let vds_server = std::env::var("DESKY_VDS_SERVER")
        .unwrap_or_else(|_| crate::config::DEFAULT_VDS_SERVER.into());
    let p2p = crate::p2p::P2PManager::new(ident.machine_id, ident.desky_id, vds_server);
    p2p.start_host_registration();

    while SERVICE_RUNNING_FLAG.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(250));
    }

    status.dwCurrentState = SERVICE_STOPPED;
    status.dwControlsAccepted = 0;
    let _ = SetServiceStatus(handle, &mut status);
}

pub fn run_windows_service() -> anyhow::Result<()> {
    let mut name: Vec<u16> = "DeskyService\0".encode_utf16().collect();
    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: windows::core::PWSTR(name.as_mut_ptr()),
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW {
            lpServiceName: windows::core::PWSTR::null(),
            lpServiceProc: None,
        },
    ];
    unsafe {
        if StartServiceCtrlDispatcherW(table.as_ptr()).is_err() {
            let err = GetLastError();
            anyhow::bail!("StartServiceCtrlDispatcherW failed: {:?}", err);
        }
    }
    let _ = PCWSTR::null();
    Ok(())
}
