//! Windows Silent Admin & Background Management Implementation
//! Zero-screen-takeover process inspection, termination, system telemetry and shell execution.

#![allow(unsafe_op_in_unsafe_fn)]

use crate::protocol::{AdminReq, AdminResp, ProcessItem, ServiceItem, SystemAction, SystemStats};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::atomic::{AtomicIsize, Ordering};
use tracing::{info, warn};
use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, SelectObject,
    SetBkMode, SetTextColor, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL, HGDIOBJ,
    PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::SystemInformation::{
    GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows::Win32::System::Threading::{
    OpenProcess, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, GetSystemMetrics,
    LoadCursorW, PostMessageW, PostQuitMessage, RegisterClassExW, SetWindowDisplayAffinity,
    TranslateMessage, IDC_ARROW, MSG, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, WINDOW_DISPLAY_AFFINITY, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_NCHITTEST,
    WM_PAINT, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    WS_POPUP, WS_VISIBLE,
};

static PRIVACY_CURTAIN_HWND: AtomicIsize = AtomicIsize::new(0);

pub fn handle_admin_request(req: AdminReq) -> AdminResp {
    match req {
        AdminReq::GetProcesses => AdminResp::Processes(get_process_list()),
        AdminReq::KillProcess { pid } => {
            let (success, msg) = kill_process(pid);
            AdminResp::KillResult { pid, success, msg }
        }
        AdminReq::GetServices => AdminResp::Services(get_service_list()),
        AdminReq::ControlService { name, action } => {
            let (success, msg) = control_service(&name, &action);
            AdminResp::ServiceResult { name, success, msg }
        }
        AdminReq::ExecuteCommand { command } => {
            let (output, exit_code) = execute_shell_command(&command);
            AdminResp::CommandOutput { output, exit_code }
        }
        AdminReq::GetSystemInfo => AdminResp::SystemInfo(get_system_info()),
        AdminReq::SwitchMonitor { monitor_index } => {
            super::dxgi_capture::set_target_monitor(monitor_index);
            let count = super::dxgi_capture::count_available_monitors();
            info!("Host switched active monitor to #{} of {}", monitor_index, count);
            AdminResp::MonitorSwitched { monitor_index, count }
        }
        AdminReq::RebootHost { safe_mode } => {
            schedule_remote_reboot(safe_mode);
            AdminResp::RebootScheduled { safe_mode }
        }
        AdminReq::SetPrivacyMode { enabled } => {
            set_privacy_curtain(enabled);
            AdminResp::PrivacyModeChanged { enabled }
        }
        AdminReq::WakeOnLan { mac } => {
            let success = if let Some(bytes) = crate::p2p::parse_mac_address(&mac) {
                crate::p2p::send_wake_on_lan(bytes).is_ok()
            } else {
                false
            };
            AdminResp::WakeOnLanSent { mac, success }
        }
        AdminReq::TriggerSystemAction { action } => {
            let (success, msg) = trigger_system_action(action);
            AdminResp::SystemActionResult { action, success, msg }
        }
    }
}

pub fn trigger_system_action(action: SystemAction) -> (bool, String) {
    match action {
        SystemAction::LockWorkstation => {
            let res = unsafe {
                use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
                use windows::core::s;
                if let Ok(h) = LoadLibraryA(s!("user32.dll")) {
                    if let Some(proc) = GetProcAddress(h, s!("LockWorkStation")) {
                        let lock_fn: extern "system" fn() -> i32 = std::mem::transmute(proc);
                        lock_fn() != 0
                    } else { false }
                } else { false }
            };
            (res, if res { "Workstation locked".into() } else { "Failed to lock workstation".into() })
        }
        SystemAction::TaskManager => {
            let res = Command::new("taskmgr.exe")
                .creation_flags(0x08000000)
                .spawn()
                .is_ok();
            (res, if res { "Task Manager launched".into() } else { "Failed to launch Task Manager".into() })
        }
        SystemAction::ShowDesktop => {
            let res = Command::new("powershell")
                .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", "(New-Object -ComObject Shell.Application).ToggleDesktop()"])
                .creation_flags(0x08000000)
                .spawn()
                .is_ok();
            (res, "Show Desktop toggled".into())
        }
        SystemAction::OpenRun => {
            let res = Command::new("powershell")
                .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", "(New-Object -ComObject Shell.Application).FileRun()"])
                .creation_flags(0x08000000)
                .spawn()
                .is_ok();
            (res, "Run Dialog opened".into())
        }
        SystemAction::CtrlAltDel => {
            let success = unsafe {
                use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
                use windows::core::s;
                if let Ok(h) = LoadLibraryA(s!("sas.dll")) {
                    if let Some(proc) = GetProcAddress(h, s!("SendSAS")) {
                        let sendsas: extern "system" fn(i32) = std::mem::transmute(proc);
                        sendsas(0);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            };
            if success {
                (true, "SAS Ctrl+Alt+Del triggered".into())
            } else {
                let _ = Command::new("taskmgr.exe").creation_flags(0x08000000).spawn();
                (true, "Security/Task Manager triggered (SAS requires sas.dll policy)".into())
            }
        }
        SystemAction::RebootNormal => {
            schedule_remote_reboot(false);
            (true, "Host reboot scheduled".into())
        }
        SystemAction::RebootSafe => {
            schedule_remote_reboot(true);
            (true, "Host Safe Mode reboot scheduled".into())
        }
    }
}

pub fn schedule_remote_reboot(safe_mode: bool) {
    std::thread::spawn(move || {
        info!("Host Remote Reboot triggered (Safe Mode: {}). Scheduling auto-reconnect...", safe_mode);

        if let Ok(exe_path) = std::env::current_exe() {
            let cmd = format!(
                "reg add HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce /v DeskyAutoReconnect /t REG_SZ /d \"\\\"{}\\\"\" /f",
                exe_path.display()
            );
            let _ = Command::new("cmd")
                .args(["/C", &cmd])
                .creation_flags(0x08000000)
                .output();
        }

        if safe_mode {
            let _ = Command::new("cmd")
                .args(["/C", "bcdedit /set {current} safeboot network"])
                .creation_flags(0x08000000)
                .output();
        }

        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = Command::new("shutdown")
            .args(["/r", "/t", "3", "/f", "/c", "Desky Remote Restart"])
            .creation_flags(0x08000000)
            .output();
    });
}

pub fn get_process_list() -> Vec<ProcessItem> {
    let mut list = Vec::new();
    unsafe {
        let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(e) => {
                warn!("Failed to create toolhelp process snapshot: {:?}", e);
                return list;
            }
        };

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let pid = entry.th32ProcessID;
                if pid > 0 {
                    let null_idx = entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    let name = String::from_utf16_lossy(&entry.szExeFile[..null_idx]);

                    let mut mem_kb = 0u64;
                    if let Ok(h_proc) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                        let mut pmc = PROCESS_MEMORY_COUNTERS::default();
                        pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
                        if GetProcessMemoryInfo(
                            h_proc,
                            &mut pmc,
                            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
                        )
                        .is_ok()
                        {
                            mem_kb = (pmc.WorkingSetSize / 1024) as u64;
                        }
                        let _ = CloseHandle(h_proc);
                    }

                    list.push(ProcessItem { pid, name, mem_kb });
                }

                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }

    list.sort_by(|a, b| b.mem_kb.cmp(&a.mem_kb));
    list
}

pub fn kill_process(pid: u32) -> (bool, String) {
    if pid <= 4 {
        return (false, "Cannot terminate system critical process".into());
    }
    unsafe {
        match OpenProcess(PROCESS_TERMINATE, false, pid) {
            Ok(h_proc) => {
                let res = TerminateProcess(h_proc, 1);
                let _ = CloseHandle(h_proc);
                if res.is_ok() {
                    info!("Successfully terminated remote process PID {}", pid);
                    (true, format!("Process PID {} terminated successfully", pid))
                } else {
                    warn!("Failed to terminate process PID {}", pid);
                    (
                        false,
                        format!("Failed to terminate PID {}: Access Denied", pid),
                    )
                }
            }
            Err(e) => (
                false,
                format!("Failed to open process PID {}: {:?}", pid, e),
            ),
        }
    }
}

pub fn get_service_list() -> Vec<ServiceItem> {
    let mut list = Vec::new();
    let mut cmd = Command::new("sc.exe");
    cmd.args(["query", "type=", "service", "state=", "all"]);
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    if let Ok(out) = cmd.output() {
        let text = String::from_utf8_lossy(&out.stdout);
        let mut cur_name = String::new();
        let mut cur_display = String::new();
        let mut cur_status = String::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("SERVICE_NAME:") {
                if !cur_name.is_empty() {
                    list.push(ServiceItem {
                        name: cur_name,
                        display_name: if cur_display.is_empty() { cur_display } else { cur_display },
                        status: if cur_status.is_empty() { "STOPPED".into() } else { cur_status },
                    });
                    cur_display = String::new();
                    cur_status = String::new();
                }
                cur_name = trimmed.trim_start_matches("SERVICE_NAME:").trim().to_string();
            } else if trimmed.starts_with("DISPLAY_NAME:") {
                cur_display = trimmed.trim_start_matches("DISPLAY_NAME:").trim().to_string();
            } else if trimmed.starts_with("STATE") && trimmed.contains(':') {
                if let Some(pos) = trimmed.find(':') {
                    let st = trimmed[pos + 1..].trim();
                    let parts: Vec<&str> = st.split_whitespace().collect();
                    if parts.len() >= 2 {
                        cur_status = parts[1].to_string();
                    } else if !parts.is_empty() {
                        cur_status = parts[0].to_string();
                    }
                }
            }
        }
        if !cur_name.is_empty() {
            list.push(ServiceItem {
                name: cur_name.clone(),
                display_name: if cur_display.is_empty() { cur_name } else { cur_display },
                status: if cur_status.is_empty() { "STOPPED".into() } else { cur_status },
            });
        }
    }
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list
}

