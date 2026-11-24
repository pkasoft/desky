//! Viewport Session Status & Alert View (Linux X11)
//!
//! Mirrors `src/windows/ui/viewport/session_status_view.rs` by displaying
//! connection state, authorization prompts, unattended password input, and error cards
//! with anti-aliased TrueType typography.

use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};
use x11rb::protocol::xproto::KeyButMask;

use super::tabs::{get_session_tabs, ACTIVE_TAB_INDEX};
use super::toolbar::TOOLBAR_HEIGHT;
use crate::linux::ui::remote_desk_edit::x11_keycode_to_ascii;
use crate::linux::ui::theme::{
    PixelCanvas, PK_AMBER, PK_CARD, PK_CARD_BORDER, PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT,
    PK_INPUT_BG, PK_MUTED, PK_RED, PK_RED_TINT, PK_TEXT,
};
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

pub fn draw_session_status_view(
    canvas: &mut PixelCanvas,
    win_w: u16,
    win_h: u16,
    target_id: &str,
    status: &SessionStatus,
) {
    let w = win_w as i32;
    let h = win_h as i32;
    let tb_h = TOOLBAR_HEIGHT as i32;

    let is_err = matches!(status, SessionStatus::Error(_));
    let is_pwd = matches!(status, SessionStatus::PasswordRequired(_));
    let is_wait = matches!(status, SessionStatus::WaitingApproval(_));

    let card_w = (w - 48).clamp(400, 520);
    let card_h = 300;
    let card_x = (w - card_w) / 2;
    let card_y = (tb_h + (h - tb_h - card_h) / 2).max(tb_h + 16);

    canvas.draw_rounded_box(card_x, card_y, card_w, card_h, PK_CARD, PK_CARD_BORDER, 10);

    // Top accent strip
    let accent = if is_err {
        PK_RED
    } else if is_wait || is_pwd {
        PK_AMBER
    } else {
        PK_GREEN
    };
    canvas.fill_rect(card_x + 8, card_y + 1, card_w - 16, 4, accent);

    // Title
    let title = match status {
        SessionStatus::Error(_) => "Connection Failed",
        SessionStatus::WaitingApproval(_) => "Waiting for Host Authorization",
        SessionStatus::PasswordRequired(_) => "Unattended Access Password Required",
        SessionStatus::Connecting(_) => "Connecting to Remote Desk",
        SessionStatus::Connected(_) => "Authorized — Starting Video Stream",
        _ => "Session Offline",
    };
    canvas.draw_text_centered(
        card_x + 16,
        card_y + 24,
        card_w - 32,
        28,
        title,
        19.0,
        true,
        if is_err { PK_RED } else { PK_TEXT },
    );

    // Target ID Badge Pill
    let pill_w = 180;
    let pill_h = 28;
    let pill_x = card_x + (card_w - pill_w) / 2;
    let pill_y = card_y + 62;
    let (pill_bg, pill_border, pill_fg) = if is_err {
        (PK_RED_TINT, PK_RED, PK_RED)
    } else {
        (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
    };
    canvas.draw_rounded_box(pill_x, pill_y, pill_w, pill_h, pill_bg, pill_border, 6);
    canvas.draw_text_centered(
        pill_x,
        pill_y,
        pill_w,
        pill_h,
        &format!("ID: {}", target_id),
        13.5,
        true,
        pill_fg,
    );

    // Message / Password Box
    let msg_x = card_x + 24;
    let msg_y = card_y + 104;
    let msg_w = card_w - 48;
    let msg_h = 84;

    if is_pwd {
        canvas.draw_rounded_box(msg_x, msg_y, msg_w, msg_h, PK_INPUT_BG, PK_GREEN, 8);
        canvas.draw_text_centered(
            msg_x + 12,
            msg_y + 8,
            msg_w - 24,
            22,
            "Enter remote master password (type on keyboard):",
            12.5,
            false,
            PK_MUTED,
        );
        let pwd = get_password_input();
        let masked = if pwd.is_empty() {
            "Type password here...".to_string()
        } else {
            "• ".repeat(pwd.len())
        };
        canvas.draw_text_centered(
            msg_x + 12,
            msg_y + 36,
            msg_w - 24,
            34,
            &masked,
            16.0,
            true,
            if pwd.is_empty() { PK_MUTED } else { PK_TEXT },
        );
    } else {
        let (box_bg, box_border, txt_col) = if is_err {
            (PK_RED_TINT, PK_CARD_BORDER, PK_RED)
        } else {
            (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
        };
        canvas.draw_rounded_box(msg_x, msg_y, msg_w, msg_h, box_bg, box_border, 8);
        let detail = match status {
            SessionStatus::Error(e) => e.clone(),
            SessionStatus::WaitingApproval(_) => {
                "An authorization prompt was displayed on the remote computer's screen. Waiting for the operator to accept your request...".to_string()
            }
            SessionStatus::Connecting(_) => {
                "Contacting rendezvous server (131.222.194.92)... Establishing secure P2P hole-punching and relay link...".to_string()
            }
            SessionStatus::Connected(_) => {
                "Authorization granted! Establishing encrypted stream and waiting for the initial video frame...".to_string()
            }
            _ => "This session is currently inactive or closed. Click Retry to reconnect to this Resky ID.".to_string(),
        };
        canvas.draw_text_wrapped(
            msg_x + 14,
            msg_y + 14,
            msg_w - 28,
            msg_h - 24,
            &detail,
            13.0,
            false,
            txt_col,
        );
    }

    // Action Buttons: [ Retry / Unlock ] and [ Return to Home ]
    let btn1_w = 185;
    let btn2_w = 165;
    let gap = 14;
    let total_w = btn1_w + gap + btn2_w;
    let btn1_x = card_x + (card_w - total_w) / 2;
    let btn2_x = btn1_x + btn1_w + gap;
    let btn_y = card_y + 212;
    let btn_h = 42;

    canvas.draw_rounded_box(btn1_x, btn_y, btn1_w, btn_h, PK_GREEN, PK_GREEN, 8);
    let b1_label = if is_pwd {
        "Unlock & Connect"
    } else {
        "Retry Connection"
    };
    canvas.draw_text_centered(btn1_x, btn_y, btn1_w, btn_h, b1_label, 13.5, true, PK_CARD);

    canvas.draw_rounded_box(btn2_x, btn_y, btn2_w, btn_h, PK_CARD, PK_CARD_BORDER, 8);
    canvas.draw_text_centered(
        btn2_x,
        btn_y,
        btn2_w,
        btn_h,
        "Return to Home",
        13.5,
        true,
        PK_TEXT,
    );
}

pub fn handle_status_view_click(mx: i16, my: i16, win_w: u16, win_h: u16) -> bool {
    let status = crate::p2p::get_session_status();
    let w = win_w as i32;
    let h = win_h as i32;
    let tb_h = TOOLBAR_HEIGHT as i32;

    let card_w = (w - 48).clamp(400, 520);
    let card_h = 300;
    let card_x = (w - card_w) / 2;
    let card_y = (tb_h + (h - tb_h - card_h) / 2).max(tb_h + 16);

    let btn1_w = 185;
    let btn2_w = 165;
    let gap = 14;
    let total_w = btn1_w + gap + btn2_w;
    let btn1_x = card_x + (card_w - total_w) / 2;
    let btn2_x = btn1_x + btn1_w + gap;
    let btn_y = card_y + 212;
    let btn_h = 42;

    let x = mx as i32;
    let y = my as i32;

    if y >= btn_y && y <= btn_y + btn_h {
        if x >= btn1_x && x <= btn1_x + btn1_w {
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
        } else if x >= btn2_x && x <= btn2_x + btn2_w {
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
