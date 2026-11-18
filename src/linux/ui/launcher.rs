//! Main X11 Launcher Window, Responsive Homepage & Application Lifecycle
//!
//! Mirrors `src/windows/ui/launcher.rs` 1:1 by exposing responsive Home page rendering,
//! 4-card layout calculations, LAN radar scanning, incoming approval modals, and lifecycle
//! using anti-aliased TrueType fonts (`PixelCanvas`).

use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tracing::info;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

use super::remote_desk_edit::{
    clear_remote_id_raw, format_desky_id_display, get_remote_id_raw, set_remote_id_raw,
    INPUT_SELECT_ALL,
};
use super::theme::{
    copy_to_clipboard, PixelCanvas, PK_BG, PK_CARD, PK_CARD_BORDER, PK_CONSOLE_BG, PK_GREEN,
    PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_RED, PK_TEXT,
};
use super::viewport::{run_viewport_x11_window, spawn_or_add_tab_viewport};

pub static LOCAL_DESKY_ID: Mutex<String> = Mutex::new(String::new());
pub static RUNNING: AtomicBool = AtomicBool::new(true);
pub static NEEDS_REDRAW: AtomicBool = AtomicBool::new(true);
pub static BRING_TO_FRONT: AtomicBool = AtomicBool::new(false);

static POPUP_ACTIVE: AtomicBool = AtomicBool::new(false);
static STATUS_LOGS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
pub static BOTTOM_CARD3_TAB: AtomicU8 = AtomicU8::new(0); // 0 = Recents, 1 = Contacts, 2 = LAN Radar
static LAN_SCAN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

fn status_logs() -> &'static Mutex<Vec<String>> {
    STATUS_LOGS.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn set_assigned_desky_id(id: &str) {
    if let Ok(mut guard) = LOCAL_DESKY_ID.lock() {
        *guard = id.to_string();
    }
    info!("✅ [LINUX HOST] Permanent Desky ID Approved: {}", id);
    append_status_log(&format!("Online | ID: {}", id));
    println!("\n\x1b[1;32m==================================================\x1b[0m");
    println!("\x1b[1;32m  DESKY LINUX ONLINE | pkasoft.com\x1b[0m");
    println!("\x1b[1;37m  YOUR PERMANENT DESKY ID: \x1b[1;32m{}\x1b[0m", id);
    println!("\x1b[1;37m  STATUS: \x1b[1;36m[Online] Ready for incoming connections\x1b[0m");
    println!("\x1b[1;32m==================================================\x1b[0m\n");
    request_main_redraw();
}

pub fn append_status_log(msg: &str) {
    info!("[STATUS] {}", msg);
    if let Ok(mut logs) = status_logs().lock() {
        logs.push(msg.to_string());
        if logs.len() > 14 {
            logs.remove(0);
        }
    }
    request_main_redraw();
}

pub fn bring_main_window_to_front() {
    BRING_TO_FRONT.store(true, Ordering::Relaxed);
    NEEDS_REDRAW.store(true, Ordering::Relaxed);
}

pub fn request_main_redraw() {
    NEEDS_REDRAW.store(true, Ordering::Relaxed);
}

pub fn prompt_incoming_request(client_id: &str) {
    append_status_log(&format!(
        "Incoming request from ID: {} - Click Accept or Decline!",
        client_id
    ));
    bring_main_window_to_front();
    spawn_linux_approval_popup(client_id.to_string());
}

pub fn is_lan_scan_active() -> bool {
    LAN_SCAN_IN_PROGRESS.load(Ordering::Relaxed)
}

pub fn trigger_lan_radar_scan() {
    if LAN_SCAN_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return;
    }
    append_status_log("LAN Radar: Broadcast scan initiated...");
    crate::p2p::trigger_lan_ping();
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(1600));
        LAN_SCAN_IN_PROGRESS.store(false, Ordering::SeqCst);
        let peers = crate::p2p::get_lan_peers();
        if peers.is_empty() {
            append_status_log("LAN Radar: Scan complete - No devices found on local subnet.");
        } else {
            let names: Vec<String> = peers.iter().take(3).map(|p| p.hostname.clone()).collect();
            append_status_log(&format!(
                "LAN Radar: Found {} device(s) ({})",
                peers.len(),
                names.join(", ")
            ));
        }
        request_main_redraw();
    });
}

#[derive(Clone, Copy, Debug)]
pub struct ResponsiveLayout {
    pub c1_x: i32,
    pub c2_x: i32,
    pub card_w: i32,
    pub card_h: i32,
    pub card_y: i32,

    pub id_box_x: i32,
    pub id_box_y: i32,
    pub id_box_w: i32,
    pub id_box_h: i32,

    pub copy_btn_x: i32,
    pub copy_btn_y: i32,
    pub copy_btn_w: i32,
    pub copy_btn_h: i32,

    pub input_box_x: i32,
    pub input_box_y: i32,
    pub input_box_w: i32,
    pub input_box_h: i32,

    pub conn_btn_x: i32,
    pub conn_btn_y: i32,
    pub conn_btn_w: i32,
    pub conn_btn_h: i32,

    pub col1_x: i32,
    pub col2_x: i32,
    pub col_w: i32,
    pub col_h: i32,
    pub col_y: i32,

    pub font_scale: f32,
}

