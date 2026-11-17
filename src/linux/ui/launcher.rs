//! Main X11 Launcher Window, Responsive Homepage & Application Lifecycle
//!
//! Mirrors `src/windows/ui/launcher.rs` by exposing responsive Home page rendering,
//! 4-card layout calculations, LAN radar scanning, incoming approval modals, and lifecycle.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tracing::info;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

use super::remote_desk_edit::{
    format_desky_id_display, get_remote_id_raw, set_remote_id_raw,
};
use super::theme::{
    copy_to_clipboard, create_color_gc, draw_str, fill_bordered_box, open_best_font, X11Palette,
    PK_BG, PK_CARD, PK_GREEN, PK_RED, PK_TEXT,
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
    append_status_log(&format!("Online | Permanent ID: {}", id));
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
            append_status_log(&format!(
                "LAN Radar: Scan complete - Found {} device(s) on local network.",
                peers.len()
            ));
        }
        request_main_redraw();
    });
}

#[derive(Clone, Copy, Debug)]
pub struct ResponsiveLayout {
    pub c1_x: i16,
    pub c2_x: i16,
    pub card_y: i16,
    pub card_w: u16,
    pub card_h: u16,

    pub id_box_x: i16,
    pub id_box_y: i16,
    pub id_box_w: u16,
    pub id_box_h: u16,

    pub copy_btn_x: i16,
    pub copy_btn_y: i16,
    pub copy_btn_w: u16,
    pub copy_btn_h: u16,

    pub input_box_x: i16,
    pub input_box_y: i16,
    pub input_box_w: u16,
    pub input_box_h: u16,

    pub conn_btn_x: i16,
    pub conn_btn_y: i16,
    pub conn_btn_w: u16,
    pub conn_btn_h: u16,

    pub col1_x: i16,
    pub col2_x: i16,
    pub col_y: i16,
    pub col_w: u16,
    pub col_h: u16,
}

