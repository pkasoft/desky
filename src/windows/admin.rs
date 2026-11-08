//! Windows Silent Admin & Background Management Implementation
//! Zero-screen-takeover process inspection, termination, system telemetry and shell execution.

use crate::protocol::{AdminReq, AdminResp, ProcessItem, SystemStats};
use std::os::windows::process::CommandExt;
use std::process::Command;
use tracing::{info, warn};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::SystemInformation::{
    GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows::Win32::System::Threading::{
    OpenProcess, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};

pub fn handle_admin_request(req: AdminReq) -> AdminResp {
    match req {
        AdminReq::GetProcesses => AdminResp::Processes(get_process_list()),
        AdminReq::KillProcess { pid } => {
            let (success, msg) = kill_process(pid);
            AdminResp::KillResult { pid, success, msg }
        }
        AdminReq::ExecuteCommand { command } => {
            let (output, exit_code) = execute_shell_command(&command);
            AdminResp::CommandOutput { output, exit_code }
        }
        AdminReq::GetSystemInfo => AdminResp::SystemInfo(get_system_info()),
    }
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

    SystemStats {
        os_name: "Windows".into(),
        hostname,
        uptime_secs,
        cpu_count,
        total_mem_mb,
        free_mem_mb,
    }
}
