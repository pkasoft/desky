//! Linux Silent Admin & Background Management Implementation

use crate::protocol::{AdminReq, AdminResp, ProcessItem, SystemStats};
use std::process::Command;

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
        AdminReq::GetServices => AdminResp::Services(Vec::new()),
        AdminReq::ControlService { name, .. } => AdminResp::ServiceResult {
            name,
            success: false,
            msg: "Service control unsupported on Linux".into(),
        },
        AdminReq::SwitchMonitor { monitor_index } => AdminResp::MonitorSwitched {
            monitor_index,
            count: 1,
        },
        AdminReq::RebootHost { safe_mode } => AdminResp::RebootScheduled { safe_mode },
        AdminReq::SetPrivacyMode { enabled } => AdminResp::PrivacyModeChanged { enabled },
        AdminReq::WakeOnLan { mac } => AdminResp::WakeOnLanSent { mac, success: false },
        AdminReq::TriggerSystemAction { action } => AdminResp::SystemActionResult {
            action,
            success: false,
            msg: "System action not supported on Linux".into(),
        },
    }
}

pub fn get_process_list() -> Vec<ProcessItem> {
    let mut list = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            if let Ok(file_name) = entry.file_name().into_string() {
                if let Ok(pid) = file_name.parse::<u32>() {
                    let comm_path = format!("/proc/{}/comm", pid);
                    let name = std::fs::read_to_string(comm_path)
                        .map(|s| s.trim().to_string())
                        .unwrap_or_else(|_| "unknown".into());
                    list.push(ProcessItem {
                        pid,
                        name,
                        mem_kb: 0,
                    });
                }
            }
        }
    }
    list
}

pub fn kill_process(pid: u32) -> (bool, String) {
    let status = Command::new("kill").arg("-9").arg(pid.to_string()).status();
    match status {
        Ok(s) if s.success() => (true, format!("Terminated PID {}", pid)),
        Ok(_) => (false, format!("Failed to terminate PID {}", pid)),
        Err(e) => (false, format!("Error terminating PID {}: {}", pid, e)),
    }
}

pub fn execute_shell_command(cmd: &str) -> (String, i32) {
    match Command::new("sh").arg("-c").arg(cmd).output() {
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
        Err(e) => (format!("Failed to execute: {}", e), -1),
    }
}

pub fn get_system_info() -> SystemStats {
    let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "Linux Host".into());
    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    SystemStats {
        os_name: "Linux".into(),
        hostname,
        uptime_secs: 0,
        cpu_count,
        total_mem_mb: 0,
        free_mem_mb: 0,
        mac_addr: String::new(),
    }
}