/// Computes responsive layout geometry identical to Windows `get_responsive_layout`
pub fn get_responsive_layout(w: i32, h: i32) -> ResponsiveLayout {
    let top_offset = 40;
    let margin = 24;
    let gap = 18;
    let card_w = (w - (2 * margin) - gap).max(220) / 2;

    let scale_w = w as f32 / 1024.0;
    let scale_h = h as f32 / 680.0;
    let font_scale = (scale_w.min(scale_h)).clamp(1.0, 1.35);

    let card_h = ((188.0 * font_scale) as i32)
        .min((h - top_offset - 200) / 2)
        .max(178);
    let card_y = top_offset + 14;

    let c1_x = margin;
    let c2_x = margin + card_w + gap;

    // Card 1: This Desk elements
    let id_box_x = c1_x + 18;
    let id_box_y = card_y + (46.0 * font_scale) as i32;
    let id_box_w = card_w - 36;
    let id_box_h = (64.0 * font_scale) as i32;

    let copy_btn_x = c1_x + 18;
    let copy_btn_y = id_box_y + id_box_h + (14.0 * font_scale) as i32;
    let copy_btn_w = card_w - 36;
    let copy_btn_h = (42.0 * font_scale) as i32;

    // Card 2: Remote Desk elements
    let input_box_x = c2_x + 18;
    let input_box_y = card_y + (46.0 * font_scale) as i32;
    let input_box_w = card_w - 36;
    let input_box_h = (64.0 * font_scale) as i32;

    let conn_btn_x = c2_x + 18;
    let conn_btn_y = copy_btn_y;
    let conn_btn_w = card_w - 36;
    let conn_btn_h = copy_btn_h;

    // Bottom Section: Split into 2 columns (Recent Sessions & Security/Server Logs)
    let col_y = card_y + card_h + 16;
    let col_w = card_w;
    let col_h = (h - col_y - margin).max(140);
    let col1_x = c1_x;
    let col2_x = c2_x;

    ResponsiveLayout {
        c1_x,
        c2_x,
        card_w,
        card_h,
        card_y,
        id_box_x,
        id_box_y,
        id_box_w,
        id_box_h,
        copy_btn_x,
        copy_btn_y,
        copy_btn_w,
        copy_btn_h,
        input_box_x,
        input_box_y,
        input_box_w,
        input_box_h,
        conn_btn_x,
        conn_btn_y,
        conn_btn_w,
        conn_btn_h,
        col1_x,
        col2_x,
        col_w,
        col_h,
        col_y,
        font_scale,
    }
}

pub fn get_card3_tab_rects(
    col1_x: i32,
    col_y: i32,
    font_scale: f32,
    contacts_count: usize,
    peers_count: usize,
) -> (
    (i32, i32, i32, i32, String),
    (i32, i32, i32, i32, String),
    (i32, i32, i32, i32, String),
) {
    let tab_h = (25.0 * font_scale) as i32;
    let tab_y = col_y + 14;

    let t0_text = "RECENTS".to_string();
    let t0_w = (104.0 * font_scale) as i32;
    let t0_x = col1_x + 18;

    let t1_text = if contacts_count > 0 {
        format!("CONTACTS ({})", contacts_count)
    } else {
        "CONTACTS".to_string()
    };
    let t1_w = if contacts_count > 0 {
        (134.0 * font_scale) as i32
    } else {
        (112.0 * font_scale) as i32
    };
    let t1_x = t0_x + t0_w + 6;

    let t2_text = if peers_count > 0 {
        format!("LAN RADAR ({})", peers_count)
    } else {
        "LAN RADAR".to_string()
    };
    let t2_w = if peers_count > 0 {
        (134.0 * font_scale) as i32
    } else {
        (114.0 * font_scale) as i32
    };
    let t2_x = t1_x + t1_w + 6;

    (
        (t0_x, tab_y, t0_w, tab_h, t0_text),
        (t1_x, tab_y, t1_w, tab_h, t1_text),
        (t2_x, tab_y, t2_w, tab_h, t2_text),
    )
}

/// Validates entered remote Desky ID and initiates connection in the unified window
pub fn handle_homepage_connect(raw_target: &str) {
    let digits: String = raw_target.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        append_status_log("[!] Please enter a 9-digit remote Desky ID (e.g. 800 258 756)");
        return;
    }

    let local_digits: String = {
        let g = LOCAL_DESKY_ID.lock().unwrap();
        let s = if !g.is_empty() {
            g.clone()
        } else {
            crate::identity::load_or_create_identity()
                .desky_id
                .unwrap_or_default()
        };
        s.chars().filter(|c| c.is_ascii_digit()).collect()
    };

    if !local_digits.is_empty() && digits == local_digits {
        append_status_log("[!] That is THIS computer's own ID! Enter a remote Desky ID.");
        return;
    }

    let target_display = if digits.len() == 9 {
        format!("{} {} {}", &digits[0..3], &digits[3..6], &digits[6..9])
    } else {
        digits.clone()
    };

    info!("Initiating P2P session to remote Desky ID: {}", target_display);
    crate::identity::save_recent_session(&target_display);
    append_status_log(&format!(
        "Requesting session with Remote Desky ID: {}...",
        target_display
    ));

    spawn_or_add_tab_viewport(&target_display);
}

