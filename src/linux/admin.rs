//! Linux Silent Admin & Background Management Implementation

use crate::protocol::{AdminReq, AdminResp, ProcessItem, SystemAction, SystemStats};
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
        AdminReq::GetServices => AdminResp::Services(get_services_list()),
        AdminReq::ControlService { name, action } => {
            let (success, msg) = control_service(&name, &action);
            AdminResp::ServiceResult { name, success, msg }
        }
        AdminReq::SwitchMonitor { monitor_index } => AdminResp::MonitorSwitched {
            monitor_index,
            count: 1,
        },
        AdminReq::RebootHost { safe_mode } => {
            let (success, msg) = trigger_reboot(safe_mode);
            if success {
                AdminResp::RebootScheduled { safe_mode }
            } else {
                AdminResp::CommandOutput {
                    output: msg,
                    exit_code: 1,
                }
            }
        }
        AdminReq::SetPrivacyMode { enabled } => AdminResp::PrivacyModeChanged { enabled },
        AdminReq::WakeOnLan { mac } => {
            let success = if let Some(mac_bytes) = crate::p2p::radar::parse_mac_address(&mac) {
                crate::p2p::radar::send_wake_on_lan(mac_bytes).is_ok()
            } else {
                false
            };
            AdminResp::WakeOnLanSent { mac, success }
        }
        AdminReq::TriggerSystemAction { action } => {
            let (success, msg) = trigger_system_action(action);
            AdminResp::SystemActionResult {
                action,
                success,
                msg,
            }
        }
    }
}

pub fn trigger_reboot(_safe_mode: bool) -> (bool, String) {
    let attempts = [
        ("systemctl", vec!["reboot"]),
        ("shutdown", vec!["-r", "now"]),
        ("reboot", vec![]),
    ];

    for (cmd, args) in attempts {
        if let Ok(status) = Command::new(cmd).args(&args).status() {
            if status.success() {
                return (true, format!("Reboot triggered via {}", cmd));
            }
        }
    }
    (false, "Failed to invoke reboot command (systemctl/shutdown)".into())
}

pub fn trigger_system_action(action: SystemAction) -> (bool, String) {
    match action {
        SystemAction::LockWorkstation => {
            let lock_cmds = [
                ("loginctl", vec!["lock-session"]),
                ("xdg-screensaver", vec!["lock"]),
                ("gnome-screensaver-command", vec!["-l"]),
                ("xflock4", vec![]),
                ("swaylock", vec![]),
            ];
            for (cmd, args) in lock_cmds {
                if let Ok(status) = Command::new(cmd).args(&args).status() {
                    if status.success() {
                        return (true, format!("Session locked via {}", cmd));
                    }
                }
            }
            (false, "No screen locker utility found (loginctl/xdg-screensaver)".into())
        }
        SystemAction::TaskManager => {
            let tm_cmds = [
                ("gnome-system-monitor", vec![]),
                ("ksysguard", vec![]),
                ("plasma-systemmonitor", vec![]),
                ("mate-system-monitor", vec![]),
                ("xfce4-taskmanager", vec![]),
                ("xterm", vec!["-e", "htop"]),
            ];
            for (cmd, args) in tm_cmds {
                if Command::new(cmd).args(&args).spawn().is_ok() {
                    return (true, format!("Task manager launched via {}", cmd));
                }
            }
            (false, "No task manager found (gnome-system-monitor, htop)".into())
        }
        SystemAction::ShowDesktop => {
            let desk_cmds = [
                ("xdotool", vec!["key", "super+d"]),
                ("wmctrl", vec!["-k", "on"]),
            ];
            for (cmd, args) in desk_cmds {
                if let Ok(status) = Command::new(cmd).args(&args).status() {
                    if status.success() {
                        return (true, format!("Desktop toggled via {}", cmd));
                    }
                }
            }
            (false, "Show desktop requires xdotool or wmctrl".into())
        }
        SystemAction::OpenRun => {
            let run_cmds = [
                ("xdotool", vec!["key", "alt+F2"]),
                ("krunner", vec![]),
                ("rofi", vec!["-show", "run"]),
                ("dmenu_run", vec![]),
            ];
            for (cmd, args) in run_cmds {
                if Command::new(cmd).args(&args).spawn().is_ok() {
                    return (true, format!("Run dialog launched via {}", cmd));
                }
            }
            (false, "Run dialog launcher not found (xdotool/krunner/rofi)".into())
        }
        SystemAction::CtrlAltDel => {
            let cad_cmds = [
                ("xdotool", vec!["key", "ctrl+alt+Delete"]),
            ];
            for (cmd, args) in cad_cmds {
                if let Ok(status) = Command::new(cmd).args(&args).status() {
                    if status.success() {
                        return (true, "Ctrl+Alt+Del triggered via xdotool".into());
                    }
                }
            }
            (false, "Ctrl+Alt+Del injection requires xdotool".into())
        }
        SystemAction::RebootNormal => trigger_reboot(false),
        SystemAction::RebootSafe => trigger_reboot(true),
        SystemAction::ElevatePrivileges => (false, "Elevation is only supported on Windows hosts".into()),
    }
}

pub fn get_services_list() -> Vec<crate::protocol::ServiceItem> {
    let mut services = Vec::new();
    if let Ok(output) = Command::new("systemctl")
        .args(["list-units", "--type=service", "--no-pager", "--no-legend"])
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let name = parts[0].trim_end_matches(".service").to_string();
                let active = parts[2] == "active";
                let status = parts[3].to_string();
                services.push(crate::protocol::ServiceItem {
                    name: name.clone(),
                    display_name: name,
                    status: if active { "Running".into() } else { status },
                });
            }
        }
    }
    services
}

pub fn control_service(name: &str, action: &str) -> (bool, String) {
    let subcmd = match action.to_lowercase().as_str() {
        "start" => "start",
        "stop" => "stop",
        "restart" => "restart",
        _ => return (false, format!("Invalid service action: {}", action)),
    };

    match Command::new("systemctl").arg(subcmd).arg(name).status() {
        Ok(s) if s.success() => (true, format!("Service {} {}ed successfully", name, subcmd)),
        Ok(s) => (false, format!("systemctl returned code {}", s.code().unwrap_or(-1))),
        Err(e) => (false, format!("Failed to control service: {}", e)),
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
