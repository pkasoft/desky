//! Silent Admin & Background Management UI View (Linux X11)
//!
//! Mirrors `src/windows/ui/admin.rs` by rendering background process manager,
//! remote services, interactive shell console, and system telemetry without
//! interrupting the remote user's screen or mouse.

#![allow(dead_code)]

use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use x11rb::protocol::xproto::KeyButMask;

use super::remote_desk_edit::x11_keycode_to_ascii;
use super::theme::{
    PixelCanvas, PK_BG, PK_BTN_HOVER, PK_CARD, PK_CARD_BORDER, PK_GREEN, PK_GREEN_DARK,
    PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_RED, PK_RED_TINT, PK_TEXT,
};
use crate::protocol::{AdminReq, AdminResp, ProcessItem, ServiceItem, SystemStats};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminSubTab {
    Processes,
    Services,
    Terminal,
    SystemInfo,
}

pub struct AdminState {
    pub sub_tab: AdminSubTab,
    pub processes: Vec<ProcessItem>,
    pub process_filter: String,
    pub scroll_y: i32,
    pub services: Vec<ServiceItem>,
    pub terminal_lines: Vec<String>,
    pub terminal_input: String,
    pub system_stats: Option<SystemStats>,
    pub status_msg: Option<(String, Instant)>,
}

impl Default for AdminState {
    fn default() -> Self {
        Self {
            sub_tab: AdminSubTab::Processes,
            processes: Vec::new(),
            process_filter: String::new(),
            scroll_y: 0,
            services: Vec::new(),
            terminal_lines: vec![
                "[Admin] Desky Silent Remote Management Console".into(),
                "Commands execute on the remote host with zero screen or mouse interruption.".into(),
                "Type any command below and press Enter (e.g. dir, ipconfig, ps aux):".into(),
            ],
            terminal_input: String::new(),
            system_stats: None,
            status_msg: None,
        }
    }
}

static ADMIN_STATE: OnceLock<Mutex<AdminState>> = OnceLock::new();

pub fn get_admin_state() -> &'static Mutex<AdminState> {
    ADMIN_STATE.get_or_init(|| Mutex::new(AdminState::default()))
}

pub fn poll_admin_responses() {
    while let Some(resp) = crate::p2p::take_admin_response() {
        if let Ok(mut state) = get_admin_state().lock() {
            match resp {
                AdminResp::Processes(list) => {
                    let count = list.len();
                    state.processes = list;
                    state.status_msg = Some((format!("Loaded {} processes", count), Instant::now()));
                }
                AdminResp::KillResult { pid, success, msg } => {
                    state.status_msg = Some((msg, Instant::now()));
                    if success {
                        state.processes.retain(|p| p.pid != pid);
                        crate::p2p::send_admin_request(AdminReq::GetProcesses);
                    }
                }
                AdminResp::Services(list) => {
                    let count = list.len();
                    state.services = list;
                    state.status_msg = Some((format!("Loaded {} services", count), Instant::now()));
                }
                AdminResp::ServiceResult { name: _, success: _, msg } => {
                    state.status_msg = Some((msg, Instant::now()));
                    crate::p2p::send_admin_request(AdminReq::GetServices);
                }
                AdminResp::CommandOutput { output, exit_code } => {
                    for line in output.lines() {
                        state.terminal_lines.push(line.to_string());
                    }
                    if exit_code != 0 {
                        state
                            .terminal_lines
                            .push(format!("[Process returned exit code {}]", exit_code));
                    }
                    state.terminal_lines.push(String::new());
                }
                AdminResp::SystemInfo(stats) => {
                    state.system_stats = Some(stats);
                    state.status_msg = Some(("System telemetry updated".into(), Instant::now()));
                }
                AdminResp::MonitorSwitched { monitor_index, count } => {
                    state.status_msg = Some((
                        format!("Switched to Monitor #{} (Total: {})", monitor_index + 1, count),
                        Instant::now(),
                    ));
                }
                AdminResp::RebootScheduled { safe_mode } => {
                    state.status_msg = Some((
                        format!("Host reboot scheduled (Safe Mode: {})", safe_mode),
                        Instant::now(),
                    ));
                }
                AdminResp::PrivacyModeChanged { enabled } => {
                    state.status_msg = Some((
                        if enabled {
                            "Host Privacy Screen ENABLED".into()
                        } else {
                            "Host Privacy Screen DISABLED".into()
                        },
                        Instant::now(),
                    ));
                }
                AdminResp::WakeOnLanSent { mac, success } => {
                    state.status_msg = Some((
                        if success {
                            format!("Magic packet sent to {}", mac)
                        } else {
                            format!("Failed to send WoL to {}", mac)
                        },
                        Instant::now(),
                    ));
                }
                AdminResp::SystemActionResult { action: _, success: _, msg } => {
                    state.status_msg = Some((msg, Instant::now()));
                }
            }
        }
    }
}