pub fn control_service(name: &str, action: &str) -> (bool, String) {
    let action_lower = action.to_lowercase();
    match action_lower.as_str() {
        "start" => {
            let mut cmd = Command::new("sc.exe");
            cmd.args(["start", name]);
            cmd.creation_flags(0x08000000);
            match cmd.output() {
                Ok(o) => (o.status.success(), format!("Started service '{}'", name)),
                Err(e) => (false, format!("Failed to start service '{}': {}", name, e)),
            }
        }
        "stop" => {
            let mut cmd = Command::new("sc.exe");
            cmd.args(["stop", name]);
            cmd.creation_flags(0x08000000);
            match cmd.output() {
                Ok(o) => (o.status.success(), format!("Stopped service '{}'", name)),
                Err(e) => (false, format!("Failed to stop service '{}': {}", name, e)),
            }
        }
        "restart" => {
            let mut cmd_stop = Command::new("sc.exe");
            cmd_stop.args(["stop", name]);
            cmd_stop.creation_flags(0x08000000);
            let _ = cmd_stop.output();
            std::thread::sleep(std::time::Duration::from_millis(600));
            let mut cmd_start = Command::new("sc.exe");
            cmd_start.args(["start", name]);
            cmd_start.creation_flags(0x08000000);
            match cmd_start.output() {
                Ok(o) => (o.status.success(), format!("Restarted service '{}'", name)),
                Err(e) => (false, format!("Failed to restart service '{}': {}", name, e)),
            }
        }
        _ => (false, format!("Unknown service action '{}'", action)),
    }
}

