//! Silent Admin & Background Management UI View
//! Renders background process manager, interactive PowerShell console and system telemetry
//! without interrupting or hijacking the remote user's screen or mouse.

#![allow(unsafe_op_in_unsafe_fn)]

use crate::protocol::{AdminReq, AdminResp, ProcessItem, ServiceItem, SystemStats};
use super::theme::{
    create_mono_font, create_segoe_font, draw_rounded_box, PK_BG, PK_BTN_HOVER, PK_CARD,
    PK_CARD_BORDER, PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_RED,
    PK_TEXT,
};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    AlphaBlend, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC,
    DeleteObject, DrawTextW, FillRect, FrameRect, SelectObject, SetBkColor, SetBkMode,
    SetTextColor, AC_SRC_OVER, BLENDFUNCTION, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, FW_BOLD, FW_NORMAL, FW_SEMIBOLD, HDC, HGDIOBJ, OPAQUE,
    TRANSPARENT,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminSubTab {
    Processes,
    Services,
    Terminal,
    SystemInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmModalKind {
    Restart,
    SafeModeRestart,
}

pub struct AdminState {
    pub sub_tab: AdminSubTab,
    pub processes: Vec<ProcessItem>,
    pub process_filter: String,
    pub filter_active: bool,
    pub scroll_y: i32,
    pub services: Vec<ServiceItem>,
    pub service_filter: String,
    pub service_filter_active: bool,
    pub service_scroll_y: i32,
    pub terminal_lines: Vec<String>,
    pub terminal_input: String,
    pub terminal_scroll_y: i32,
    pub system_stats: Option<SystemStats>,
    pub status_msg: Option<(String, Instant)>,
    pub is_loading: bool,
    pub last_req_time: Option<Instant>,
    pub confirm_modal: Option<ConfirmModalKind>,
}

impl Default for AdminState {
    fn default() -> Self {
        Self {
            sub_tab: AdminSubTab::Processes,
            processes: Vec::new(),
            process_filter: String::new(),
            filter_active: false,
            scroll_y: 0,
            services: Vec::new(),
            service_filter: String::new(),
            service_filter_active: false,
            service_scroll_y: 0,
            terminal_lines: vec![
                "[Admin] Desky Silent Remote Management Console - PowerShell Engine".into(),
                "Commands execute with zero remote screen or mouse interruption.".into(),
                "Type any PowerShell/CMD command below and press Enter (e.g. dir, ipconfig, Get-Service):".into(),
            ],
            terminal_input: String::new(),
            terminal_scroll_y: 0,
            system_stats: None,
            status_msg: None,
            is_loading: false,
            last_req_time: None,
            confirm_modal: None,
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
            state.is_loading = false;
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
                    state.status_msg = Some((format!("Loaded {} Windows services", count), Instant::now()));
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
                        state.terminal_lines.push(format!("[Process returned exit code {}]", exit_code));
                    }
                    state.terminal_lines.push(String::new());
                    state.terminal_scroll_y = 0;
                }
                AdminResp::SystemInfo(stats) => {
                    state.system_stats = Some(stats);
                    state.status_msg = Some(("System telemetry updated".into(), Instant::now()));
                }
                AdminResp::MonitorSwitched { monitor_index, count } => {
                    state.status_msg = Some((format!("Switched to Monitor #{} (Total: {})", monitor_index + 1, count), Instant::now()));
                }
                AdminResp::RebootScheduled { safe_mode } => {
                    state.status_msg = Some((format!("Host reboot scheduled (Safe Mode: {}). Desky will reconnect automatically...", safe_mode), Instant::now()));
                }
                AdminResp::PrivacyModeChanged { enabled } => {
                    state.status_msg = Some((if enabled { "Host Privacy Screen & Input Lock ENABLED".into() } else { "Host Privacy Screen & Input Lock DISABLED".into() }, Instant::now()));
                }
                AdminResp::WakeOnLanSent { mac, success } => {
                    state.status_msg = Some((if success { format!("Magic packet broadcasted to {}", mac) } else { format!("Failed to send WoL packet to {}", mac) }, Instant::now()));
                }
                AdminResp::SystemActionResult { action: _, success: _, msg } => {
                    state.status_msg = Some((msg, Instant::now()));
                }
            }
        }
    }
}

pub unsafe fn draw_admin_view(hdc: HDC, width: i32, height: i32, hover_state: i32) {
    poll_admin_responses();

    let bg_brush = CreateSolidBrush(PK_BG);
    let full_rc = RECT { left: 0, top: 40, right: width, bottom: height };
    FillRect(hdc, &full_rc, bg_brush);
    let _ = DeleteObject(HGDIOBJ(bg_brush.0));

    let sub_tab = {
        let state = get_admin_state().lock().unwrap();
        state.sub_tab
    };

    draw_sub_tab_bar(hdc, width, sub_tab, hover_state);

    match sub_tab {
        AdminSubTab::Processes => draw_processes_view(hdc, width, height, hover_state),
        AdminSubTab::Services => draw_services_view(hdc, width, height, hover_state),
        AdminSubTab::Terminal => draw_terminal_view(hdc, width, height),
        AdminSubTab::SystemInfo => draw_system_info_view(hdc, width, height, hover_state),
    }

    draw_status_toast(hdc, width, height);
    draw_reboot_confirmation_modal(hdc, width, height, hover_state);
}