pub fn draw_admin_view(canvas: &mut PixelCanvas, win_w: u16, win_h: u16, hover_state: i32) {
    poll_admin_responses();

    let width = win_w as i32;
    let height = win_h as i32;
    canvas.fill_rect(0, 40, width, height - 40, PK_BG);

    let (sub_tab, processes, services, term_lines, term_in, stats, status_msg) = {
        let s = get_admin_state().lock().unwrap_or_else(|e| e.into_inner());
        (
            s.sub_tab,
            s.processes.clone(),
            s.services.clone(),
            s.terminal_lines.clone(),
            s.terminal_input.clone(),
            s.system_stats.clone(),
            s.status_msg.clone(),
        )
    };

    // Sub-tab bar
    let bar_y = 48;
    let bar_h = 36;
    canvas.draw_tb_button_box(24, bar_y, width - 48, bar_h, PK_CARD, PK_CARD_BORDER, 1);

    let tabs = [
        (AdminSubTab::Processes, "Processes"),
        (AdminSubTab::Services, "Services"),
        (AdminSubTab::Terminal, "Terminal"),
        (AdminSubTab::SystemInfo, "Telemetry"),
    ];

    let mut cur_x = 32;
    for (i, (tab_kind, label)) in tabs.iter().enumerate() {
        let is_active = *tab_kind == sub_tab;
        let is_hover = hover_state == (2000 + i as i32);
        let tab_w = 118;
        let (bg, bd, fg) = if is_active {
            (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
        } else if is_hover {
            (PK_BTN_HOVER, PK_CARD_BORDER, PK_TEXT)
        } else {
            (PK_CARD, PK_CARD_BORDER, PK_MUTED)
        };
        canvas.draw_rounded_box(cur_x, bar_y + 4, tab_w, 28, bg, bd, 4);
        canvas.draw_text_centered(cur_x, bar_y + 4, tab_w, 28, label, 13.0, is_active, fg);
        cur_x += tab_w + 8;
    }

    // Refresh button on far right
    let ref_w = 92;
    let ref_x = width - 24 - ref_w - 6;
    canvas.draw_rounded_box(ref_x, bar_y + 4, ref_w, 28, PK_GREEN_TINT, PK_GREEN, 4);
    canvas.draw_text_centered(ref_x, bar_y + 4, ref_w, 28, "Refresh", 12.5, true, PK_GREEN_DARK);

    let body_y = 94;
    let body_h = (height - body_y - 20).max(80);
    canvas.draw_rounded_box(24, body_y, width - 48, body_h, PK_CARD, PK_CARD_BORDER, 8);

    match sub_tab {
        AdminSubTab::Processes => {
            canvas.fill_rect(25, body_y + 1, width - 50, 30, PK_CARD_BORDER);
            canvas.draw_text_vcenter(36, body_y + 1, 90, 30, "PID", 12.5, true, PK_TEXT);
            canvas.draw_text_vcenter(136, body_y + 1, 240, 30, "Process Name", 12.5, true, PK_TEXT);
            canvas.draw_text_vcenter(
                width - 240,
                body_y + 1,
                110,
                30,
                "Memory",
                12.5,
                true,
                PK_TEXT,
            );
            canvas.draw_text_vcenter(width - 120, body_y + 1, 90, 30, "Action", 12.5, true, PK_TEXT);

            let row_h = 28;
            let max_rows = ((body_h - 36) / row_h).max(1) as usize;
            for (idx, p) in processes.iter().take(max_rows).enumerate() {
                let ry = body_y + 32 + (idx as i32) * row_h;
                if idx % 2 == 1 {
                    canvas.fill_rect(25, ry, width - 50, row_h, PK_INPUT_BG);
                }
                canvas.draw_text_vcenter(
                    36,
                    ry,
                    90,
                    row_h,
                    &p.pid.to_string(),
                    12.0,
                    false,
                    PK_MUTED,
                );
                canvas.draw_text_vcenter(
                    136,
                    ry,
                    width - 390,
                    row_h,
                    &p.name,
                    12.5,
                    false,
                    PK_TEXT,
                );
                let mem_str = if p.mem_kb >= 1024 {
                    format!("{:.1} MB", (p.mem_kb as f64) / 1024.0)
                } else {
                    format!("{} KB", p.mem_kb)
                };
                canvas.draw_text_vcenter(
                    width - 240,
                    ry,
                    110,
                    row_h,
                    &mem_str,
                    12.0,
                    false,
                    PK_MUTED,
                );
                canvas.draw_rounded_box(width - 120, ry + 3, 84, row_h - 6, PK_RED_TINT, PK_RED, 4);
                canvas.draw_text_centered(
                    width - 120,
                    ry + 3,
                    84,
                    row_h - 6,
                    "End Task",
                    11.5,
                    true,
                    PK_RED,
                );
            }
        }
        AdminSubTab::Services => {
            canvas.fill_rect(25, body_y + 1, width - 50, 30, PK_CARD_BORDER);
            canvas.draw_text_vcenter(36, body_y + 1, 200, 30, "Service Name", 12.5, true, PK_TEXT);
            canvas.draw_text_vcenter(250, body_y + 1, 300, 30, "Display Name", 12.5, true, PK_TEXT);
            canvas.draw_text_vcenter(
                width - 140,
                body_y + 1,
                100,
                30,
                "Status",
                12.5,
                true,
                PK_TEXT,
            );

            let row_h = 28;
            let max_rows = ((body_h - 36) / row_h).max(1) as usize;
            for (idx, s) in services.iter().take(max_rows).enumerate() {
                let ry = body_y + 32 + (idx as i32) * row_h;
                if idx % 2 == 1 {
                    canvas.fill_rect(25, ry, width - 50, row_h, PK_INPUT_BG);
                }
                canvas.draw_text_vcenter(36, ry, 200, row_h, &s.name, 12.0, true, PK_TEXT);
                canvas.draw_text_vcenter(
                    250,
                    ry,
                    width - 400,
                    row_h,
                    &s.display_name,
                    12.0,
                    false,
                    PK_MUTED,
                );
                canvas.draw_text_vcenter(
                    width - 140,
                    ry,
                    100,
                    row_h,
                    &s.status,
                    12.0,
                    true,
                    if s.status.eq_ignore_ascii_case("Running") {
                        PK_GREEN_DARK
                    } else {
                        PK_MUTED
                    },
                );
            }
        }
        AdminSubTab::Terminal => {
            let line_h = 18;
            let max_lines = ((body_h - 56) / line_h).max(1) as usize;
            let start = term_lines.len().saturating_sub(max_lines);
            for (i, line) in term_lines[start..].iter().enumerate() {
                canvas.draw_text(
                    38,
                    body_y + 14 + (i as i32) * line_h,
                    line,
                    12.5,
                    false,
                    PK_TEXT,
                    Some(width - 76),
                );
            }

            let in_y = body_y + body_h - 42;
            canvas.draw_rounded_box(
                36,
                in_y,
                width - 72,
                32,
                PK_INPUT_BG,
                PK_GREEN,
                6,
            );
            let prompt = format!("PS> {}_", term_in);
            canvas.draw_text_vcenter(48, in_y, width - 96, 32, &prompt, 13.0, true, PK_TEXT);
        }
        AdminSubTab::SystemInfo => {
            if let Some(st) = stats {
                canvas.draw_text(
                    40,
                    body_y + 24,
                    &format!("Hostname:      {}", st.hostname),
                    14.0,
                    true,
                    PK_TEXT,
                    Some(width - 80),
                );
                canvas.draw_text(
                    40,
                    body_y + 54,
                    &format!("OS Version:    {}", st.os_name),
                    13.5,
                    false,
                    PK_TEXT,
                    Some(width - 80),
                );
                canvas.draw_text(
                    40,
                    body_y + 84,
                    &format!("CPU Usage:     {:.1}%", st.cpu_usage_pct),
                    13.5,
                    false,
                    PK_TEXT,
                    Some(width - 80),
                );
                canvas.draw_text(
                    40,
                    body_y + 114,
                    &format!(
                        "Memory:        {} MB / {} MB",
                        st.ram_used_mb, st.ram_total_mb
                    ),
                    13.5,
                    false,
                    PK_TEXT,
                    Some(width - 80),
                );
            } else {
                canvas.draw_text(
                    40,
                    body_y + 24,
                    "Click [Refresh] to query remote system telemetry...",
                    13.5,
                    false,
                    PK_MUTED,
                    Some(width - 80),
                );
            }
        }
    }

    if let Some((msg, when)) = status_msg {
        if when.elapsed().as_secs() < 4 {
            canvas.draw_rounded_box(width - 320, height - 48, 290, 32, PK_GREEN_TINT, PK_GREEN, 6);
            canvas.draw_text_centered(
                width - 320,
                height - 48,
                290,
                32,
                &msg,
                12.0,
                true,
                PK_GREEN_DARK,
            );
        }
    }
}

pub fn handle_admin_click(mx: i16, my: i16, win_w: u16, _win_h: u16) -> bool {
    let x = mx as i32;
    let y = my as i32;
    let width = win_w as i32;
    let bar_y = 48;

    if y >= bar_y + 4 && y <= bar_y + 32 {
        let tabs = [
            AdminSubTab::Processes,
            AdminSubTab::Services,
            AdminSubTab::Terminal,
            AdminSubTab::SystemInfo,
        ];
        let mut cur_x = 32;
        for tab_kind in tabs {
            if x >= cur_x && x <= cur_x + 118 {
                if let Ok(mut s) = get_admin_state().lock() {
                    s.sub_tab = tab_kind;
                }
                match tab_kind {
                    AdminSubTab::Processes => crate::p2p::send_admin_request(AdminReq::GetProcesses),
                    AdminSubTab::Services => crate::p2p::send_admin_request(AdminReq::GetServices),
                    AdminSubTab::SystemInfo => {
                        crate::p2p::send_admin_request(AdminReq::GetSystemInfo)
                    }
                    _ => {}
                }
                return true;
            }
            cur_x += 126;
        }

        let ref_w = 92;
        let ref_x = width - 24 - ref_w - 6;
        if x >= ref_x && x <= ref_x + ref_w {
            let sub = get_admin_state()
                .lock()
                .map(|s| s.sub_tab)
                .unwrap_or(AdminSubTab::Processes);
            match sub {
                AdminSubTab::Processes => crate::p2p::send_admin_request(AdminReq::GetProcesses),
                AdminSubTab::Services => crate::p2p::send_admin_request(AdminReq::GetServices),
                AdminSubTab::SystemInfo => crate::p2p::send_admin_request(AdminReq::GetSystemInfo),
                _ => {}
            }
            return true;
        }
    }

    // Check End Task click in Processes table
    let (sub_tab, processes) = {
        let s = get_admin_state().lock().unwrap_or_else(|e| e.into_inner());
        (s.sub_tab, s.processes.clone())
    };
    if sub_tab == AdminSubTab::Processes && x >= width - 120 && x <= width - 36 && y >= 126 {
        let row_idx = ((y - 126) / 28) as usize;
        if let Some(p) = processes.get(row_idx) {
            crate::p2p::send_admin_request(AdminReq::KillProcess { pid: p.pid });
            return true;
        }
    }

    false
}

pub fn handle_admin_key(keycode: u8, state: KeyButMask) {
    let Ok(mut s) = get_admin_state().lock() else {
        return;
    };
    if s.sub_tab != AdminSubTab::Terminal {
        return;
    }
    match keycode {
        22 => {
            s.terminal_input.pop();
        }
        36 | 104 => {
            let cmd = s.terminal_input.trim().to_string();
            if !cmd.is_empty() {
                s.terminal_lines.push(format!("PS> {}", cmd));
                s.terminal_input.clear();
                crate::p2p::send_admin_request(AdminReq::ExecCommand { cmd });
            }
        }
        _ => {
            let shift = state.contains(KeyButMask::SHIFT);
            if let Some(ch) = x11_keycode_to_ascii(keycode, shift) {
                s.terminal_input.push(ch);
            }
        }
    }
}