pub fn execute_shell_command(cmd: &str) -> (String, i32) {
    info!("⚡ [SILENT ADMIN] Executing remote shell command: {}", cmd);
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command", cmd]);
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW

    match command.output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let combined = if stderr.is_empty() {
                stdout
            } else if stdout.is_empty() {
                stderr
            } else {
                format!("{}\n{}", stdout, stderr)
            };
            let code = out.status.code().unwrap_or(0);
            (combined, code)
        }
        Err(e) => (format!("Execution failed: {}", e), -1),
    }
}

pub fn get_system_info() -> SystemStats {
    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "Desky Host".into());
    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    let (total_mem_mb, free_mem_mb) = unsafe {
        let mut mem_stat = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        if GlobalMemoryStatusEx(&mut mem_stat).is_ok() {
            (
                mem_stat.ullTotalPhys / (1024 * 1024),
                mem_stat.ullAvailPhys / (1024 * 1024),
            )
        } else {
            (0, 0)
        }
    };

    let uptime_secs = unsafe { GetTickCount64() / 1000 };

    let mac_output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", "(Get-NetAdapter | Where-Object Status -eq 'Up').MacAddress | Select-Object -First 1"])
        .creation_flags(0x08000000)
        .output();
    let mac_addr = match mac_output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Err(_) => String::new(),
    };

    SystemStats {
        os_name: "Windows".into(),
        hostname,
        uptime_secs,
        cpu_count,
        total_mem_mb,
        free_mem_mb,
        mac_addr,
    }
}