unsafe fn draw_sub_tab_bar(hdc: HDC, width: i32, current_tab: AdminSubTab, hover_state: i32) {
    let bar_y = 48;
    let bar_h = 36;
    let card_rc = RECT { left: 24, top: bar_y, right: width - 24, bottom: bar_y + bar_h };
    let bar_brush = CreateSolidBrush(PK_CARD);
    FillRect(hdc, &card_rc, bar_brush);
    let _ = DeleteObject(HGDIOBJ(bar_brush.0));

    let border_brush = CreateSolidBrush(PK_CARD_BORDER);
    FrameRect(hdc, &card_rc, border_brush);
    let _ = DeleteObject(HGDIOBJ(border_brush.0));

    let tabs = [
        (AdminSubTab::Processes, "Processes"),
        (AdminSubTab::Services, "Services"),
        (AdminSubTab::Terminal, "PowerShell"),
        (AdminSubTab::SystemInfo, "Telemetry"),
    ];

    let font = create_segoe_font(13, FW_SEMIBOLD);
    let old_font = SelectObject(hdc, HGDIOBJ(font.0));
    SetBkMode(hdc, TRANSPARENT);

    let mut cur_x = 32;
    for (i, (tab_kind, label)) in tabs.iter().enumerate() {
        let is_active = *tab_kind == current_tab;
        let is_hover = hover_state == (2000 + i as i32);
        let tab_w = 120;

        let bg = if is_active {
            PK_GREEN_TINT
        } else if is_hover {
            PK_BTN_HOVER
        } else {
            PK_CARD
        };
        let fg = if is_active {
            PK_GREEN_DARK
        } else if is_hover {
            PK_TEXT
        } else {
            PK_MUTED
        };

        draw_rounded_box(hdc, cur_x, bar_y + 4, tab_w, 28, bg, if is_active { PK_GREEN } else { PK_CARD_BORDER }, 4);

        // Render Lucide Vector Icon for Sub-Tab
        match tab_kind {
            AdminSubTab::Processes => crate::ui::draw_cpu(hdc, cur_x + 8, bar_y + 10, 16, fg, 2),
            AdminSubTab::Services => crate::ui::draw_server(hdc, cur_x + 8, bar_y + 10, 16, fg, 2),
            AdminSubTab::Terminal => crate::ui::draw_terminal(hdc, cur_x + 8, bar_y + 10, 16, fg, 2),
            AdminSubTab::SystemInfo => crate::ui::draw_info(hdc, cur_x + 8, bar_y + 10, 16, fg, 2),
        }

        SetTextColor(hdc, fg);
        let mut txt: Vec<u16> = label.encode_utf16().collect();
        let mut rc = RECT { left: cur_x + 24, top: bar_y + 4, right: cur_x + tab_w - 4, bottom: bar_y + 32 };
        let _ = DrawTextW(hdc, &mut txt, &mut rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

        cur_x += tab_w + 8;
    }

    // Power Actions & Refresh on the far right
    let ref_w = 92;
    let ref_x = width - 24 - ref_w - 6;
    let is_ref_hover = hover_state == 2999;
    let ref_bg = if is_ref_hover { PK_GREEN_TINT } else { PK_CARD };
    draw_rounded_box(hdc, ref_x, bar_y + 4, ref_w, 28, ref_bg, PK_GREEN, 4);
    crate::ui::draw_refresh(hdc, ref_x + 8, bar_y + 10, 16, PK_GREEN_DARK, 2);
    SetTextColor(hdc, PK_GREEN_DARK);
    let mut ref_txt: Vec<u16> = "Refresh".encode_utf16().collect();
    let mut ref_rc = RECT { left: ref_x + 22, top: bar_y + 4, right: ref_x + ref_w, bottom: bar_y + 32 };
    let _ = DrawTextW(hdc, &mut ref_txt, &mut ref_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    let reboot_w = 92;
    let reboot_x = ref_x - reboot_w - 8;
    let is_reboot_hover = hover_state == 2998;
    let reboot_bg = if is_reboot_hover { PK_BTN_HOVER } else { PK_CARD };
    draw_rounded_box(hdc, reboot_x, bar_y + 4, reboot_w, 28, reboot_bg, PK_CARD_BORDER, 4);
    SetTextColor(hdc, PK_TEXT);
    let mut reb_txt: Vec<u16> = "Restart".encode_utf16().collect();
    let mut reb_rc = RECT { left: reboot_x, top: bar_y + 4, right: reboot_x + reboot_w, bottom: bar_y + 32 };
    let _ = DrawTextW(hdc, &mut reb_txt, &mut reb_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    let safe_w = 98;
    let safe_x = reboot_x - safe_w - 8;
    let is_safe_hover = hover_state == 2997;
    let safe_bg = if is_safe_hover { PK_BTN_HOVER } else { PK_CARD };
    draw_rounded_box(hdc, safe_x, bar_y + 4, safe_w, 28, safe_bg, PK_CARD_BORDER, 4);
    SetTextColor(hdc, PK_MUTED);
    let mut safe_txt: Vec<u16> = "Safe Mode".encode_utf16().collect();
    let mut safe_rc = RECT { left: safe_x, top: bar_y + 4, right: safe_x + safe_w, bottom: bar_y + 32 };
    let _ = DrawTextW(hdc, &mut safe_txt, &mut safe_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    let last_time = {
        let state = get_admin_state().lock().unwrap();
        state.last_req_time
    };
    let ref_label = if let Some(t) = last_time {
        if t.elapsed().as_secs() < 3 {
            "Refreshed"
        } else {
            "Refresh"
        }
    } else {
        "Refresh"
    };

    let mut ref_txt: Vec<u16> = ref_label.encode_utf16().collect();
    let mut ref_rc = RECT { left: ref_x + 26, top: bar_y + 4, right: ref_x + ref_w, bottom: bar_y + 32 };
    let _ = DrawTextW(hdc, &mut ref_txt, &mut ref_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(HGDIOBJ(font.0));
}

unsafe fn draw_processes_view(hdc: HDC, width: i32, height: i32, hover_state: i32) {
    let top_y = 96;
    let table_w = width - 48;
    let table_h = height - top_y - 24;
    if table_h <= 60 {
        return;
    }

    // Search bar box
    let search_h = 32;
    let search_rc = RECT { left: 24, top: top_y, right: width - 24, bottom: top_y + search_h };
    let s_brush = CreateSolidBrush(PK_INPUT_BG);
    FillRect(hdc, &search_rc, s_brush);
    let _ = DeleteObject(HGDIOBJ(s_brush.0));
    let s_border = CreateSolidBrush(PK_CARD_BORDER);
    FrameRect(hdc, &search_rc, s_border);
    let _ = DeleteObject(HGDIOBJ(s_border.0));

    let font_body = create_segoe_font(13, FW_NORMAL);
    let font_bold = create_segoe_font(13, FW_SEMIBOLD);
    let font_mono = create_mono_font(12, FW_NORMAL);
    let old_font = SelectObject(hdc, HGDIOBJ(font_body.0));
    SetBkMode(hdc, TRANSPARENT);

    // Search text label (with Lucide Search vector icon)
    let (filter_text, filter_active, processes, scroll_y) = {
        let state = get_admin_state().lock().unwrap();
        (
            state.process_filter.clone(),
            state.filter_active,
            state.processes.clone(),
            state.scroll_y,
        )
    };

    crate::ui::draw_search(hdc, 34, top_y + 8, 16, PK_MUTED, 2);

    SetTextColor(hdc, if filter_text.is_empty() { PK_MUTED } else { PK_TEXT });
    let disp_filter = if filter_text.is_empty() {
        "Filter processes by name or PID (type to search)...".to_string()
    } else {
        format!("Filter: {}{}", filter_text, if filter_active { "_" } else { "" })
    };
    let mut f_txt: Vec<u16> = disp_filter.encode_utf16().collect();
    let mut f_rc = RECT { left: 58, top: top_y, right: width - 36, bottom: top_y + search_h };
    let _ = DrawTextW(hdc, &mut f_txt, &mut f_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    // Table Header
    let head_y = top_y + search_h + 8;
    let head_h = 30;
    let head_rc = RECT { left: 24, top: head_y, right: 24 + table_w, bottom: head_y + head_h };
    let head_brush = CreateSolidBrush(PK_CARD_BORDER);
    FillRect(hdc, &head_rc, head_brush);
    let _ = DeleteObject(HGDIOBJ(head_brush.0));

    let _ = SelectObject(hdc, HGDIOBJ(font_bold.0));
    SetTextColor(hdc, PK_TEXT);

    let cols = [
        (36, 100, "PID"),
        (136, table_w - 360, "Process Name"),
        (table_w - 224, 110, "Memory Usage"),
        (table_w - 114, 100, "Action"),
    ];

    for (cx, cw, title) in cols {
        let mut t: Vec<u16> = title.encode_utf16().collect();
        let mut rc = RECT { left: cx, top: head_y, right: cx + cw, bottom: head_y + head_h };
        let _ = DrawTextW(hdc, &mut t, &mut rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    }

    // Filter processes
    let lower_filter = filter_text.to_lowercase();
    let filtered: Vec<&ProcessItem> = processes
        .iter()
        .filter(|p| {
            if lower_filter.is_empty() {
                true
            } else {
                p.name.to_lowercase().contains(&lower_filter)
                    || p.pid.to_string().contains(&lower_filter)
            }
        })
        .collect();

    // Table rows
    let rows_y = head_y + head_h;
    let max_rows_h = height - rows_y - 20;
    let row_h = 28;
    let visible_rows = (max_rows_h / row_h).max(1);

    let start_idx = (scroll_y / row_h).max(0) as usize;
    let end_idx = (start_idx + visible_rows as usize + 1).min(filtered.len());

    for (i, p) in filtered.iter().enumerate().skip(start_idx).take(end_idx - start_idx) {
        let ry = rows_y + ((i as i32) * row_h - scroll_y);
        if ry + row_h > height - 16 || ry < rows_y {
            continue;
        }

        let is_hover = hover_state == (3000 + i as i32);
        let is_kill_hover = hover_state == (4000 + i as i32);

        let row_bg = if is_hover {
            PK_GREEN_TINT
        } else if i % 2 == 0 {
            PK_CARD
        } else {
            PK_INPUT_BG
        };

        let row_rc = RECT { left: 24, top: ry, right: 24 + table_w, bottom: ry + row_h };
        let r_brush = CreateSolidBrush(row_bg);
        FillRect(hdc, &row_rc, r_brush);
        let _ = DeleteObject(HGDIOBJ(r_brush.0));

        // PID
        let _ = SelectObject(hdc, HGDIOBJ(font_mono.0));
        SetTextColor(hdc, PK_MUTED);
        let mut pid_txt: Vec<u16> = format!("{:>5}", p.pid).encode_utf16().collect();
        let mut pid_rc = RECT { left: 36, top: ry, right: 126, bottom: ry + row_h };
        let _ = DrawTextW(hdc, &mut pid_txt, &mut pid_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

        // Name
        let _ = SelectObject(hdc, HGDIOBJ(font_body.0));
        SetTextColor(hdc, PK_TEXT);
        let mut name_txt: Vec<u16> = p.name.encode_utf16().collect();
        let mut name_rc = RECT { left: 136, top: ry, right: table_w - 230, bottom: ry + row_h };
        let _ = DrawTextW(hdc, &mut name_txt, &mut name_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);

        // Memory
        let mem_str = if p.mem_kb >= 1024 {
            format!("{:.1} MB", (p.mem_kb as f64) / 1024.0)
        } else {
            format!("{} KB", p.mem_kb)
        };
        let _ = SelectObject(hdc, HGDIOBJ(font_mono.0));
        SetTextColor(hdc, PK_MUTED);
        let mut mem_txt: Vec<u16> = mem_str.encode_utf16().collect();
        let mut mem_rc = RECT { left: table_w - 224, top: ry, right: table_w - 120, bottom: ry + row_h };
        let _ = DrawTextW(hdc, &mut mem_txt, &mut mem_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

        // Kill Button (Lucide Trash-2 vector)
        let btn_x = table_w - 114;
        let btn_w = 90;
        let btn_bg = if is_kill_hover { COLORREF(0x00EBF0FF) } else { PK_CARD };
        draw_rounded_box(hdc, btn_x, ry + 3, btn_w, row_h - 6, btn_bg, PK_RED, 4);
        crate::ui::draw_trash(hdc, btn_x + 8, ry + 6, 14, PK_RED, 2);

        let _ = SelectObject(hdc, HGDIOBJ(font_bold.0));
        SetTextColor(hdc, PK_RED);
        let mut kill_txt: Vec<u16> = "End Task".encode_utf16().collect();
        let mut kill_rc = RECT { left: btn_x + 24, top: ry + 3, right: btn_x + btn_w, bottom: ry + row_h - 3 };
        let _ = DrawTextW(hdc, &mut kill_txt, &mut kill_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    }

    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(HGDIOBJ(font_body.0));
    let _ = DeleteObject(HGDIOBJ(font_bold.0));
    let _ = DeleteObject(HGDIOBJ(font_mono.0));
}

unsafe fn draw_services_view(hdc: HDC, width: i32, height: i32, hover_state: i32) {
    let top_y = 96;
    let table_w = width - 48;
    let table_h = height - top_y - 24;
    if table_h <= 60 {
        return;
    }

    // Search bar box
    let search_h = 32;
    let search_rc = RECT { left: 24, top: top_y, right: width - 24, bottom: top_y + search_h };
    let s_brush = CreateSolidBrush(PK_INPUT_BG);
    FillRect(hdc, &search_rc, s_brush);
    let _ = DeleteObject(HGDIOBJ(s_brush.0));
    let s_border = CreateSolidBrush(PK_CARD_BORDER);
    FrameRect(hdc, &search_rc, s_border);
    let _ = DeleteObject(HGDIOBJ(s_border.0));

    let font_body = create_segoe_font(13, FW_NORMAL);
    let font_bold = create_segoe_font(13, FW_SEMIBOLD);
    let font_mono = create_mono_font(12, FW_NORMAL);
    let old_font = SelectObject(hdc, HGDIOBJ(font_body.0));
    SetBkMode(hdc, TRANSPARENT);

    let (filter_text, filter_active, services, scroll_y) = {
        let state = get_admin_state().lock().unwrap();
        (
            state.service_filter.clone(),
            state.service_filter_active,
            state.services.clone(),
            state.service_scroll_y,
        )
    };

    crate::ui::draw_search(hdc, 34, top_y + 8, 16, PK_MUTED, 2);

    SetTextColor(hdc, if filter_text.is_empty() { PK_MUTED } else { PK_TEXT });
    let disp_filter = if filter_text.is_empty() {
        "Filter Windows services by name, display name, or status (type to search)...".to_string()
    } else {
        format!("Filter: {}{}", filter_text, if filter_active { "_" } else { "" })
    };
    let mut f_txt: Vec<u16> = disp_filter.encode_utf16().collect();
    let mut f_rc = RECT { left: 58, top: top_y, right: width - 36, bottom: top_y + search_h };
    let _ = DrawTextW(hdc, &mut f_txt, &mut f_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    // Table Header
    let head_y = top_y + search_h + 8;
    let head_h = 30;
    let head_rc = RECT { left: 24, top: head_y, right: 24 + table_w, bottom: head_y + head_h };
    let head_brush = CreateSolidBrush(PK_CARD_BORDER);
    FillRect(hdc, &head_rc, head_brush);
    let _ = DeleteObject(HGDIOBJ(head_brush.0));

    let _ = SelectObject(hdc, HGDIOBJ(font_bold.0));
    SetTextColor(hdc, PK_TEXT);

    let cols = [
        (36, 180, "Service Name"),
        (226, table_w - 530, "Display Name"),
        (table_w - 294, 90, "Status"),
        (table_w - 194, 180, "Actions"),
    ];

    for (cx, cw, title) in cols {
        let mut t: Vec<u16> = title.encode_utf16().collect();
        let mut rc = RECT { left: cx, top: head_y, right: cx + cw, bottom: head_y + head_h };
        let _ = DrawTextW(hdc, &mut t, &mut rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    }

    // Filter services
    let lower_filter = filter_text.to_lowercase();
    let filtered: Vec<&ServiceItem> = services
        .iter()
        .filter(|s| {
            if lower_filter.is_empty() {
                true
            } else {
                s.name.to_lowercase().contains(&lower_filter)
                    || s.display_name.to_lowercase().contains(&lower_filter)
                    || s.status.to_lowercase().contains(&lower_filter)
            }
        })
        .collect();

    // Table rows
    let rows_y = head_y + head_h;
    let max_rows_h = height - rows_y - 20;
    let row_h = 30;
    let visible_rows = (max_rows_h / row_h).max(1);

    let start_idx = (scroll_y / row_h).max(0) as usize;
    let end_idx = (start_idx + visible_rows as usize + 1).min(filtered.len());

    for (i, s) in filtered.iter().enumerate().skip(start_idx).take(end_idx - start_idx) {
        let ry = rows_y + ((i as i32) * row_h - scroll_y);
        if ry + row_h > height - 16 || ry < rows_y {
            continue;
        }

        let is_hover = hover_state == (5000 + i as i32);
        let row_bg = if is_hover {
            PK_GREEN_TINT
        } else if i % 2 == 0 {
            PK_CARD
        } else {
            PK_INPUT_BG
        };

        let row_rc = RECT { left: 24, top: ry, right: 24 + table_w, bottom: ry + row_h };
        let r_brush = CreateSolidBrush(row_bg);
        FillRect(hdc, &row_rc, r_brush);
        let _ = DeleteObject(HGDIOBJ(r_brush.0));

        // Name
        let _ = SelectObject(hdc, HGDIOBJ(font_mono.0));
        SetTextColor(hdc, PK_TEXT);
        let mut name_txt: Vec<u16> = s.name.encode_utf16().collect();
        let mut name_rc = RECT { left: 36, top: ry, right: 216, bottom: ry + row_h };
        let _ = DrawTextW(hdc, &mut name_txt, &mut name_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);

        // Display Name
        let _ = SelectObject(hdc, HGDIOBJ(font_body.0));
        SetTextColor(hdc, PK_MUTED);
        let mut disp_txt: Vec<u16> = s.display_name.encode_utf16().collect();
        let mut disp_rc = RECT { left: 226, top: ry, right: table_w - 304, bottom: ry + row_h };
        let _ = DrawTextW(hdc, &mut disp_txt, &mut disp_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);

        // Status Badge
        let is_running = s.status.eq_ignore_ascii_case("running");
        let (st_bg, st_border, st_fg) = if is_running {
            (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
        } else {
            (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
        };
        let st_w = 72;
        let st_x = table_w - 294;
        draw_rounded_box(hdc, st_x, ry + 4, st_w, row_h - 8, st_bg, st_border, 4);

        let _ = SelectObject(hdc, HGDIOBJ(font_bold.0));
        SetTextColor(hdc, st_fg);
        let mut st_txt: Vec<u16> = (if is_running { "RUNNING" } else { "STOPPED" }).encode_utf16().collect();
        let mut st_rc = RECT { left: st_x, top: ry + 4, right: st_x + st_w, bottom: ry + row_h - 4 };
        let _ = DrawTextW(hdc, &mut st_txt, &mut st_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

        // Actions: [ Start / Stop ] and [ Restart ]
        let act_x = table_w - 194;
        let btn1_w = 56;
        let btn2_w = 64;

        let is_b1_hover = hover_state == (6000 + i as i32);
        let is_b2_hover = hover_state == (7000 + i as i32);

        // Button 1: Start (if stopped) or Stop (if running)
        let (b1_txt_str, b1_fg, b1_border) = if is_running {
            ("Stop", PK_RED, PK_RED)
        } else {
            ("Start", PK_GREEN_DARK, PK_GREEN)
        };
        let b1_bg = if is_b1_hover { PK_BTN_HOVER } else { PK_CARD };
        draw_rounded_box(hdc, act_x, ry + 3, btn1_w, row_h - 6, b1_bg, b1_border, 4);
        SetTextColor(hdc, b1_fg);
        let mut b1_txt: Vec<u16> = b1_txt_str.encode_utf16().collect();
        let mut b1_rc = RECT { left: act_x, top: ry + 3, right: act_x + btn1_w, bottom: ry + row_h - 3 };
        let _ = DrawTextW(hdc, &mut b1_txt, &mut b1_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

        // Button 2: Restart
        let b2_x = act_x + btn1_w + 6;
        let b2_bg = if is_b2_hover { PK_BTN_HOVER } else { PK_CARD };
        draw_rounded_box(hdc, b2_x, ry + 3, btn2_w, row_h - 6, b2_bg, PK_CARD_BORDER, 4);
        SetTextColor(hdc, PK_TEXT);
        let mut b2_txt: Vec<u16> = "Restart".encode_utf16().collect();
        let mut b2_rc = RECT { left: b2_x, top: ry + 3, right: b2_x + btn2_w, bottom: ry + row_h - 3 };
        let _ = DrawTextW(hdc, &mut b2_txt, &mut b2_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    }

    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(HGDIOBJ(font_body.0));
    let _ = DeleteObject(HGDIOBJ(font_bold.0));
    let _ = DeleteObject(HGDIOBJ(font_mono.0));
}

unsafe fn draw_terminal_view(hdc: HDC, width: i32, height: i32) {
    let term_x = 24;
    let term_y = 96;
    let term_w = width - 48;
    let term_h = height - term_y - 24;
    if term_h <= 100 {
        return;
    }

    // Terminal container (Dark Charcoal Theme)
    let term_bg = COLORREF(0x001B1818); // Dark graphite
    let term_rc = RECT { left: term_x, top: term_y, right: term_x + term_w, bottom: term_y + term_h };
    let bg_brush = CreateSolidBrush(term_bg);
    FillRect(hdc, &term_rc, bg_brush);
    let _ = DeleteObject(HGDIOBJ(bg_brush.0));

    let border_brush = CreateSolidBrush(PK_GREEN_DARK);
    FrameRect(hdc, &term_rc, border_brush);
    let _ = DeleteObject(HGDIOBJ(border_brush.0));

    let font_term = create_mono_font(13, FW_NORMAL);
    let old_font = SelectObject(hdc, HGDIOBJ(font_term.0));
    SetBkMode(hdc, OPAQUE);
    SetBkColor(hdc, term_bg);

    let (lines, input, scroll_y) = {
        let state = get_admin_state().lock().unwrap();
        (
            state.terminal_lines.clone(),
            state.terminal_input.clone(),
            state.terminal_scroll_y,
        )
    };

    let line_h = 18;
    let prompt_y = term_y + term_h - 26;
    let output_max_h = prompt_y - term_y - 12;
    let visible_lines = (output_max_h / line_h).max(1) as usize;

    let start_idx = if lines.len() > visible_lines {
        ((lines.len() - visible_lines) as i32 - scroll_y).max(0) as usize
    } else {
        0
    };

    let mut cy = term_y + 10;
    for line in lines.iter().skip(start_idx).take(visible_lines) {
        if cy + line_h > prompt_y {
            break;
        }
        if line.is_empty() {
            cy += line_h;
            continue;
        }
        let is_prompt = line.starts_with("PS >") || line.starts_with("[Admin]");
        SetTextColor(hdc, if is_prompt { COLORREF(0x0099D334) } else { COLORREF(0x00F6F4F3) });
        let mut t: Vec<u16> = line.encode_utf16().collect();
        if !t.is_empty() {
            let mut rc = RECT { left: term_x + 14, top: cy, right: term_x + term_w - 14, bottom: cy + line_h };
            let _ = DrawTextW(hdc, &mut t, &mut rc, DT_LEFT | DT_SINGLELINE | DT_NOPREFIX);
        }
        cy += line_h;
    }

    // Input Prompt Line at the bottom
    let prompt_rc = RECT { left: term_x + 6, top: prompt_y - 4, right: term_x + term_w - 6, bottom: prompt_y + 22 };
    let p_brush = CreateSolidBrush(COLORREF(0x00272222));
    FillRect(hdc, &prompt_rc, p_brush);
    let _ = DeleteObject(HGDIOBJ(p_brush.0));

    SetBkColor(hdc, COLORREF(0x00272222));
    SetTextColor(hdc, COLORREF(0x0034D399)); // Emerald PS prompt
    let prompt_prefix = "PS C:\\> ";
    let mut p_txt: Vec<u16> = prompt_prefix.encode_utf16().collect();
    let mut prc = RECT { left: term_x + 14, top: prompt_y, right: term_x + 85, bottom: prompt_y + 20 };
    let _ = DrawTextW(hdc, &mut p_txt, &mut prc, DT_LEFT | DT_SINGLELINE);

    SetTextColor(hdc, COLORREF(0x00FFFFFF));
    let typed = format!("{}_", input);
    let mut in_txt: Vec<u16> = typed.encode_utf16().collect();
    if !in_txt.is_empty() {
        let mut in_rc = RECT { left: term_x + 86, top: prompt_y, right: term_x + term_w - 14, bottom: prompt_y + 20 };
        let _ = DrawTextW(hdc, &mut in_txt, &mut in_rc, DT_LEFT | DT_SINGLELINE | DT_NOPREFIX);
    }

    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(HGDIOBJ(font_term.0));
}

unsafe fn draw_system_info_view(hdc: HDC, width: i32, _height: i32, hover_state: i32) {
    let top_y = 96;
    let stats = {
        let state = get_admin_state().lock().unwrap();
        state.system_stats.clone()
    };

    let font_title = create_segoe_font(15, FW_BOLD);
    let font_label = create_segoe_font(12, FW_NORMAL);
    let font_val = create_segoe_font(18, FW_BOLD);
    let old_font = SelectObject(hdc, HGDIOBJ(font_title.0));
    SetBkMode(hdc, TRANSPARENT);

    let card_w = (width - 48 - 24) / 2;
    let card_h = 130;

    let stats_ref = stats.as_ref();
    let hostname = stats_ref.map(|s| s.hostname.as_str()).unwrap_or("Querying...");
    let os_name = stats_ref.map(|s| s.os_name.as_str()).unwrap_or("Windows Native");
    let cpus = stats_ref.map(|s| format!("{} Cores", s.cpu_count)).unwrap_or_else(|| "-- Cores".into());
    let uptime = stats_ref.map(|s| {
        let d = s.uptime_secs / 86400;
        let h = (s.uptime_secs % 86400) / 3600;
        let m = (s.uptime_secs % 3600) / 60;
        if d > 0 {
            format!("{}d {}h {}m", d, h, m)
        } else {
            format!("{}h {}m", h, m)
        }
    }).unwrap_or_else(|| "--".into());

    let (mem_used, mem_tot, mem_pct) = if let Some(s) = stats_ref {
        let used = s.total_mem_mb.saturating_sub(s.free_mem_mb);
        let pct = if s.total_mem_mb > 0 {
            ((used as f64 / s.total_mem_mb as f64) * 100.0) as u32
        } else {
            0
        };
        (used, s.total_mem_mb, pct)
    } else {
        (0, 0, 0)
    };

    let cards = [
        (24, top_y, "Host & Operating System", hostname, os_name, 0),
        (24 + card_w + 24, top_y, "CPU Architecture", &cpus, "Background execution active (0% screen load)", 1),
        (24, top_y + card_h + 16, "System Uptime", &uptime, "Remote service running normally", 2),
        (24 + card_w + 24, top_y + card_h + 16, "RAM Memory Usage", &format!("{}% Used", mem_pct), &format!("{:.1} GB / {:.1} GB", (mem_used as f64) / 1024.0, (mem_tot as f64) / 1024.0), 3),
    ];

    for (cx, cy, header, val, sub, icon_kind) in cards {
        draw_rounded_box(hdc, cx, cy, card_w, card_h, PK_CARD, PK_CARD_BORDER, 6);

        // Render Lucide Vector Card Icon
        match icon_kind {
            0 => crate::ui::draw_monitor(hdc, cx + 18, cy + 14, 18, PK_GREEN_DARK, 2),
            1 => crate::ui::draw_cpu(hdc, cx + 18, cy + 14, 18, PK_GREEN_DARK, 2),
            2 => crate::ui::draw_radio(hdc, cx + 18, cy + 14, 18, PK_GREEN_DARK, 2),
            _ => crate::ui::draw_lock(hdc, cx + 18, cy + 14, 18, PK_GREEN_DARK, 2),
        }

        let _ = SelectObject(hdc, HGDIOBJ(font_title.0));
        SetTextColor(hdc, PK_GREEN_DARK);
        let mut h_txt: Vec<u16> = header.encode_utf16().collect();
        let mut h_rc = RECT { left: cx + 42, top: cy + 14, right: cx + card_w - 18, bottom: cy + 34 };
        let _ = DrawTextW(hdc, &mut h_txt, &mut h_rc, DT_LEFT | DT_SINGLELINE);

        let _ = SelectObject(hdc, HGDIOBJ(font_val.0));
        SetTextColor(hdc, PK_TEXT);
        let mut v_txt: Vec<u16> = val.encode_utf16().collect();
        let mut v_rc = RECT { left: cx + 18, top: cy + 42, right: cx + card_w - 18, bottom: cy + 70 };
        let _ = DrawTextW(hdc, &mut v_txt, &mut v_rc, DT_LEFT | DT_SINGLELINE);

        let _ = SelectObject(hdc, HGDIOBJ(font_label.0));
        SetTextColor(hdc, PK_MUTED);
        let mut s_txt: Vec<u16> = sub.encode_utf16().collect();
        let mut s_rc = RECT { left: cx + 18, top: cy + 74, right: cx + card_w - 18, bottom: cy + 96 };
        let _ = DrawTextW(hdc, &mut s_txt, &mut s_rc, DT_LEFT | DT_SINGLELINE);

        // RAM gauge bar if card 4
        if header.contains("RAM") && mem_tot > 0 {
            let bar_y = cy + 104;
            let bar_w = card_w - 36;
            let bar_rc = RECT { left: cx + 18, top: bar_y, right: cx + 18 + bar_w, bottom: bar_y + 10 };
            let bb = CreateSolidBrush(PK_CARD_BORDER);
            FillRect(hdc, &bar_rc, bb);
            let _ = DeleteObject(HGDIOBJ(bb.0));

            let fill_w = ((bar_w as f64) * (mem_pct as f64 / 100.0)) as i32;
            let fill_rc = RECT { left: cx + 18, top: bar_y, right: cx + 18 + fill_w, bottom: bar_y + 10 };
            let fb = CreateSolidBrush(PK_GREEN);
            FillRect(hdc, &fill_rc, fb);
            let _ = DeleteObject(HGDIOBJ(fb.0));
        }
    }

    // 5. Wake-on-LAN (WoL) & Hardware Network Identity Card
    let net_cx = 24;
    let net_cy = top_y + (card_h + 16) * 2;
    let net_w = width - 48;
    let net_h = 76;
    draw_rounded_box(hdc, net_cx, net_cy, net_w, net_h, PK_CARD, PK_CARD_BORDER, 6);

    crate::ui::draw_server(hdc, net_cx + 18, net_cy + 16, 18, PK_GREEN_DARK, 2);

    let _ = SelectObject(hdc, HGDIOBJ(font_title.0));
    SetTextColor(hdc, PK_GREEN_DARK);
    let mut net_h_txt: Vec<u16> = "Wake-on-LAN & Physical Network Interface".encode_utf16().collect();
    let mut net_h_rc = RECT { left: net_cx + 42, top: net_cy + 14, right: net_cx + net_w - 240, bottom: net_cy + 34 };
    let _ = DrawTextW(hdc, &mut net_h_txt, &mut net_h_rc, DT_LEFT | DT_SINGLELINE);

    let mac_str = stats_ref.map(|s| if s.mac_addr.is_empty() { "Auto-Discovered Physical Interface (Online)" } else { s.mac_addr.as_str() }).unwrap_or("Querying host network adapter...");
    let _ = SelectObject(hdc, HGDIOBJ(font_val.0));
    SetTextColor(hdc, PK_TEXT);
    let mut mac_txt: Vec<u16> = mac_str.encode_utf16().collect();
    let mut mac_rc = RECT { left: net_cx + 18, top: net_cy + 40, right: net_cx + net_w - 240, bottom: net_cy + 66 };
    let _ = DrawTextW(hdc, &mut mac_txt, &mut mac_rc, DT_LEFT | DT_SINGLELINE);

    // [ Broadcast WoL Magic Packet ] Button
    let wol_btn_w = 210;
    let wol_btn_x = net_cx + net_w - wol_btn_w - 18;
    let is_wol_hover = hover_state == 2990;
    let wol_bg = if is_wol_hover { PK_GREEN_DARK } else { PK_GREEN };
    draw_rounded_box(hdc, wol_btn_x, net_cy + 18, wol_btn_w, 40, wol_bg, PK_GREEN, 6);
    let font_btn = create_segoe_font(12, FW_SEMIBOLD);
    let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
    SetTextColor(hdc, COLORREF(0x00FFFFFF));
    let mut wol_btn_txt: Vec<u16> = "Broadcast Wake-on-LAN".encode_utf16().collect();
    let mut wol_btn_rc = RECT { left: wol_btn_x, top: net_cy + 18, right: wol_btn_x + wol_btn_w, bottom: net_cy + 58 };
    let _ = DrawTextW(hdc, &mut wol_btn_txt, &mut wol_btn_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    let _ = DeleteObject(HGDIOBJ(font_btn.0));

    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(HGDIOBJ(font_title.0));
    let _ = DeleteObject(HGDIOBJ(font_val.0));
    let _ = DeleteObject(HGDIOBJ(font_label.0));
}

unsafe fn draw_reboot_confirmation_modal(hdc: HDC, width: i32, height: i32, hover_state: i32) {
    let modal_kind = {
        let state = get_admin_state().lock().unwrap();
        state.confirm_modal
    };

    let kind = match modal_kind {
        Some(k) => k,
        None => return,
    };

    // 1. Dim background
    let mem_dc = CreateCompatibleDC(Some(hdc));
    let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
    let old_bmp = SelectObject(mem_dc, HGDIOBJ(mem_bmp.0));
    let dark_brush = CreateSolidBrush(COLORREF(0x000F0A05));
    let full_rc = RECT { left: 0, top: 40, right: width, bottom: height };
    FillRect(mem_dc, &full_rc, dark_brush);
    let _ = DeleteObject(HGDIOBJ(dark_brush.0));

    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 140,
        AlphaFormat: 0,
    };
    let _ = AlphaBlend(hdc, 0, 40, width, height - 40, mem_dc, 0, 40, width, height - 40, blend);
    let _ = SelectObject(mem_dc, old_bmp);
    let _ = DeleteObject(HGDIOBJ(mem_bmp.0));
    let _ = DeleteDC(mem_dc);

    // 2. Centered Modal Card
    let card_w = 460;
    let card_h = 220;
    let card_x = (width - card_w) / 2;
    let card_y = (height - card_h) / 2;

    draw_rounded_box(hdc, card_x, card_y, card_w, card_h, PK_CARD, PK_GREEN, 8);

    // 3. Header & Icon
    crate::ui::draw_refresh(hdc, card_x + 24, card_y + 24, 22, PK_GREEN_DARK, 2);

    let (title, desc, confirm_btn_label, confirm_bg) = match kind {
        ConfirmModalKind::Restart => (
            "Confirm Remote Restart",
            "The remote host will restart immediately.\n\nDesky will automatically register itself to launch upon startup so you can reconnect once Windows boots back up. Proceed?",
            "Yes, Restart",
            if hover_state == 2996 { PK_GREEN_DARK } else { PK_GREEN },
        ),
        ConfirmModalKind::SafeModeRestart => (
            "Confirm Safe Mode Restart",
            "The remote host will configure BCD to boot into Safe Mode with Networking.\n\nDesky will auto-launch upon boot so you can reconnect in diagnostic mode. Proceed?",
            "Yes, Safe Mode",
            if hover_state == 2996 { COLORREF(0x000F7AEE) } else { COLORREF(0x001290FF) },
        ),
    };

    let font_title = create_segoe_font(16, FW_BOLD);
    let old_f = SelectObject(hdc, HGDIOBJ(font_title.0));
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, PK_TEXT);
    let mut t_title: Vec<u16> = title.encode_utf16().collect();
    let mut rc_title = RECT { left: card_x + 56, top: card_y + 22, right: card_x + card_w - 24, bottom: card_y + 48 };
    let _ = DrawTextW(hdc, &mut t_title, &mut rc_title, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

    let font_body = create_segoe_font(12, FW_NORMAL);
    let _ = SelectObject(hdc, HGDIOBJ(font_body.0));
    SetTextColor(hdc, PK_MUTED);
    let mut t_desc: Vec<u16> = desc.encode_utf16().collect();
    let mut rc_desc = RECT { left: card_x + 24, top: card_y + 58, right: card_x + card_w - 24, bottom: card_y + 150 };
    let _ = DrawTextW(hdc, &mut t_desc, &mut rc_desc, DT_LEFT | DT_WORDBREAK);

    // 4. Action Buttons
    let btn_y = card_y + 160;
    let btn_h = 36;
    let confirm_w = 140;
    let confirm_x = card_x + card_w - 24 - confirm_w;
    let cancel_w = 110;
    let cancel_x = confirm_x - 12 - cancel_w;

    // Cancel Button
    let cancel_bg = if hover_state == 2995 { PK_BTN_HOVER } else { PK_CARD };
    draw_rounded_box(hdc, cancel_x, btn_y, cancel_w, btn_h, cancel_bg, PK_CARD_BORDER, 6);
    let font_btn = create_segoe_font(13, FW_SEMIBOLD);
    let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
    SetTextColor(hdc, PK_TEXT);
    let mut t_canc: Vec<u16> = "Cancel".encode_utf16().collect();
    let mut rc_canc = RECT { left: cancel_x, top: btn_y, right: cancel_x + cancel_w, bottom: btn_y + btn_h };
    let _ = DrawTextW(hdc, &mut t_canc, &mut rc_canc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    // Confirm Button
    draw_rounded_box(hdc, confirm_x, btn_y, confirm_w, btn_h, confirm_bg, confirm_bg, 6);
    SetTextColor(hdc, COLORREF(0x00FFFFFF));
    let mut t_conf: Vec<u16> = confirm_btn_label.encode_utf16().collect();
    let mut rc_conf = RECT { left: confirm_x, top: btn_y, right: confirm_x + confirm_w, bottom: btn_y + btn_h };
    let _ = DrawTextW(hdc, &mut t_conf, &mut rc_conf, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

    let _ = SelectObject(hdc, old_f);
    let _ = DeleteObject(HGDIOBJ(font_title.0));
    let _ = DeleteObject(HGDIOBJ(font_body.0));
    let _ = DeleteObject(HGDIOBJ(font_btn.0));
}

unsafe fn draw_status_toast(hdc: HDC, width: i32, height: i32) {
    let msg_opt = {
        let state = get_admin_state().lock().unwrap();
        state.status_msg.clone()
    };

    if let Some((msg, time)) = msg_opt {
        if time.elapsed().as_secs() < 4 {
            let font = create_segoe_font(12, FW_SEMIBOLD);
            let old_font = SelectObject(hdc, HGDIOBJ(font.0));
            SetBkMode(hdc, TRANSPARENT);

            let toast_w = 340;
            let toast_h = 28;
            let toast_x = (width - toast_w) / 2;
            let toast_y = height - 44;

            draw_rounded_box(hdc, toast_x, toast_y, toast_w, toast_h, PK_GREEN_TINT, PK_GREEN, 14);
            crate::ui::draw_check(hdc, toast_x + 12, toast_y + 6, 16, PK_GREEN_DARK, 2);
            SetTextColor(hdc, PK_GREEN_DARK);
            let mut t_txt: Vec<u16> = msg.encode_utf16().collect();
            let mut t_rc = RECT { left: toast_x + 32, top: toast_y, right: toast_x + toast_w - 12, bottom: toast_y + toast_h };
            let _ = DrawTextW(hdc, &mut t_txt, &mut t_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            let _ = SelectObject(hdc, old_font);
            let _ = DeleteObject(HGDIOBJ(font.0));
        }
    }
}

pub fn handle_admin_click(x: i32, y: i32, width: i32, height: i32) -> bool {
    // 0. Handle confirmation modal if active
    if let Ok(mut state) = get_admin_state().lock() {
        if let Some(modal) = state.confirm_modal {
            let card_w = 460;
            let card_h = 220;
            let card_x = (width - card_w) / 2;
            let card_y = (height - card_h) / 2;

            let btn_y = card_y + 160;
            let btn_h = 36;
            let confirm_w = 140;
            let confirm_x = card_x + card_w - 24 - confirm_w;
            let cancel_w = 110;
            let cancel_x = confirm_x - 12 - cancel_w;

            if y >= btn_y && y < btn_y + btn_h {
                if x >= confirm_x && x < confirm_x + confirm_w {
                    let safe_mode = matches!(modal, ConfirmModalKind::SafeModeRestart);
                    crate::p2p::send_admin_request(AdminReq::RebootHost { safe_mode });
                    state.status_msg = Some((
                        if safe_mode { "Initiating remote Safe Mode reboot...".into() } else { "Initiating remote reboot...".into() },
                        Instant::now(),
                    ));
                    state.confirm_modal = None;
                    return true;
                } else if x >= cancel_x && x < cancel_x + cancel_w {
                    state.confirm_modal = None;
                    return true;
                }
            } else if x < card_x || x >= card_x + card_w || y < card_y || y >= card_y + card_h {
                state.confirm_modal = None;
                return true;
            }

            return true;
        }
    }

    let bar_y = 48;
    if y >= bar_y && y < bar_y + 36 {
        let mut cur_x = 32;
        let tabs = [AdminSubTab::Processes, AdminSubTab::Services, AdminSubTab::Terminal, AdminSubTab::SystemInfo];
        for tab in tabs {
            let tab_w = 120;
            if x >= cur_x && x < cur_x + tab_w {
                if let Ok(mut state) = get_admin_state().lock() {
                    state.sub_tab = tab;
                    if tab == AdminSubTab::Processes && state.processes.is_empty() {
                        crate::p2p::send_admin_request(AdminReq::GetProcesses);
                    } else if tab == AdminSubTab::Services && state.services.is_empty() {
                        crate::p2p::send_admin_request(AdminReq::GetServices);
                    } else if tab == AdminSubTab::SystemInfo && state.system_stats.is_none() {
                        crate::p2p::send_admin_request(AdminReq::GetSystemInfo);
                    }
                }
                return true;
            }
            cur_x += tab_w + 8;
        }

        // Refresh button
        let ref_w = 92;
        let ref_x = width - 24 - ref_w - 6;
        if x >= ref_x && x < ref_x + ref_w {
            if let Ok(mut state) = get_admin_state().lock() {
                state.is_loading = true;
                state.last_req_time = Some(Instant::now());
                match state.sub_tab {
                    AdminSubTab::Processes => crate::p2p::send_admin_request(AdminReq::GetProcesses),
                    AdminSubTab::Services => crate::p2p::send_admin_request(AdminReq::GetServices),
                    AdminSubTab::Terminal => state.terminal_lines.push(">> Console ready.".into()),
                    AdminSubTab::SystemInfo => crate::p2p::send_admin_request(AdminReq::GetSystemInfo),
                }
            }
            return true;
        }

        let reboot_w = 92;
        let reboot_x = ref_x - reboot_w - 8;
        if x >= reboot_x && x < reboot_x + reboot_w {
            if let Ok(mut state) = get_admin_state().lock() {
                state.confirm_modal = Some(ConfirmModalKind::Restart);
            }
            return true;
        }

        let safe_w = 98;
        let safe_x = reboot_x - safe_w - 8;
        if x >= safe_x && x < safe_x + safe_w {
            if let Ok(mut state) = get_admin_state().lock() {
                state.confirm_modal = Some(ConfirmModalKind::SafeModeRestart);
            }
            return true;
        }
    }

    let sub_tab = {
        let state = get_admin_state().lock().unwrap();
        state.sub_tab
    };

    if sub_tab == AdminSubTab::SystemInfo {
        let card_h = 120;
        let net_cy = 96 + (card_h + 16) * 2;
        let net_w = width - 48;
        let wol_btn_w = 210;
        let wol_btn_x = 24 + net_w - wol_btn_w - 18;
        if y >= net_cy + 18 && y < net_cy + 18 + 40 && x >= wol_btn_x && x < wol_btn_x + wol_btn_w {
            let mac = {
                let state = get_admin_state().lock().unwrap();
                state.system_stats.as_ref().map(|s| s.mac_addr.clone()).unwrap_or_default()
            };
            if !mac.is_empty() {
                crate::p2p::send_admin_request(AdminReq::WakeOnLan { mac: mac.clone() });
                if let Ok(mut state) = get_admin_state().lock() {
                    state.status_msg = Some((format!("Broadcasting Wake-on-LAN to {}...", mac), Instant::now()));
                }
            } else {
                let _ = crate::p2p::send_wake_on_lan([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
                if let Ok(mut state) = get_admin_state().lock() {
                    state.status_msg = Some(("Broadcasting LAN Wake-on-LAN Magic Packet...".into(), Instant::now()));
                }
            }
            return true;
        }
    }

    if sub_tab == AdminSubTab::Services {
        let top_y = 96;
        let search_h = 32;
        if x >= 24 && x < width - 24 && y >= top_y && y < top_y + search_h {
            if let Ok(mut state) = get_admin_state().lock() {
                state.service_filter_active = true;
                state.filter_active = false;
            }
            return true;
        } else {
            if let Ok(mut state) = get_admin_state().lock() {
                state.service_filter_active = false;
            }
        }

        let table_w = width - 48;
        let head_y = top_y + search_h + 8;
        let rows_y = head_y + 30;
        let row_h = 30;
        if y >= rows_y && y < height - 20 {
            let (filter_text, services, scroll_y) = {
                let state = get_admin_state().lock().unwrap();
                (state.service_filter.clone(), state.services.clone(), state.service_scroll_y)
            };
            let lower_filter = filter_text.to_lowercase();
            let filtered: Vec<&ServiceItem> = services
                .iter()
                .filter(|s| {
                    if lower_filter.is_empty() {
                        true
                    } else {
                        s.name.to_lowercase().contains(&lower_filter)
                            || s.display_name.to_lowercase().contains(&lower_filter)
                            || s.status.to_lowercase().contains(&lower_filter)
                    }
                })
                .collect();

            let clicked_idx = ((y - rows_y + scroll_y) / row_h) as usize;
            if clicked_idx < filtered.len() {
                let s = filtered[clicked_idx];
                let act_x = table_w - 194;
                let btn1_w = 56;
                let btn2_w = 64;

                // Button 1: Start / Stop
                if x >= act_x && x < act_x + btn1_w {
                    let is_running = s.status.eq_ignore_ascii_case("running");
                    let action = if is_running { "stop" } else { "start" };
                    crate::p2p::send_admin_request(AdminReq::ControlService {
                        name: s.name.clone(),
                        action: action.into(),
                    });
                    if let Ok(mut state) = get_admin_state().lock() {
                        state.status_msg = Some((format!("Sending {} command to '{}'...", action, s.name), Instant::now()));
                    }
                    return true;
                }
                // Button 2: Restart
                let b2_x = act_x + btn1_w + 6;
                if x >= b2_x && x < b2_x + btn2_w {
                    crate::p2p::send_admin_request(AdminReq::ControlService {
                        name: s.name.clone(),
                        action: "restart".into(),
                    });
                    if let Ok(mut state) = get_admin_state().lock() {
                        state.status_msg = Some((format!("Restarting service '{}'...", s.name), Instant::now()));
                    }
                    return true;
                }
            }
        }
    }

    if sub_tab == AdminSubTab::Processes {
        let top_y = 96;
        let search_h = 32;
        if x >= 24 && x < width - 24 && y >= top_y && y < top_y + search_h {
            if let Ok(mut state) = get_admin_state().lock() {
                state.filter_active = true;
            }
            return true;
        } else {
            if let Ok(mut state) = get_admin_state().lock() {
                state.filter_active = false;
            }
        }

        let table_w = width - 48;
        let head_y = top_y + search_h + 8;
        let rows_y = head_y + 30;
        if y >= rows_y && y < height - 20 {
            let (filter_text, processes, scroll_y) = {
                let state = get_admin_state().lock().unwrap();
                (state.process_filter.clone(), state.processes.clone(), state.scroll_y)
            };
            let lower_filter = filter_text.to_lowercase();
            let filtered: Vec<&ProcessItem> = processes
                .iter()
                .filter(|p| {
                    if lower_filter.is_empty() {
                        true
                    } else {
                        p.name.to_lowercase().contains(&lower_filter)
                            || p.pid.to_string().contains(&lower_filter)
                    }
                })
                .collect();

            let row_h = 28;
            let clicked_idx = ((y - rows_y + scroll_y) / row_h) as usize;
            if clicked_idx < filtered.len() {
                let btn_x = table_w - 114;
                let btn_w = 90;
                if x >= btn_x && x < btn_x + btn_w {
                    let pid = filtered[clicked_idx].pid;
                    crate::p2p::send_admin_request(AdminReq::KillProcess { pid });
                    if let Ok(mut state) = get_admin_state().lock() {
                        state.status_msg = Some((format!("Terminating process PID {}...", pid), Instant::now()));
                    }
                    return true;
                }
            }
        }
    }

    false
}

pub fn handle_admin_char(c: char) -> bool {
    let sub_tab = {
        let state = get_admin_state().lock().unwrap();
        state.sub_tab
    };

    match sub_tab {
        AdminSubTab::Processes => {
            if let Ok(mut state) = get_admin_state().lock() {
                if state.filter_active {
                    if c == '\x08' {
                        state.process_filter.pop();
                    } else if !c.is_control() {
                        state.process_filter.push(c);
                    }
                    state.scroll_y = 0;
                    return true;
                }
            }
        }
        AdminSubTab::Services => {
            if let Ok(mut state) = get_admin_state().lock() {
                if state.service_filter_active {
                    if c == '\x08' {
                        state.service_filter.pop();
                    } else if !c.is_control() {
                        state.service_filter.push(c);
                    }
                    state.service_scroll_y = 0;
                    return true;
                }
            }
        }
        AdminSubTab::Terminal => {
            if let Ok(mut state) = get_admin_state().lock() {
                if c == '\r' || c == '\n' {
                    let cmd = state.terminal_input.trim().to_string();
                    if !cmd.is_empty() {
                        state.terminal_lines.push(format!("PS > {}", cmd));
                        state.terminal_input.clear();
                        crate::p2p::send_admin_request(AdminReq::ExecuteCommand { command: cmd });
                    }
                } else if c == '\x08' {
                    state.terminal_input.pop();
                } else if !c.is_control() {
                    state.terminal_input.push(c);
                }
                return true;
            }
        }
        _ => {}
    }

    false
}

pub fn handle_admin_key(key: u32) -> bool {
    if let Ok(mut state) = get_admin_state().lock() {
        if let Some(modal) = state.confirm_modal {
            if key == 0x1B {
                state.confirm_modal = None;
                return true;
            } else if key == 0x0D {
                let safe_mode = matches!(modal, ConfirmModalKind::SafeModeRestart);
                crate::p2p::send_admin_request(AdminReq::RebootHost { safe_mode });
                state.status_msg = Some((
                    if safe_mode { "Initiating remote Safe Mode reboot...".into() } else { "Initiating remote reboot...".into() },
                    Instant::now(),
                ));
                state.confirm_modal = None;
                return true;
            }
            return true;
        }
    }

    if key == 0x0D {
        return handle_admin_char('\r');
    } else if key == 0x08 {
        return handle_admin_char('\x08');
    }
    false
}

pub fn handle_admin_scroll(delta: i32) {
    if let Ok(mut state) = get_admin_state().lock() {
        match state.sub_tab {
            AdminSubTab::Processes => {
                let lines_to_scroll = (delta / 120) * 36;
                state.scroll_y = (state.scroll_y - lines_to_scroll).max(0);
            }
            AdminSubTab::Services => {
                let lines_to_scroll = (delta / 120) * 36;
                state.service_scroll_y = (state.service_scroll_y - lines_to_scroll).max(0);
            }
            AdminSubTab::Terminal => {
                let lines_to_scroll = (delta / 120) * 3;
                state.terminal_scroll_y = (state.terminal_scroll_y + lines_to_scroll).max(0);
            }
            _ => {}
        }
    }
}

pub fn get_admin_hover_state(x: i32, y: i32, width: i32, height: i32) -> i32 {
    if let Ok(state) = get_admin_state().lock() {
        if state.confirm_modal.is_some() {
            let card_w = 460;
            let card_h = 220;
            let card_x = (width - card_w) / 2;
            let card_y = (height - card_h) / 2;
            let btn_y = card_y + 160;
            let btn_h = 36;
            let confirm_w = 140;
            let confirm_x = card_x + card_w - 24 - confirm_w;
            let cancel_w = 110;
            let cancel_x = confirm_x - 12 - cancel_w;

            if y >= btn_y && y < btn_y + btn_h {
                if x >= confirm_x && x < confirm_x + confirm_w {
                    return 2996;
                } else if x >= cancel_x && x < cancel_x + cancel_w {
                    return 2995;
                }
            }
            return 2994;
        }
    }

    let bar_y = 48;
    if y >= bar_y && y < bar_y + 36 {
        let mut cur_x = 32;
        for i in 0..4 {
            let tab_w = 120;
            if x >= cur_x && x < cur_x + tab_w {
                return 2000 + i;
            }
            cur_x += tab_w + 8;
        }
        let ref_w = 92;
        let ref_x = width - 24 - ref_w - 6;
        if x >= ref_x && x < ref_x + ref_w {
            return 2999;
        }
        let reboot_w = 92;
        let reboot_x = ref_x - reboot_w - 8;
        if x >= reboot_x && x < reboot_x + reboot_w {
            return 2998;
        }
        let safe_w = 98;
        let safe_x = reboot_x - safe_w - 8;
        if x >= safe_x && x < safe_x + safe_w {
            return 2997;
        }
    }

    let sub_tab = {
        let state = get_admin_state().lock().unwrap();
        state.sub_tab
    };

    if sub_tab == AdminSubTab::Services {
        let top_y = 96;
        let table_w = width - 48;
        let search_h = 32;
        let head_y = top_y + search_h + 8;
        let rows_y = head_y + 30;
        let row_h = 30;
        if y >= rows_y && y < height - 20 {
            let scroll_y = {
                let state = get_admin_state().lock().unwrap();
                state.service_scroll_y
            };
            let i = ((y - rows_y + scroll_y) / row_h) as i32;
            let act_x = table_w - 194;
            let btn1_w = 56;
            let btn2_w = 64;
            if x >= act_x && x < act_x + btn1_w {
                return 6000 + i;
            } else if x >= act_x + btn1_w + 6 && x < act_x + btn1_w + 6 + btn2_w {
                return 7000 + i;
            }
            return 5000 + i;
        }
    }

    if sub_tab == AdminSubTab::Processes {
        let top_y = 96;
        let search_h = 32;
        let head_y = top_y + search_h + 8;
        let rows_y = head_y + 30;
        if y >= rows_y && y < height - 20 {
            let (filter_text, processes, scroll_y) = {
                let state = get_admin_state().lock().unwrap();
                (state.process_filter.clone(), state.processes.clone(), state.scroll_y)
            };
            let lower_filter = filter_text.to_lowercase();
            let filtered_count = processes
                .iter()
                .filter(|p| {
                    lower_filter.is_empty()
                        || p.name.to_lowercase().contains(&lower_filter)
                        || p.pid.to_string().contains(&lower_filter)
                })
                .count();

            let row_h = 28;
            let row_idx = ((y - rows_y + scroll_y) / row_h) as usize;
            if row_idx < filtered_count {
                let table_w = width - 48;
                let btn_x = table_w - 114;
                let btn_w = 90;
                if x >= btn_x && x < btn_x + btn_w {
                    return 4000 + row_idx as i32;
                }
                return 3000 + row_idx as i32;
            }
        }
    }

    if sub_tab == AdminSubTab::SystemInfo {
        let card_h = 120;
        let net_cy = 96 + (card_h + 16) * 2;
        let net_w = width - 48;
        let wol_btn_w = 210;
        let wol_btn_x = 24 + net_w - wol_btn_w - 18;
        if y >= net_cy + 18 && y < net_cy + 18 + 40 && x >= wol_btn_x && x < wol_btn_x + wol_btn_w {
            return 2990;
        }
    }

    -1
}
