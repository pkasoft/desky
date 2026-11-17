//! Viewport Session Status & Alert View (Linux X11)
//!
//! Mirrors `src/windows/ui/viewport/session_status_view.rs` by displaying
//! connection state, authorization prompts, unattended password input, and error cards.

use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Drawable, KeyButMask};

use super::tabs::{get_session_tabs, ACTIVE_TAB_INDEX};
use super::toolbar::TOOLBAR_HEIGHT;
use crate::linux::ui::remote_desk_edit::x11_keycode_to_ascii;
use crate::linux::ui::theme::{draw_str, fill_bordered_box, X11Palette};
use crate::p2p::SessionStatus;

static PWD_INPUT_BUFFER: OnceLock<Mutex<String>> = OnceLock::new();

fn pwd_store() -> &'static Mutex<String> {
    PWD_INPUT_BUFFER.get_or_init(|| Mutex::new(String::new()))
}

pub fn get_password_input() -> String {
    pwd_store().lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn clear_password_input() {
    if let Ok(mut g) = pwd_store().lock() {
        g.clear();
    }
}

pub fn draw_session_status_view<C: Connection>(
    conn: &C,
    canvas: Drawable,
    win_w: u16,
    win_h: u16,
    target_id: &str,
    status: &SessionStatus,
    pal: &X11Palette,
) {
    let is_err = matches!(status, SessionStatus::Error(_));
    let cw = 480u16;
    let ch = 260u16;
    let cx = ((win_w.saturating_sub(cw)) / 2) as i16;
    let cy = (TOOLBAR_HEIGHT + (win_h.saturating_sub(TOOLBAR_HEIGHT + ch)) / 2) as i16;

    let border_gc = if is_err { pal.gc_red } else { pal.gc_green };
    fill_bordered_box(conn, canvas, cx, cy, cw, ch, pal.gc_card, border_gc);

    let title = match status {
        SessionStatus::Error(_) => "Connection Failed",
        SessionStatus::WaitingApproval(_) => "Waiting for Host Authorization",
        SessionStatus::PasswordRequired(_) => "Unattended Access Password Required",
        SessionStatus::Connecting(_) => "Connecting to Remote Desk...",
        _ => "Session Status",
    };
    draw_str(
        conn,
        canvas,
        if is_err {
            pal.txt_red_on_card
        } else {
            pal.txt_large_dark_on_card
        },
        cx + 32,
        cy + 42,
        title,
    );

    fill_bordered_box(
        conn,
        canvas,
        cx + 32,
        cy + 58,
        180,
        26,
        pal.gc_green_tint,
        pal.gc_green,
    );
    draw_str(
        conn,
        canvas,
        pal.txt_green_on_tint,
        cx + 46,
        cy + 76,
        &format!("ID: {}", target_id),
    );

    if matches!(status, SessionStatus::PasswordRequired(_)) {
        fill_bordered_box(
            conn,
            canvas,
            cx + 32,
            cy + 100,
            cw - 64,
            68,
            pal.gc_input_bg,
            pal.gc_green,
        );
        draw_str(
            conn,
            canvas,
            pal.txt_muted_on_input,
            cx + 44,
            cy + 122,
            "Type unattended password and press Enter:",
        );
        let pwd = get_password_input();
        let masked = if pwd.is_empty() {
            "Type password...".to_string()
        } else {
            "*".repeat(pwd.len())
        };
        draw_str(conn, canvas, pal.txt_dark_on_input, cx + 44, cy + 150, &masked);
    } else {
        fill_bordered_box(
            conn,
            canvas,
            cx + 32,
            cy + 100,
            cw - 64,
            68,
            pal.gc_input_bg,
            pal.gc_border,
        );
        let detail = match status {
            SessionStatus::Error(e) => e.clone(),
            SessionStatus::WaitingApproval(_) => {
                "Prompt shown on remote desktop. Waiting for operator approval...".to_string()
            }
            SessionStatus::Connecting(_) => {
                "Contacting rendezvous server & establishing encrypted P2P tunnel...".to_string()
            }
            _ => "Session is idle or disconnected.".to_string(),
        };
        let line1: String = detail.chars().take(50).collect();
        let line2: String = detail.chars().skip(50).take(50).collect();
        draw_str(conn, canvas, pal.txt_muted_on_input, cx + 44, cy + 126, &line1);
        if !line2.is_empty() {
            draw_str(conn, canvas, pal.txt_muted_on_input, cx + 44, cy + 148, &line2);
        }
    }

    // Action Buttons: [ Retry / Submit ] and [ Return to Home ]
    fill_bordered_box(
        conn,
        canvas,
        cx + 32,
        cy + 190,
        195,
        42,
        pal.gc_green,
        pal.gc_green,
    );
    let b1_label = if matches!(status, SessionStatus::PasswordRequired(_)) {
        "Unlock & Connect"
    } else {
        "Retry Connection"
    };
    draw_str(conn, canvas, pal.txt_white_on_green, cx + 62, cy + 216, b1_label);

    fill_bordered_box(
        conn,
        canvas,
        cx + 250,
        cy + 190,
        195,
        42,
        pal.gc_card,
        pal.gc_border,
    );
    draw_str(
        conn,
        canvas,
        pal.txt_dark_on_card,
        cx + 288,
        cy + 216,
        "Return to Home",
    );
}

pub fn handle_status_view_click(mx: i16, my: i16, win_w: u16, win_h: u16) -> bool {
    let status = crate::p2p::get_session_status();
    let cw = 480u16;
    let ch = 260u16;
    let cx = ((win_w.saturating_sub(cw)) / 2) as i16;
    let cy = (TOOLBAR_HEIGHT + (win_h.saturating_sub(TOOLBAR_HEIGHT + ch)) / 2) as i16;

    if my >= cy + 190 && my <= cy + 232 {
        if mx >= cx + 32 && mx <= cx + 227 {
            if matches!(status, SessionStatus::PasswordRequired(_)) {
                let pwd = get_password_input();
                if !pwd.is_empty() {
                    crate::p2p::submit_unattended_password(&pwd);
                    clear_password_input();
                }
            } else {
                let active_idx = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
                if let Some(tab) = get_session_tabs().get(active_idx) {
                    crate::p2p::start_client_session(&tab.id, 60, true);
                }
            }
            return true;
        } else if mx >= cx + 250 && mx <= cx + 445 {
            crate::p2p::stop_client_session();
            ACTIVE_TAB_INDEX.store(0, Ordering::Relaxed);
            return true;
        }
    }
    false
}

pub fn handle_password_key_input(keycode: u8, state: KeyButMask) {
    match keycode {
        22 => {
            if let Ok(mut g) = pwd_store().lock() {
                g.pop();
            }
        }
        36 | 104 => {
            let pwd = get_password_input();
            if !pwd.is_empty() {
                crate::p2p::submit_unattended_password(&pwd);
                clear_password_input();
            }
        }
        9 => {
            crate::p2p::stop_client_session();
            ACTIVE_TAB_INDEX.store(0, Ordering::Relaxed);
        }
        _ => {
            let shift = state.contains(KeyButMask::SHIFT);
            if let Some(ch) = x11_keycode_to_ascii(keycode, shift) {
                if let Ok(mut g) = pwd_store().lock() {
                    g.push(ch);
                }
            }
        }
    }
}