/// Renders the complete pkasoft.com White Theme Homepage onto the `PixelCanvas`
pub fn draw_homepage_view(canvas: &mut PixelCanvas, win_w: u16, win_h: u16) {
    let w = win_w as i32;
    let h = win_h as i32;
    let l = get_responsive_layout(w, h);

    let f_badge = 12.0 * l.font_scale;
    let f_id = 30.0 * l.font_scale;
    let f_btn = 15.0 * l.font_scale;
    let f_console = 12.5 * l.font_scale;

    // 1. Pending Incoming Connection Approval Modal
    if let Some(req) = crate::p2p::get_pending_approval() {
        let mw = (w - 64).clamp(440, 620);
        let mh = 280;
        let mx = (w - mw) / 2;
        let my = ((h - mh) / 2).max(64);

        canvas.draw_rounded_box(mx, my, mw, mh, PK_CARD, PK_GREEN, 12);

        let badge_w = 260;
        let badge_x = mx + (mw - badge_w) / 2;
        canvas.draw_rounded_box(badge_x, my + 22, badge_w, 28, PK_GREEN_TINT, PK_GREEN, 14);
        canvas.draw_text_centered(
            badge_x,
            my + 22,
            badge_w,
            28,
            "INCOMING CONNECTION REQUEST",
            f_badge,
            true,
            PK_GREEN_DARK,
        );

        canvas.draw_text_centered(
            mx + 20,
            my + 66,
            mw - 40,
            48,
            &format!("ID: {}", req.client_id),
            f_id,
            true,
            PK_TEXT,
        );
        canvas.draw_text_centered(
            mx + 24,
            my + 124,
            mw - 48,
            28,
            "This remote client is requesting permission to view and control your desktop.",
            13.5,
            false,
            PK_MUTED,
        );

        let btn_w = (mw - 72) / 2;
        let btn_h = 48;
        let btn_y = my + 188;
        let acc_x = mx + 24;
        let dec_x = mx + 48 + btn_w;

        canvas.draw_rounded_box(acc_x, btn_y, btn_w, btn_h, PK_GREEN, PK_GREEN, 8);
        canvas.draw_text_centered(
            acc_x,
            btn_y,
            btn_w,
            btn_h,
            "✓  Accept Connection",
            f_btn,
            true,
            PK_CARD,
        );

        canvas.draw_rounded_box(dec_x, btn_y, btn_w, btn_h, PK_RED, PK_RED, 8);
        canvas.draw_text_centered(dec_x, btn_y, btn_w, btn_h, "✕  Decline", f_btn, true, PK_CARD);
        return;
    }

    // 2. Card 1: THIS DESK (Top Left)
    canvas.draw_rounded_box(l.c1_x, l.card_y, l.card_w, l.card_h, PK_CARD, PK_GREEN, 12);

    let tag1_w = (94.0 * l.font_scale) as i32;
    let tag1_h = (22.0 * l.font_scale) as i32;
    canvas.draw_rounded_box(
        l.c1_x + 18,
        l.card_y + 14,
        tag1_w,
        tag1_h,
        PK_GREEN_TINT,
        PK_GREEN,
        6,
    );
    canvas.draw_text_centered(
        l.c1_x + 18,
        l.card_y + 14,
        tag1_w,
        tag1_h,
        "THIS DESK",
        f_badge,
        true,
        PK_GREEN_DARK,
    );

    canvas.draw_rounded_box(
        l.id_box_x,
        l.id_box_y,
        l.id_box_w,
        l.id_box_h,
        PK_INPUT_BG,
        PK_CARD_BORDER,
        8,
    );
    canvas.draw_text(
        l.id_box_x + 14,
        l.id_box_y + 7,
        "YOUR ID",
        11.0 * l.font_scale,
        true,
        PK_MUTED,
        Some(l.id_box_w - 28),
    );

    let current_id = {
        let guard = LOCAL_DESKY_ID.lock().unwrap();
        if !guard.is_empty() {
            guard.clone()
        } else {
            crate::identity::load_or_create_identity()
                .desky_id
                .unwrap_or_else(|| "Connecting...".into())
        }
    };
    canvas.draw_text_vcenter(
        l.id_box_x + 14,
        l.id_box_y + 20,
        l.id_box_w - 28,
        l.id_box_h - 22,
        &current_id,
        f_id,
        true,
        PK_GREEN,
    );

    canvas.draw_rounded_box(
        l.copy_btn_x,
        l.copy_btn_y,
        l.copy_btn_w,
        l.copy_btn_h,
        PK_CARD,
        PK_CARD_BORDER,
        8,
    );
    canvas.draw_text_centered(
        l.copy_btn_x,
        l.copy_btn_y,
        l.copy_btn_w,
        l.copy_btn_h,
        "Copy ID",
        f_btn,
        true,
        PK_TEXT,
    );

    // 3. Card 2: REMOTE DESK (Top Right)
    canvas.draw_rounded_box(
        l.c2_x,
        l.card_y,
        l.card_w,
        l.card_h,
        PK_CARD,
        PK_CARD_BORDER,
        12,
    );

    let tag2_w = (112.0 * l.font_scale) as i32;
    let tag2_h = (22.0 * l.font_scale) as i32;
    canvas.draw_rounded_box(
        l.c2_x + 18,
        l.card_y + 14,
        tag2_w,
        tag2_h,
        PK_INPUT_BG,
        PK_CARD_BORDER,
        6,
    );
    canvas.draw_text_centered(
        l.c2_x + 18,
        l.card_y + 14,
        tag2_w,
        tag2_h,
        "REMOTE DESK",
        f_badge,
        true,
        PK_MUTED,
    );

    canvas.draw_rounded_box(
        l.input_box_x,
        l.input_box_y,
        l.input_box_w,
        l.input_box_h,
        PK_INPUT_BG,
        PK_CARD_BORDER,
        8,
    );
    canvas.draw_text(
        l.input_box_x + 14,
        l.input_box_y + 7,
        "ENTER 9-DIGIT REMOTE DESKY ID",
        11.0 * l.font_scale,
        true,
        PK_MUTED,
        Some(l.input_box_w - 28),
    );

    let typed_raw = get_remote_id_raw();
    if typed_raw.is_empty() {
        canvas.draw_text_vcenter(
            l.input_box_x + 14,
            l.input_box_y + 20,
            l.input_box_w - 28,
            l.input_box_h - 22,
            "e.g. 800 258 756",
            22.0 * l.font_scale,
            false,
            PK_MUTED,
        );
    } else {
        let formatted = format!("{} |", format_desky_id_display(&typed_raw));
        canvas.draw_text_vcenter(
            l.input_box_x + 14,
            l.input_box_y + 20,
            l.input_box_w - 64,
            l.input_box_h - 22,
            &formatted,
            26.0 * l.font_scale,
            true,
            PK_TEXT,
        );
        // Clear [×] button on right side of input box
        let clr_sz = 28;
        let clr_x = l.input_box_x + l.input_box_w - clr_sz - 10;
        let clr_y = l.input_box_y + (l.input_box_h - clr_sz) / 2;
        canvas.draw_rounded_box(clr_x, clr_y, clr_sz, clr_sz, PK_CARD, PK_CARD_BORDER, 6);
        canvas.draw_icon_x(clr_x + 8, clr_y + 8, 12, PK_MUTED);
    }

    canvas.draw_rounded_box(
        l.conn_btn_x,
        l.conn_btn_y,
        l.conn_btn_w,
        l.conn_btn_h,
        PK_GREEN,
        PK_GREEN,
        8,
    );
    canvas.draw_text_centered(
        l.conn_btn_x,
        l.conn_btn_y,
        l.conn_btn_w,
        l.conn_btn_h,
        "Connect  →",
        f_btn,
        true,
        PK_CARD,
    );

    // 4. Card 3: RECENTS / CONTACTS / LAN RADAR (Bottom Left)
    canvas.draw_rounded_box(
        l.col1_x,
        l.col_y,
        l.col_w,
        l.col_h,
        PK_CARD,
        PK_CARD_BORDER,
        12,
    );

    let c3_tab = BOTTOM_CARD3_TAB.load(Ordering::Relaxed);
    let contacts = crate::identity::load_address_book();
    let lan_peers = crate::p2p::get_lan_peers();

    let (t0, t1, t2) = get_card3_tab_rects(
        l.col1_x,
        l.col_y,
        l.font_scale,
        contacts.len(),
        lan_peers.len(),
    );

    let icon_sz = (13.0 * l.font_scale) as i32;
    let icon_gap = 5;

    // Tab 0: RECENTS (Clock Icon)
    let (t0_x, t0_y, t0_w, t0_h, t0_text) = t0;
    let (bg0, bd0, fg0) = if c3_tab == 0 {
        (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
    } else {
        (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
    };
    canvas.draw_rounded_box(t0_x, t0_y, t0_w, t0_h, bg0, bd0, 6);
    let icon_y0 = t0_y + (t0_h - icon_sz) / 2;
    canvas.draw_icon_clock(t0_x + 9, icon_y0, icon_sz, fg0);
    canvas.draw_text_vcenter(t0_x + 9 + icon_sz + icon_gap, t0_y, t0_w - (9 + icon_sz + icon_gap), t0_h, &t0_text, f_badge, true, fg0);

    // Tab 1: CONTACTS (Users Icon)
    let (t1_x, t1_y, t1_w, t1_h, t1_text) = t1;
    let (bg1, bd1, fg1) = if c3_tab == 1 {
        (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
    } else {
        (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
    };
    canvas.draw_rounded_box(t1_x, t1_y, t1_w, t1_h, bg1, bd1, 6);
    let icon_y1 = t1_y + (t1_h - icon_sz) / 2;
    canvas.draw_icon_users(t1_x + 9, icon_y1, icon_sz, fg1);
    canvas.draw_text_vcenter(t1_x + 9 + icon_sz + icon_gap, t1_y, t1_w - (9 + icon_sz + icon_gap), t1_h, &t1_text, f_badge, true, fg1);

    // Tab 2: LAN RADAR (Radio/Radar Broadcast Icon)
    let (t2_x, t2_y, t2_w, t2_h, t2_text) = t2;
    let (bg2, bd2, fg2) = if c3_tab == 2 {
        (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
    } else {
        (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
    };
    canvas.draw_rounded_box(t2_x, t2_y, t2_w, t2_h, bg2, bd2, 6);
    let icon_y2 = t2_y + (t2_h - icon_sz) / 2;
    canvas.draw_icon_radio(t2_x + 9, icon_y2, icon_sz, fg2);
    canvas.draw_text_vcenter(t2_x + 9 + icon_sz + icon_gap, t2_y, t2_w - (9 + icon_sz + icon_gap), t2_h, &t2_text, f_badge, true, fg2);

    // Right action button in Card 3 header (+ Add or Scan)
    if c3_tab == 2 {
        let scanning = is_lan_scan_active();
        let scan_w = if scanning { 76 } else { 56 };
        let scan_x = l.col1_x + l.col_w - scan_w - 18;
        let (s_bg, s_bd, s_fg) = if scanning {
            (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
        } else {
            (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
        };
        canvas.draw_rounded_box(scan_x, t0_y, scan_w, t0_h, s_bg, s_bd, 6);
        canvas.draw_text_centered(
            scan_x,
            t0_y,
            scan_w,
            t0_h,
            if scanning { "Scanning..." } else { "Scan" },
            f_badge,
            true,
            s_fg,
        );
    } else if c3_tab == 1 {
        let add_w = 56;
        let add_x = l.col1_x + l.col_w - add_w - 18;
        canvas.draw_rounded_box(add_x, t0_y, add_w, t0_h, PK_GREEN_TINT, PK_GREEN, 6);
        canvas.draw_text_centered(add_x, t0_y, add_w, t0_h, "+ Add", f_badge, true, PK_GREEN_DARK);
    }

    let item_x = l.col1_x + 18;
    let item_w = l.col_w - 36;

    if c3_tab == 2 {
        if lan_peers.is_empty() {
            let empty_title = if is_lan_scan_active() {
                "Scanning local subnet for Desky devices..."
            } else {
                "No Desky devices found on local network"
            };
            canvas.draw_text(
                item_x,
                l.col_y + 52,
                empty_title,
                14.5 * l.font_scale,
                true,
                PK_TEXT,
                Some(item_w),
            );
            canvas.draw_text_wrapped(
                item_x,
                l.col_y + 78,
                item_w,
                50,
                "Desky instances on your local Wi-Fi or subnet appear here with 1-click zero-cloud connect.",
                f_console,
                false,
                PK_MUTED,
            );
        } else {
            let item_h = (48.0 * l.font_scale) as i32;
            for (idx, peer) in lan_peers.iter().take(3).enumerate() {
                let iy = l.col_y + 48 + (idx as i32 * (item_h + 8));
                if iy + item_h > l.col_y + l.col_h - 10 {
                    break;
                }
                canvas.draw_rounded_box(item_x, iy, item_w, item_h, PK_INPUT_BG, PK_CARD_BORDER, 8);

                let pill_w = 74;
                let pill_h = 28;
                let pill_x = item_x + item_w - pill_w - 8;
                let pill_y = iy + (item_h - pill_h) / 2;

                // Two-line peer details clipped strictly to the left of the Connect button
                let text_avail_w = (pill_x - item_x - 18).max(40);
                let title_text = if !peer.user.is_empty() {
                    format!("{} ({})", peer.hostname, peer.user)
                } else {
                    peer.hostname.clone()
                };
                canvas.draw_text(
                    item_x + 12,
                    iy + 6,
                    &title_text,
                    14.0 * l.font_scale,
                    true,
                    PK_TEXT,
                    Some(text_avail_w),
                );

                let sub_text = format!(
                    "ID: {}  •  {}",
                    peer.id,
                    peer.ip.map(|i| i.to_string()).unwrap_or_default()
                );
                canvas.draw_text(
                    item_x + 12,
                    iy + 25,
                    &sub_text,
                    12.0 * l.font_scale,
                    false,
                    PK_MUTED,
                    Some(text_avail_w),
                );

                canvas.draw_rounded_box(pill_x, pill_y, pill_w, pill_h, PK_GREEN_TINT, PK_GREEN, 6);
                canvas.draw_text_centered(
                    pill_x,
                    pill_y,
                    pill_w,
                    pill_h,
                    "Connect",
                    f_badge,
                    true,
                    PK_GREEN_DARK,
                );
            }
        }
    } else if c3_tab == 1 {
        if contacts.is_empty() {
            canvas.draw_text(
                item_x,
                l.col_y + 52,
                "No contacts in Address Book yet.",
                14.0 * l.font_scale,
                true,
                PK_TEXT,
                Some(item_w),
            );
            canvas.draw_text_wrapped(
                item_x,
                l.col_y + 78,
                item_w,
                50,
                "Enter a 9-digit Desky ID above and click [+ Add] to save it here with a nickname.",
                f_console,
                false,
                PK_MUTED,
            );
        } else {
            let item_h = (48.0 * l.font_scale) as i32;
            for (idx, entry) in contacts.iter().take(3).enumerate() {
                let iy = l.col_y + 48 + (idx as i32 * (item_h + 8));
                if iy + item_h > l.col_y + l.col_h - 10 {
                    break;
                }
                canvas.draw_rounded_box(item_x, iy, item_w, item_h, PK_INPUT_BG, PK_CARD_BORDER, 8);

                let pill_w = 68;
                let pill_h = 28;
                let pill_x = item_x + item_w - pill_w - 32;
                let pill_y = iy + (item_h - pill_h) / 2;
                let text_avail_w = (pill_x - item_x - 18).max(40);

                canvas.draw_text(
                    item_x + 12,
                    iy + 6,
                    &entry.alias,
                    14.0 * l.font_scale,
                    true,
                    PK_TEXT,
                    Some(text_avail_w),
                );
                let sub_info = format!("{}  •  {}", entry.id, entry.group);
                canvas.draw_text(
                    item_x + 12,
                    iy + 25,
                    &sub_info,
                    12.0 * l.font_scale,
                    false,
                    PK_MUTED,
                    Some(text_avail_w),
                );

                canvas.draw_rounded_box(pill_x, pill_y, pill_w, pill_h, PK_GREEN_TINT, PK_GREEN, 6);
                canvas.draw_text_centered(
                    pill_x,
                    pill_y,
                    pill_w,
                    pill_h,
                    "Connect",
                    f_badge,
                    true,
                    PK_GREEN_DARK,
                );

                let del_w = 20;
                let del_x = item_x + item_w - del_w - 6;
                canvas.draw_rounded_box(del_x, pill_y, del_w, pill_h, PK_CARD, PK_CARD_BORDER, 4);
                canvas.draw_text_centered(del_x, pill_y, del_w, pill_h, "×", 14.0, true, PK_MUTED);
            }
        }
    } else {
        let recent_sessions = crate::identity::get_recent_sessions();
        if recent_sessions.is_empty() {
            canvas.draw_text(
                item_x,
                l.col_y + 52,
                "No recent sessions yet.",
                14.0 * l.font_scale,
                true,
                PK_TEXT,
                Some(item_w),
            );
            canvas.draw_text_wrapped(
                item_x,
                l.col_y + 78,
                item_w,
                50,
                "Remote desks you connect to will appear here for fast 1-click reconnect.",
                f_console,
                false,
                PK_MUTED,
            );
        } else {
            let item_h = (38.0 * l.font_scale) as i32;
            for (idx, session) in recent_sessions.iter().take(3).enumerate() {
                let iy = l.col_y + 48 + (idx as i32 * (item_h + 8));
                if iy + item_h > l.col_y + l.col_h - 10 {
                    break;
                }
                canvas.draw_rounded_box(item_x, iy, item_w, item_h, PK_INPUT_BG, PK_CARD_BORDER, 8);

                let pill_w = 74;
                let pill_h = item_h - 10;
                let pill_x = item_x + item_w - pill_w - 6;
                let pill_y = iy + 5;
                let text_avail_w = (pill_x - item_x - 18).max(40);

                canvas.draw_text_vcenter(
                    item_x + 14,
                    iy,
                    text_avail_w,
                    item_h,
                    session,
                    15.0 * l.font_scale,
                    true,
                    PK_TEXT,
                );

                canvas.draw_rounded_box(pill_x, pill_y, pill_w, pill_h, PK_GREEN_TINT, PK_GREEN, 6);
                canvas.draw_text_centered(
                    pill_x,
                    pill_y,
                    pill_w,
                    pill_h,
                    "Connect",
                    f_badge,
                    true,
                    PK_GREEN_DARK,
                );
            }
        }
    }

    // 5. Card 4: SECURITY & SERVER LOGS (Bottom Right)
    canvas.draw_rounded_box(
        l.col2_x,
        l.col_y,
        l.col_w,
        l.col_h,
        PK_CARD,
        PK_CARD_BORDER,
        12,
    );

    let tag4_w = (184.0 * l.font_scale) as i32;
    let tag4_h = (25.0 * l.font_scale) as i32;
    canvas.draw_rounded_box(
        l.col2_x + 18,
        l.col_y + 14,
        tag4_w,
        tag4_h,
        PK_INPUT_BG,
        PK_CARD_BORDER,
        6,
    );
    canvas.draw_text_centered(
        l.col2_x + 18,
        l.col_y + 14,
        tag4_w,
        tag4_h,
        "CONNECTION & ACTIVITY LOGS",
        f_badge,
        true,
        PK_MUTED,
    );

    let console_box_x = l.col2_x + 18;
    let console_box_y = l.col_y + 48;
    let console_box_w = l.col_w - 36;
    let console_box_h = l.col_h - 60;

    canvas.draw_rounded_box(
        console_box_x,
        console_box_y,
        console_box_w,
        console_box_h,
        PK_CONSOLE_BG,
        PK_CARD_BORDER,
        8,
    );

    let logs = status_logs().lock().map(|g| g.clone()).unwrap_or_default();
    let line_height = (18.5 * l.font_scale) as i32;
    let max_visible_lines = ((console_box_h - 16) / line_height).max(1) as usize;
    let start = logs.len().saturating_sub(max_visible_lines);

    for (i, log_line) in logs[start..].iter().enumerate() {
        let ly = console_box_y + 8 + (i as i32 * line_height);
        canvas.draw_text(
            console_box_x + 12,
            ly,
            log_line,
            f_console,
            false,
            PK_TEXT,
            Some(console_box_w - 24),
        );
    }
}

/// Handles mouse clicks on the Homepage view
pub fn handle_homepage_click(mx: i16, my: i16, win_w: u16, win_h: u16) -> bool {
    let x = mx as i32;
    let y = my as i32;
    let w = win_w as i32;
    let h = win_h as i32;

    // 1. Check Incoming Connection Approval Modal
    if crate::p2p::get_pending_approval().is_some() {
        let mw = (w - 64).clamp(440, 620);
        let mh = 280;
        let modal_x = (w - mw) / 2;
        let modal_y = ((h - mh) / 2).max(64);
        let btn_w = (mw - 72) / 2;
        let btn_h = 48;
        let btn_y = modal_y + 188;
        let acc_x = modal_x + 24;
        let dec_x = modal_x + 48 + btn_w;

        if y >= btn_y && y <= btn_y + btn_h {
            if x >= acc_x && x <= acc_x + btn_w {
                crate::p2p::respond_to_pending_approval(true);
                return true;
            } else if x >= dec_x && x <= dec_x + btn_w {
                crate::p2p::respond_to_pending_approval(false);
                return true;
            }
        }
        return false;
    }

    let l = get_responsive_layout(w, h);

    // 2. Copy ID button
    if x >= l.copy_btn_x
        && x <= l.copy_btn_x + l.copy_btn_w
        && y >= l.copy_btn_y
        && y <= l.copy_btn_y + l.copy_btn_h
    {
        let id_to_copy = {
            let guard = LOCAL_DESKY_ID.lock().unwrap();
            if !guard.is_empty() {
                guard.clone()
            } else {
                crate::identity::load_or_create_identity()
                    .desky_id
                    .unwrap_or_default()
            }
        };
        if !id_to_copy.is_empty() {
            copy_to_clipboard(&id_to_copy);
            append_status_log(&format!(
                "Desky ID '{}' copied to X11 Clipboard!",
                id_to_copy
            ));
        }
        return true;
    }

    // 3. Connect button
    if x >= l.conn_btn_x
        && x <= l.conn_btn_x + l.conn_btn_w
        && y >= l.conn_btn_y
        && y <= l.conn_btn_y + l.conn_btn_h
    {
        let target = get_remote_id_raw();
        handle_homepage_connect(&target);
        return true;
    }

    // 3b. Remote Desk Input Box & Clear [×] button clicked
    if x >= l.input_box_x
        && x <= l.input_box_x + l.input_box_w
        && y >= l.input_box_y
        && y <= l.input_box_y + l.input_box_h
    {
        let clr_x = l.input_box_x + l.input_box_w - 42;
        if x >= clr_x && !get_remote_id_raw().is_empty() {
            clear_remote_id_raw();
        } else {
            INPUT_SELECT_ALL.store(false, Ordering::Relaxed);
        }
        return true;
    }

    // 4. Card 3 Tabs
    let contacts = crate::identity::load_address_book();
    let lan_peers = crate::p2p::get_lan_peers();
    let (t0, t1, t2) = get_card3_tab_rects(
        l.col1_x,
        l.col_y,
        l.font_scale,
        contacts.len(),
        lan_peers.len(),
    );
    let (t0_x, t0_y, t0_w, t0_h, _) = t0;
    let (t1_x, _, t1_w, _, _) = t1;
    let (t2_x, _, t2_w, _, _) = t2;
    let c3_tab = BOTTOM_CARD3_TAB.load(Ordering::Relaxed);

    if y >= t0_y && y <= t0_y + t0_h {
        if x >= t0_x && x <= t0_x + t0_w {
            BOTTOM_CARD3_TAB.store(0, Ordering::Relaxed);
            return true;
        }
        if x >= t1_x && x <= t1_x + t1_w {
            BOTTOM_CARD3_TAB.store(1, Ordering::Relaxed);
            return true;
        }
        if x >= t2_x && x <= t2_x + t2_w {
            BOTTOM_CARD3_TAB.store(2, Ordering::Relaxed);
            trigger_lan_radar_scan();
            return true;
        }

        let act_x = l.col1_x + l.col_w - 76 - 18;
        if x >= act_x && x <= l.col1_x + l.col_w - 18 {
            if c3_tab == 1 {
                let raw = get_remote_id_raw();
                let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
                if digits.len() == 9 {
                    let alias = format!("Desk {}", &digits[6..9]);
                    crate::identity::add_or_update_contact(&digits, &alias, "Favorites", None);
                    append_status_log(&format!("Saved ID {} to Address Book!", digits));
                } else {
                    append_status_log("[!] Enter a 9-digit Desky ID above, then click [+ Add]");
                }
            } else if c3_tab == 2 {
                trigger_lan_radar_scan();
            }
            return true;
        }
    }

    // 5. Card 3 List Item clicks
    let item_x = l.col1_x + 18;
    let item_w = l.col_w - 36;
    if c3_tab == 2 {
        let item_h = (48.0 * l.font_scale) as i32;
        for (idx, peer) in lan_peers.iter().take(3).enumerate() {
            let iy = l.col_y + 48 + (idx as i32 * (item_h + 8));
            if x >= item_x && x <= item_x + item_w && y >= iy && y <= iy + item_h {
                set_remote_id_raw(&peer.id);
                if x >= item_x + item_w - 84 {
                    handle_homepage_connect(&peer.id);
                }
                return true;
            }
        }
    } else if c3_tab == 1 {
        let item_h = (48.0 * l.font_scale) as i32;
        for (idx, entry) in contacts.iter().take(3).enumerate() {
            let iy = l.col_y + 48 + (idx as i32 * (item_h + 8));
            if x >= item_x && x <= item_x + item_w && y >= iy && y <= iy + item_h {
                if x >= item_x + item_w - 28 {
                    crate::identity::delete_contact(&entry.id);
                    append_status_log(&format!("Removed {} from Address Book", entry.alias));
                } else {
                    set_remote_id_raw(&entry.id);
                    if x >= item_x + item_w - 104 {
                        handle_homepage_connect(&entry.id);
                    }
                }
                return true;
            }
        }
    } else {
        let recents = crate::identity::get_recent_sessions();
        let item_h = (38.0 * l.font_scale) as i32;
        for (idx, sess) in recents.iter().take(3).enumerate() {
            let iy = l.col_y + 48 + (idx as i32 * (item_h + 8));
            if x >= item_x && x <= item_x + item_w && y >= iy && y <= iy + item_h {
                set_remote_id_raw(sess);
                if x >= item_x + item_w - 84 {
                    handle_homepage_connect(sess);
                }
                return true;
            }
        }
    }

    false
}

/// Spawns a native X11 approval popup dialog when a remote client requests to connect
pub fn spawn_linux_approval_popup(client_id: String) {
    if POPUP_ACTIVE.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        println!("\n\x1b[1;33m==================================================\x1b[0m");
        println!("\x1b[1;33m  🔔 INCOMING DESKY CONNECTION REQUEST!\x1b[0m");
        println!("\x1b[1;37m  Remote Client ID: \x1b[1;32m{}\x1b[0m", client_id);
        println!("\x1b[1;37m  Click [ACCEPT] or [DECLINE] in the popup window!\x1b[0m");
        println!("\x1b[1;33m==================================================\x1b[0m\n");

        if let Ok((conn, screen_num)) = x11rb::connect(None) {
            let screen = &conn.setup().roots[screen_num];
            let Ok(win_id) = conn.generate_id() else {
                POPUP_ACTIVE.store(false, Ordering::SeqCst);
                return;
            };
            let win_w = 460u16;
            let win_h = 220u16;
            let pos_x = ((screen.width_in_pixels.saturating_sub(win_w)) / 2) as i16;
            let pos_y = ((screen.height_in_pixels.saturating_sub(win_h)) / 2) as i16;

            let win_aux = CreateWindowAux::new()
                .background_pixel(PK_BG)
                .override_redirect(1)
                .event_mask(
                    EventMask::EXPOSURE
                        | EventMask::BUTTON_PRESS
                        | EventMask::KEY_PRESS
                        | EventMask::STRUCTURE_NOTIFY,
                );

            let _ = conn.create_window(
                screen.root_depth,
                win_id,
                screen.root,
                pos_x,
                pos_y,
                win_w,
                win_h,
                2,
                WindowClass::INPUT_OUTPUT,
                screen.root_visual,
                &win_aux,
            );

            let title = "Desky Connection Request";
            let _ = conn.change_property8(
                PropMode::REPLACE,
                win_id,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                title.as_bytes(),
            );

            let _ = conn.map_window(win_id);
            let _ = conn.configure_window(
                win_id,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            );
            let _ = conn.flush();

            let gc_blit = conn.generate_id().unwrap_or(0);
            let _ = conn.create_gc(gc_blit, win_id, &CreateGCAux::new().graphics_exposures(0));
            let mut pop_canvas = PixelCanvas::new(win_w, win_h);

            while crate::p2p::get_pending_approval().is_some() {
                if let Ok(Some(ev)) = conn.poll_for_event() {
                    match ev {
                        x11rb::protocol::Event::Expose(_) => {
                            pop_canvas.clear(PK_BG);
                            pop_canvas.draw_rounded_box(
                                10,
                                10,
                                (win_w - 20) as i32,
                                (win_h - 20) as i32,
                                PK_CARD,
                                PK_GREEN,
                                10,
                            );
                            pop_canvas.draw_text_centered(
                                20,
                                24,
                                (win_w - 40) as i32,
                                24,
                                "INCOMING DESKY SESSION REQUEST",
                                14.0,
                                true,
                                PK_GREEN_DARK,
                            );
                            pop_canvas.draw_text_centered(
                                20,
                                58,
                                (win_w - 40) as i32,
                                32,
                                &format!("Remote Client ID: {}", client_id),
                                20.0,
                                true,
                                PK_TEXT,
                            );
                            pop_canvas.draw_text_centered(
                                20,
                                96,
                                (win_w - 40) as i32,
                                22,
                                "Allow this peer to view and control your desktop?",
                                13.0,
                                false,
                                PK_MUTED,
                            );

                            pop_canvas.draw_rounded_box(30, 140, 190, 46, PK_GREEN, PK_GREEN, 8);
                            pop_canvas.draw_text_centered(
                                30,
                                140,
                                190,
                                46,
                                "Accept (Click / Y)",
                                14.0,
                                true,
                                PK_CARD,
                            );

                            pop_canvas.draw_rounded_box(240, 140, 190, 46, PK_RED, PK_RED, 8);
                            pop_canvas.draw_text_centered(
                                240,
                                140,
                                190,
                                46,
                                "Decline (Click / N)",
                                14.0,
                                true,
                                PK_CARD,
                            );

                            pop_canvas.present_to_x11(&conn, win_id, gc_blit, screen.root_depth);
                            let _ = conn.flush();
                        }
                        x11rb::protocol::Event::ButtonPress(bp) => {
                            let bx = bp.event_x;
                            let by = bp.event_y;
                            if (140..=186).contains(&by) {
                                if (30..=220).contains(&bx) {
                                    crate::p2p::respond_to_pending_approval(true);
                                    break;
                                } else if (240..=430).contains(&bx) {
                                    crate::p2p::respond_to_pending_approval(false);
                                    break;
                                }
                            }
                        }
                        x11rb::protocol::Event::KeyPress(kp) => {
                            if kp.detail == 36 || kp.detail == 29 {
                                crate::p2p::respond_to_pending_approval(true);
                                break;
                            } else if kp.detail == 57 || kp.detail == 9 {
                                crate::p2p::respond_to_pending_approval(false);
                                break;
                            }
                        }
                        _ => {}
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(25));
                }
            }

            let _ = conn.unmap_window(win_id);
            let _ = conn.destroy_window(win_id);
            let _ = conn.flush();
        }
        POPUP_ACTIVE.store(false, Ordering::SeqCst);
        request_main_redraw();
    });
}

pub struct LinuxApp {
    has_x11: bool,
}

impl LinuxApp {
    pub fn new() -> Result<Self> {
        let has_x11 = std::env::var("DISPLAY").is_ok();
        Ok(Self { has_x11 })
    }

    pub fn run(&self) {
        let current_id = {
            let guard = LOCAL_DESKY_ID.lock().unwrap();
            if !guard.is_empty() {
                guard.clone()
            } else {
                crate::identity::load_or_create_identity()
                    .desky_id
                    .unwrap_or_else(|| "Connecting...".into())
            }
        };

        println!("\x1b[1;32m==================================================\x1b[0m");
        println!("\x1b[1;32m  DESKY REMOTE DESKTOP (Linux / X11)\x1b[0m");
        println!(
            "\x1b[1;37m  Permanent Desky ID: \x1b[1;32m{}\x1b[0m",
            current_id
        );
        println!("\x1b[1;37m  Theme: \x1b[0;32mpkasoft.com White & Emerald\x1b[0m");
        println!("\x1b[1;37m  Input Injection: \x1b[0;36mPure Rust X11 / XTest Active\x1b[0m");
        println!(
            "\x1b[1;37m  Display Server: \x1b[0;33m{}\x1b[0m",
            if self.has_x11 {
                "X11 Display Detected"
            } else {
                "Headless / Terminal"
            }
        );
        println!("\x1b[1;32m==================================================\x1b[0m\n");

        if self.has_x11 {
            if let Err(e) = run_viewport_x11_window() {
                info!(
                    "X11 Window loop error ({:?}), falling back to terminal loop",
                    e
                );
                self.run_terminal_loop();
            }
        } else {
            self.run_terminal_loop();
        }
    }

    fn run_terminal_loop(&self) {
        info!("Running Desky Linux host daemon. Press Ctrl+C to exit.");
        while RUNNING.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}