pub fn set_privacy_curtain(enabled: bool) {
    if enabled {
        let existing = PRIVACY_CURTAIN_HWND.load(Ordering::SeqCst);
        if existing != 0 {
            return;
        }

        // Dedicated thread running a message pump to keep the curtain responsive and prevent desktop ghosting
        std::thread::Builder::new()
            .name("desky_privacy_curtain".to_string())
            .spawn(move || unsafe {
                let h_module = GetModuleHandleW(None).unwrap_or_default();
                let h_instance: HINSTANCE = h_module.into();
                let class_name = w!("DeskyPrivacyCurtain");

                let mut wc = WNDCLASSEXW::default();
                wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
                wc.lpfnWndProc = Some(privacy_curtain_wndproc);
                wc.hInstance = h_instance;
                wc.lpszClassName = class_name;
                wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
                let _ = RegisterClassExW(&wc);

                let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
                let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
                let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
                let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN);

                if let Ok(hwnd) = CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                    class_name,
                    w!("Desky Privacy Curtain"),
                    WS_POPUP | WS_VISIBLE,
                    vx,
                    vy,
                    vw,
                    vh,
                    None,
                    None,
                    Some(h_instance),
                    None,
                ) {
                    // WDA_EXCLUDEFROMCAPTURE (0x00000011): visible on physical screen, hidden from DXGI capture
                    let _ = SetWindowDisplayAffinity(hwnd, WINDOW_DISPLAY_AFFINITY(0x00000011));
                    PRIVACY_CURTAIN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
                    info!("🔒 Host Privacy Curtain window activated ({}x{} at {},{})", vw, vh, vx, vy);

                    // Win32 message loop to prevent Windows DWM from marking the window or desktop as unresponsive
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    PRIVACY_CURTAIN_HWND.store(0, Ordering::SeqCst);
                    info!("🔓 Host Privacy Curtain thread exited cleanly");
                }
            })
            .ok();
    } else {
        let old = PRIVACY_CURTAIN_HWND.swap(0, Ordering::SeqCst);
        if old != 0 {
            unsafe {
                let hwnd = HWND(old as *mut _);
                // Cross-thread safe dismissal via WM_CLOSE message to the owning thread
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            info!("🔓 Host Privacy Curtain dismissal posted");
        }
    }
}

unsafe extern "system" fn privacy_curtain_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_NCHITTEST => LRESULT(-1), // HTTRANSPARENT: clicks pass directly through to apps underneath
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rect = RECT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);
            let bg_brush = CreateSolidBrush(COLORREF(0x000F0A05));
            FillRect(hdc, &rect, bg_brush);
            let _ = DeleteObject(HGDIOBJ(bg_brush.0));

            SetBkMode(hdc, TRANSPARENT);

            let font_title = super::ui::theme::create_segoe_font(28, FW_BOLD);
            let old_f = SelectObject(hdc, HGDIOBJ(font_title.0));
            SetTextColor(hdc, super::ui::theme::PK_GREEN);
            let mut t_title: Vec<u16> = "DESKY PRIVACY CURTAIN ACTIVE".encode_utf16().collect();
            let mut rc_title = RECT {
                left: 0,
                top: (rect.bottom / 2) - 60,
                right: rect.right,
                bottom: (rect.bottom / 2) - 10,
            };
            let _ = DrawTextW(hdc, &mut t_title, &mut rc_title, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

            let font_sub = super::ui::theme::create_segoe_font(16, FW_NORMAL);
            let _ = SelectObject(hdc, HGDIOBJ(font_sub.0));
            SetTextColor(hdc, COLORREF(0x0094A3B8));
            let mut t_sub: Vec<u16> = "This computer is being remotely serviced. Physical display is masked for privacy."
                .encode_utf16()
                .collect();
            let mut rc_sub = RECT {
                left: 0,
                top: (rect.bottom / 2),
                right: rect.right,
                bottom: (rect.bottom / 2) + 40,
            };
            let _ = DrawTextW(hdc, &mut t_sub, &mut rc_sub, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

            let _ = SelectObject(hdc, old_f);
            let _ = DeleteObject(HGDIOBJ(font_title.0));
            let _ = DeleteObject(HGDIOBJ(font_sub.0));

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