pub fn get_responsive_layout(win_w: u16, win_h: u16) -> ResponsiveLayout {
    let w = win_w.max(640) as i32;
    let h = win_h.max(480) as i32;
    let top_offset = 42;
    let margin = 20;
    let gap = 16;
    let card_w = ((w - 2 * margin - gap) / 2).max(260);
    let card_h = 205;
    let card_y = top_offset + 14;

    let c1_x = margin;
    let c2_x = margin + card_w + gap;

    let id_box_x = c1_x + 16;
    let id_box_y = card_y + 46;
    let id_box_w = card_w - 32;
    let id_box_h = 68;

    let copy_btn_x = c1_x + 16;
    let copy_btn_y = id_box_y + id_box_h + 14;
    let copy_btn_w = card_w - 32;
    let copy_btn_h = 42;

    let input_box_x = c2_x + 16;
    let input_box_y = card_y + 46;
    let input_box_w = card_w - 32;
    let input_box_h = 68;

    let conn_btn_x = c2_x + 16;
    let conn_btn_y = copy_btn_y;
    let conn_btn_w = card_w - 32;
    let conn_btn_h = 42;

    let col_y = card_y + card_h + 16;
    let col_w = card_w;
    let col_h = (h - col_y - margin).max(160);

    ResponsiveLayout {
        c1_x: c1_x as i16,
        c2_x: c2_x as i16,
        card_y: card_y as i16,
        card_w: card_w as u16,
        card_h: card_h as u16,
        id_box_x: id_box_x as i16,
        id_box_y: id_box_y as i16,
        id_box_w: id_box_w as u16,
        id_box_h: id_box_h as u16,
        copy_btn_x: copy_btn_x as i16,
        copy_btn_y: copy_btn_y as i16,
        copy_btn_w: copy_btn_w as u16,
        copy_btn_h: copy_btn_h as u16,
        input_box_x: input_box_x as i16,
        input_box_y: input_box_y as i16,
        input_box_w: input_box_w as u16,
        input_box_h: input_box_h as u16,
        conn_btn_x: conn_btn_x as i16,
        conn_btn_y: conn_btn_y as i16,
        conn_btn_w: conn_btn_w as u16,
        conn_btn_h: conn_btn_h as u16,
        col1_x: c1_x as i16,
        col2_x: c2_x as i16,
        col_y: col_y as i16,
        col_w: col_w as u16,
        col_h: col_h as u16,
    }
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

/// Renders the complete pkasoft.com White Theme Homepage onto the X11 drawable
pub fn draw_homepage_view<C: Connection>(
    conn: &C,
    canvas: Drawable,
    win_w: u16,
    win_h: u16,
    pal: &X11Palette,
) {
    let l = get_responsive_layout(win_w, win_h);

    // 1. Pending Incoming Connection Approval Modal
    if let Some(req) = crate::p2p::get_pending_approval() {
        let mw = 480u16;
        let mh = 230u16;
        let mx = ((win_w.saturating_sub(mw)) / 2) as i16;
        let my = ((win_h.saturating_sub(mh)) / 2).max(60) as i16;

        fill_bordered_box(conn, canvas, mx, my, mw, mh, pal.gc_card, pal.gc_green);
        fill_bordered_box(
            conn,
            canvas,
            mx + 110,
            my + 18,
            260,
            26,
            pal.gc_green_tint,
            pal.gc_green,
        );
        draw_str(
            conn,
            canvas,
            pal.txt_green_on_tint,
            mx + 128,
            my + 36,
            "INCOMING CONNECTION REQUEST",
        );

        let id_line = format!("Remote Client ID: {}", req.client_id);
        draw_str(
            conn,
            canvas,
            pal.txt_large_dark_on_card,
            mx + 95,
            my + 84,
            &id_line,
        );
        draw_str(
            conn,
            canvas,
            pal.txt_muted_on_card,
            mx + 52,
            my + 118,
            "This remote peer is requesting permission to view and control your desktop.",
        );

        fill_bordered_box(
            conn,
            canvas,
            mx + 30,
            my + 154,
            195,
            46,
            pal.gc_green,
            pal.gc_green,
        );
        draw_str(
            conn,
            canvas,
            pal.txt_white_on_green,
            mx + 64,
            my + 182,
            "Accept Connection",
        );

        fill_bordered_box(
            conn,
            canvas,
            mx + 255,
            my + 154,
            195,
            46,
            pal.gc_red,
            pal.gc_red,
        );
        draw_str(
            conn,
            canvas,
            pal.txt_white_on_red,
            mx + 318,
            my + 182,
            "Decline",
        );
        return;
    }

    // 2. Card 1: THIS DESK (Top Left)
    fill_bordered_box(
        conn,
        canvas,
        l.c1_x,
        l.card_y,
        l.card_w,
        l.card_h,
        pal.gc_card,
        pal.gc_green,
    );
    fill_bordered_box(
        conn,
        canvas,
        l.c1_x + 16,
        l.card_y + 14,
        96,
        22,
        pal.gc_green_tint,
        pal.gc_green,
    );
    draw_str(
        conn,
        canvas,
        pal.txt_green_on_tint,
        l.c1_x + 26,
        l.card_y + 30,
        "THIS DESK",
    );

    fill_bordered_box(
        conn,
        canvas,
        l.id_box_x,
        l.id_box_y,
        l.id_box_w,
        l.id_box_h,
        pal.gc_input_bg,
        pal.gc_border,
    );
    draw_str(
        conn,
        canvas,
        pal.txt_muted_on_input,
        l.id_box_x + 14,
        l.id_box_y + 20,
        "YOUR PERMANENT DESKY ID",
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
    draw_str(
        conn,
        canvas,
        pal.txt_large_green_on_input,
        l.id_box_x + 14,
        l.id_box_y + 52,
        &current_id,
    );

    fill_bordered_box(
        conn,
        canvas,
        l.copy_btn_x,
        l.copy_btn_y,
        l.copy_btn_w,
        l.copy_btn_h,
        pal.gc_card,
        pal.gc_border,
    );
    let copy_text_x = l.copy_btn_x + (l.copy_btn_w as i16 / 2) - 48;
    draw_str(
        conn,
        canvas,
        pal.txt_dark_on_card,
        copy_text_x,
        l.copy_btn_y + 26,
        "[ Copy Desky ID ]",
    );

    // 3. Card 2: REMOTE DESK (Top Right)
    fill_bordered_box(
        conn,
        canvas,
        l.c2_x,
        l.card_y,
        l.card_w,
        l.card_h,
        pal.gc_card,
        pal.gc_border,
    );
    fill_bordered_box(
        conn,
        canvas,
        l.c2_x + 16,
        l.card_y + 14,
        114,
        22,
        pal.gc_input_bg,
        pal.gc_border,
    );
    draw_str(
        conn,
        canvas,
        pal.txt_muted_on_input,
        l.c2_x + 26,
        l.card_y + 30,
        "REMOTE DESK",
    );

    fill_bordered_box(
        conn,
        canvas,
        l.input_box_x,
        l.input_box_y,
        l.input_box_w,
        l.input_box_h,
        pal.gc_input_bg,
        pal.gc_green,
    );
    draw_str(
        conn,
        canvas,
        pal.txt_muted_on_input,
        l.input_box_x + 14,
        l.input_box_y + 20,
        "ENTER 9-DIGIT PARTNER ID (TYPE DIGITS & ENTER)",
    );

    let typed_raw = get_remote_id_raw();
    if typed_raw.is_empty() {
        draw_str(
            conn,
            canvas,
            pal.txt_muted_on_input,
            l.input_box_x + 14,
            l.input_box_y + 50,
            "e.g. 800 258 756 _",
        );
    } else {
        let formatted = format!("{} _", format_desky_id_display(&typed_raw));
        draw_str(
            conn,
            canvas,
            pal.txt_large_dark_on_input,
            l.input_box_x + 14,
            l.input_box_y + 52,
            &formatted,
        );
    }

    fill_bordered_box(
        conn,
        canvas,
        l.conn_btn_x,
        l.conn_btn_y,
        l.conn_btn_w,
        l.conn_btn_h,
        pal.gc_green,
        pal.gc_green,
    );
    let conn_text_x = l.conn_btn_x + (l.conn_btn_w as i16 / 2) - 56;
    draw_str(
        conn,
        canvas,
        pal.txt_white_on_green,
        conn_text_x,
        l.conn_btn_y + 26,
        "Connect to Desk ->",
    );

    // 4. Card 3: RECENTS / CONTACTS / LAN RADAR (Bottom Left)
    fill_bordered_box(
        conn,
        canvas,
        l.col1_x,
        l.col_y,
        l.col_w,
        l.col_h,
        pal.gc_card,
        pal.gc_border,
    );

    let c3_tab = BOTTOM_CARD3_TAB.load(Ordering::Relaxed);
    let contacts = crate::identity::load_address_book();
    let lan_peers = crate::p2p::get_lan_peers();

    let tab_y = l.col_y + 14;
    let t0_x = l.col1_x + 16;
    let t0_w = 88u16;
    let t1_x = t0_x + t0_w as i16 + 6;
    let t1_w = 112u16;
    let t2_x = t1_x + t1_w as i16 + 6;
    let t2_w = 122u16;

    // Tab 0: RECENTS
    fill_bordered_box(
        conn,
        canvas,
        t0_x,
        tab_y,
        t0_w,
        24,
        if c3_tab == 0 {
            pal.gc_green_tint
        } else {
            pal.gc_input_bg
        },
        if c3_tab == 0 {
            pal.gc_green
        } else {
            pal.gc_border
        },
    );
    draw_str(
        conn,
        canvas,
        if c3_tab == 0 {
            pal.txt_green_on_tint
        } else {
            pal.txt_muted_on_input
        },
        t0_x + 12,
        tab_y + 17,
        "RECENTS",
    );

    // Tab 1: CONTACTS
    fill_bordered_box(
        conn,
        canvas,
        t1_x,
        tab_y,
        t1_w,
        24,
        if c3_tab == 1 {
            pal.gc_green_tint
        } else {
            pal.gc_input_bg
        },
        if c3_tab == 1 {
            pal.gc_green
        } else {
            pal.gc_border
        },
    );
    let c_label = if contacts.is_empty() {
        "CONTACTS".to_string()
    } else {
        format!("CONTACTS ({})", contacts.len())
    };
    draw_str(
        conn,
        canvas,
        if c3_tab == 1 {
            pal.txt_green_on_tint
        } else {
            pal.txt_muted_on_input
        },
        t1_x + 10,
        tab_y + 17,
        &c_label,
    );

    // Tab 2: LAN RADAR
    fill_bordered_box(
        conn,
        canvas,
        t2_x,
        tab_y,
        t2_w,
        24,
        if c3_tab == 2 {
            pal.gc_green_tint
        } else {
            pal.gc_input_bg
        },
        if c3_tab == 2 {
            pal.gc_green
        } else {
            pal.gc_border
        },
    );
    let r_label = if lan_peers.is_empty() {
        "LAN RADAR".to_string()
    } else {
        format!("LAN RADAR ({})", lan_peers.len())
    };
    draw_str(
        conn,
        canvas,
        if c3_tab == 2 {
            pal.txt_green_on_tint
        } else {
            pal.txt_muted_on_input
        },
        t2_x + 10,
        tab_y + 17,
        &r_label,
    );

    if c3_tab == 1 {
        let act_w = 64u16;
        let act_x = l.col1_x + l.col_w as i16 - act_w as i16 - 16;
        fill_bordered_box(
            conn,
            canvas,
            act_x,
            tab_y,
            act_w,
            24,
            pal.gc_green_tint,
            pal.gc_green,
        );
        draw_str(conn, canvas, pal.txt_green_on_tint, act_x + 12, tab_y + 17, "+ Add");
    } else if c3_tab == 2 {
        let act_w = 68u16;
        let act_x = l.col1_x + l.col_w as i16 - act_w as i16 - 16;
        fill_bordered_box(
            conn,
            canvas,
            act_x,
            tab_y,
            act_w,
            24,
            pal.gc_green_tint,
            pal.gc_green,
        );
        let s_lbl = if is_lan_scan_active() { "Scan.." } else { "Scan" };
        draw_str(conn, canvas, pal.txt_green_on_tint, act_x + 14, tab_y + 17, s_lbl);
    }

    let item_w = l.col_w.saturating_sub(32);
    let item_x = l.col1_x + 16;

    if c3_tab == 0 {
        let recents = crate::identity::get_recent_sessions();
        if recents.is_empty() {
            draw_str(
                conn,
                canvas,
                pal.txt_muted_on_card,
                item_x,
                l.col_y + 68,
                "No recent sessions yet.",
            );
            draw_str(
                conn,
                canvas,
                pal.txt_muted_on_card,
                item_x,
                l.col_y + 92,
                "Remote desks you connect to appear here for 1-click reconnect.",
            );
        } else {
            for (idx, sess) in recents.iter().take(3).enumerate() {
                let iy = l.col_y + 48 + (idx as i16 * 44);
                fill_bordered_box(
                    conn,
                    canvas,
                    item_x,
                    iy,
                    item_w,
                    36,
                    pal.gc_input_bg,
                    pal.gc_border,
                );
                draw_str(conn, canvas, pal.txt_dark_on_input, item_x + 14, iy + 23, sess);

                let btn_w = 78u16;
                let btn_x = item_x + item_w as i16 - btn_w as i16 - 6;
                fill_bordered_box(
                    conn,
                    canvas,
                    btn_x,
                    iy + 5,
                    btn_w,
                    26,
                    pal.gc_green_tint,
                    pal.gc_green,
                );
                draw_str(
                    conn,
                    canvas,
                    pal.txt_green_on_tint,
                    btn_x + 12,
                    iy + 23,
                    "Connect",
                );
            }
        }
    } else if c3_tab == 1 {
        if contacts.is_empty() {
            draw_str(
                conn,
                canvas,
                pal.txt_muted_on_card,
                item_x,
                l.col_y + 68,
                "No contacts in Address Book yet.",
            );
            draw_str(
                conn,
                canvas,
                pal.txt_muted_on_card,
                item_x,
                l.col_y + 92,
                "Type a 9-digit ID in Remote Desk and click [+ Add] to save.",
            );
        } else {
            for (idx, entry) in contacts.iter().take(3).enumerate() {
                let iy = l.col_y + 48 + (idx as i16 * 44);
                fill_bordered_box(
                    conn,
                    canvas,
                    item_x,
                    iy,
                    item_w,
                    36,
                    pal.gc_input_bg,
                    pal.gc_border,
                );
                let line = format!("{}  ({})", entry.alias, entry.id);
                draw_str(conn, canvas, pal.txt_dark_on_input, item_x + 14, iy + 23, &line);

                let btn_w = 74u16;
                let btn_x = item_x + item_w as i16 - btn_w as i16 - 32;
                fill_bordered_box(
                    conn,
                    canvas,
                    btn_x,
                    iy + 5,
                    btn_w,
                    26,
                    pal.gc_green_tint,
                    pal.gc_green,
                );
                draw_str(
                    conn,
                    canvas,
                    pal.txt_green_on_tint,
                    btn_x + 10,
                    iy + 23,
                    "Connect",
                );

                let del_x = item_x + item_w as i16 - 24;
                fill_bordered_box(
                    conn,
                    canvas,
                    del_x,
                    iy + 5,
                    20,
                    26,
                    pal.gc_card,
                    pal.gc_border,
                );
                draw_str(conn, canvas, pal.txt_red_on_card, del_x + 6, iy + 23, "x");
            }
        }
    } else if lan_peers.is_empty() {
        let msg = if is_lan_scan_active() {
            "Scanning local subnet for Desky devices..."
        } else {
            "No Desky devices found on local network."
        };
        draw_str(conn, canvas, pal.txt_muted_on_card, item_x, l.col_y + 68, msg);
        draw_str(
            conn,
            canvas,
            pal.txt_muted_on_card,
            item_x,
            l.col_y + 92,
            "Devices on the same Wi-Fi/LAN appear here for direct P2P connect.",
        );
    } else {
        for (idx, peer) in lan_peers.iter().take(3).enumerate() {
            let iy = l.col_y + 48 + (idx as i16 * 44);
            fill_bordered_box(
                conn,
                canvas,
                item_x,
                iy,
                item_w,
                36,
                pal.gc_input_bg,
                pal.gc_border,
            );
            let line = format!(
                "{} | ID: {} ({})",
                peer.hostname,
                peer.id,
                peer.ip.map(|i| i.to_string()).unwrap_or_default()
            );
            draw_str(conn, canvas, pal.txt_dark_on_input, item_x + 12, iy + 23, &line);

            let btn_w = 78u16;
            let btn_x = item_x + item_w as i16 - btn_w as i16 - 6;
            fill_bordered_box(
                conn,
                canvas,
                btn_x,
                iy + 5,
                btn_w,
                26,
                pal.gc_green_tint,
                pal.gc_green,
            );
            draw_str(
                conn,
                canvas,
                pal.txt_green_on_tint,
                btn_x + 12,
                iy + 23,
                "Connect",
            );
        }
    }

    // 5. Card 4: SECURITY & SERVER LOGS (Bottom Right)
    fill_bordered_box(
        conn,
        canvas,
        l.col2_x,
        l.col_y,
        l.col_w,
        l.col_h,
        pal.gc_card,
        pal.gc_border,
    );
    fill_bordered_box(
        conn,
        canvas,
        l.col2_x + 16,
        l.col_y + 14,
        154,
        22,
        pal.gc_input_bg,
        pal.gc_border,
    );
    draw_str(
        conn,
        canvas,
        pal.txt_muted_on_input,
        l.col2_x + 26,
        l.col_y + 30,
        "SECURITY & SERVER",
    );

    let con_x = l.col2_x + 16;
    let con_y = l.col_y + 46;
    let con_w = l.col_w.saturating_sub(32);
    let con_h = l.col_h.saturating_sub(58);
    fill_bordered_box(
        conn,
        canvas,
        con_x,
        con_y,
        con_w,
        con_h,
        pal.gc_input_bg,
        pal.gc_border,
    );

    let logs = status_logs().lock().map(|g| g.clone()).unwrap_or_default();
    let line_h = 18i16;
    let max_lines = ((con_h as i16 - 12) / line_h).max(1) as usize;
    let start = logs.len().saturating_sub(max_lines);
    let max_chars = ((con_w as usize).saturating_sub(20) / 8).max(20);

    for (i, log_line) in logs[start..].iter().enumerate() {
        let truncated: String = log_line.chars().take(max_chars).collect();
        draw_str(
            conn,
            canvas,
            pal.txt_dark_on_input,
            con_x + 10,
            con_y + 20 + (i as i16 * line_h),
            &truncated,
        );
    }
}

/// Handles mouse clicks on the Homepage view
pub fn handle_homepage_click(mx: i16, my: i16, win_w: u16, win_h: u16) -> bool {
    // 1. Check Incoming Connection Approval Modal
    if crate::p2p::get_pending_approval().is_some() {
        let mw = 480u16;
        let mh = 230u16;
        let modal_x = ((win_w.saturating_sub(mw)) / 2) as i16;
        let modal_y = ((win_h.saturating_sub(mh)) / 2).max(60) as i16;
        if my >= modal_y + 154 && my <= modal_y + 200 {
            if mx >= modal_x + 30 && mx <= modal_x + 225 {
                crate::p2p::respond_to_pending_approval(true);
                return true;
            } else if mx >= modal_x + 255 && mx <= modal_x + 450 {
                crate::p2p::respond_to_pending_approval(false);
                return true;
            }
        }
        return false;
    }

    let l = get_responsive_layout(win_w, win_h);

    // 2. Copy ID button
    if mx >= l.copy_btn_x
        && mx <= l.copy_btn_x + l.copy_btn_w as i16
        && my >= l.copy_btn_y
        && my <= l.copy_btn_y + l.copy_btn_h as i16
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
    if mx >= l.conn_btn_x
        && mx <= l.conn_btn_x + l.conn_btn_w as i16
        && my >= l.conn_btn_y
        && my <= l.conn_btn_y + l.conn_btn_h as i16
    {
        let target = get_remote_id_raw();
        handle_homepage_connect(&target);
        return true;
    }

    // 4. Card 3 Tabs
    let tab_y = l.col_y + 14;
    let t0_x = l.col1_x + 16;
    let t1_x = t0_x + 88 + 6;
    let t2_x = t1_x + 112 + 6;

    if my >= tab_y && my <= tab_y + 24 {
        if mx >= t0_x && mx <= t0_x + 88 {
            BOTTOM_CARD3_TAB.store(0, Ordering::Relaxed);
            return true;
        } else if mx >= t1_x && mx <= t1_x + 112 {
            BOTTOM_CARD3_TAB.store(1, Ordering::Relaxed);
            return true;
        } else if mx >= t2_x && mx <= t2_x + 122 {
            BOTTOM_CARD3_TAB.store(2, Ordering::Relaxed);
            trigger_lan_radar_scan();
            return true;
        }

        let c3_tab = BOTTOM_CARD3_TAB.load(Ordering::Relaxed);
        let act_x = l.col1_x + l.col_w as i16 - 68 - 16;
        if mx >= act_x && mx <= act_x + 68 {
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
    let c3_tab = BOTTOM_CARD3_TAB.load(Ordering::Relaxed);
    let item_x = l.col1_x + 16;
    let item_w = l.col_w.saturating_sub(32) as i16;
    for idx in 0..3 {
        let iy = l.col_y + 48 + (idx as i16 * 44);
        if mx >= item_x && mx <= item_x + item_w && my >= iy && my <= iy + 36 {
            if c3_tab == 0 {
                let recents = crate::identity::get_recent_sessions();
                if let Some(sess) = recents.get(idx) {
                    set_remote_id_raw(sess);
                    if mx >= item_x + item_w - 84 {
                        handle_homepage_connect(sess);
                    }
                }
            } else if c3_tab == 1 {
                let contacts = crate::identity::load_address_book();
                if let Some(entry) = contacts.get(idx) {
                    if mx >= item_x + item_w - 26 {
                        crate::identity::delete_contact(&entry.id);
                        append_status_log(&format!("Removed {} from Address Book", entry.alias));
                    } else {
                        set_remote_id_raw(&entry.id);
                        if mx >= item_x + item_w - 110 {
                            handle_homepage_connect(&entry.id);
                        }
                    }
                }
            } else {
                let peers = crate::p2p::get_lan_peers();
                if let Some(peer) = peers.get(idx) {
                    set_remote_id_raw(&peer.id);
                    if mx >= item_x + item_w - 84 {
                        handle_homepage_connect(&peer.id);
                    }
                }
            }
            return true;
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
            let win_w = 440u16;
            let win_h = 210u16;
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

            let font_id = open_best_font(&conn, &["9x15bold", "9x15", "fixed"]);

            let gc_card = create_color_gc(&conn, win_id, PK_CARD, PK_CARD, font_id);
            let gc_green = create_color_gc(&conn, win_id, PK_GREEN, PK_GREEN, font_id);
            let gc_red = create_color_gc(&conn, win_id, PK_RED, PK_RED, font_id);
            let gc_text_dark = create_color_gc(&conn, win_id, PK_TEXT, PK_CARD, font_id);
            let gc_text_accept = create_color_gc(&conn, win_id, PK_CARD, PK_GREEN, font_id);
            let gc_text_decline = create_color_gc(&conn, win_id, PK_CARD, PK_RED, font_id);

            while crate::p2p::get_pending_approval().is_some() {
                if let Ok(Some(ev)) = conn.poll_for_event() {
                    match ev {
                        x11rb::protocol::Event::Expose(_) => {
                            let card = Rectangle {
                                x: 12,
                                y: 12,
                                width: 416,
                                height: 186,
                            };
                            let _ = conn.poly_fill_rectangle(win_id, gc_card, &[card]);
                            let _ = conn.poly_rectangle(win_id, gc_green, &[card]);

                            let line1 = "INCOMING DESKY SESSION REQUEST";
                            let line2 = format!("Remote Client ID: {} wants to connect.", client_id);
                            let line3 = "Allow this peer to view and control your desktop?";
                            let _ =
                                conn.image_text8(win_id, gc_text_dark, 26, 46, line1.as_bytes());
                            let _ =
                                conn.image_text8(win_id, gc_text_dark, 26, 78, line2.as_bytes());
                            let _ =
                                conn.image_text8(win_id, gc_text_dark, 26, 104, line3.as_bytes());

                            let btn_acc = Rectangle {
                                x: 30,
                                y: 132,
                                width: 180,
                                height: 46,
                            };
                            let _ = conn.poly_fill_rectangle(win_id, gc_green, &[btn_acc]);
                            let _ = conn.image_text8(
                                win_id,
                                gc_text_accept,
                                46,
                                160,
                                b"ACCEPT (Click / Y)",
                            );

                            let btn_dec = Rectangle {
                                x: 230,
                                y: 132,
                                width: 180,
                                height: 46,
                            };
                            let _ = conn.poly_fill_rectangle(win_id, gc_red, &[btn_dec]);
                            let _ = conn.image_text8(
                                win_id,
                                gc_text_decline,
                                244,
                                160,
                                b"DECLINE (Click / N)",
                            );

                            let _ = conn.flush();
                        }
                        x11rb::protocol::Event::ButtonPress(bp) => {
                            let bx = bp.event_x;
                            let by = bp.event_y;
                            if (132..=178).contains(&by) {
                                if (30..=210).contains(&bx) {
                                    crate::p2p::respond_to_pending_approval(true);
                                    break;
                                } else if (230..=410).contains(&bx) {
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
