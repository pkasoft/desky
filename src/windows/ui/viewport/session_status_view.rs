//! Viewport Session Status & Alert View
//! Displays modern connection state, connecting loader, authorization prompts,
//! unattended password prompts, and clear failure/offline alert cards.

#![allow(unsafe_op_in_unsafe_fn)]

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Mutex, OnceLock};
use windows::Win32::Foundation::{COLORREF, HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, DeleteObject, DrawTextW, FillRect, InvalidateRect, SelectObject, SetBkMode,
    SetTextColor, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, FW_BOLD,
    FW_SEMIBOLD, HDC, HGDIOBJ, TRANSPARENT,
};

use super::theme::{
    create_mono_font, create_segoe_font, draw_rounded_box, PK_BG, PK_BTN_HOVER, PK_CARD,
    PK_CARD_BORDER, PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_RED, PK_TEXT,
};
use crate::p2p::SessionStatus;

pub const BTN_CANCEL_CONNECTION: i32 = 9401;
pub const BTN_RETRY_CONNECTION: i32 = 9402;
pub const BTN_CLOSE_SESSION: i32 = 9403;
pub const BTN_RETURN_TO_HOME: i32 = 9404;
pub const BTN_SUBMIT_PASSWORD: i32 = 9405;

pub static STATUS_HOVER_STATE: AtomicI32 = AtomicI32::new(-1);
static PWD_INPUT_BUFFER: OnceLock<Mutex<String>> = OnceLock::new();

fn pwd_store() -> &'static Mutex<String> {
    PWD_INPUT_BUFFER.get_or_init(|| Mutex::new(String::new()))
}

pub fn get_password_input() -> String {
    pwd_store().lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn append_password_char(c: char) {
    if !c.is_control() {
        if let Ok(mut g) = pwd_store().lock() {
            g.push(c);
        }
    }
}

pub fn pop_password_char() {
    if let Ok(mut g) = pwd_store().lock() {
        g.pop();
    }
}

pub fn clear_password_input() {
    if let Ok(mut g) = pwd_store().lock() {
        g.clear();
    }
}

pub fn get_status_hover_state() -> i32 {
    STATUS_HOVER_STATE.load(Ordering::Relaxed)
}

fn compute_card_layout(client_w: i32, client_h: i32, toolbar_h: i32) -> (i32, i32, i32, i32) {
    let view_h = client_h - toolbar_h;
    let card_w = (client_w - 48).clamp(400, 520);
    let card_h = 330;
    let card_x = (client_w - card_w) / 2;
    let card_y = (toolbar_h + (view_h - card_h) / 2).max(toolbar_h + 16);
    (card_x, card_y, card_w, card_h)
}

/// Tests hit coordinates on the session status view buttons
pub fn hit_test_session_status_view(
    client_x: i32,
    client_y: i32,
    client_w: i32,
    client_h: i32,
    toolbar_h: i32,
    status: &SessionStatus,
) -> i32 {
    let (card_x, card_y, card_w, _) = compute_card_layout(client_w, client_h, toolbar_h);

    match status {
        SessionStatus::PasswordRequired(_) => {
            // [ Unlock & Connect ] and [ Cancel ]
            let btn1_w = 175;
            let btn2_w = 110;
            let btn_gap = 14;
            let total_w = btn1_w + btn_gap + btn2_w;
            let btn1_x = card_x + (card_w - total_w) / 2;
            let btn2_x = btn1_x + btn1_w + btn_gap;
            let btn_y = card_y + 236;
            let btn_h = 42;

            if client_x >= btn1_x && client_x <= btn1_x + btn1_w && client_y >= btn_y && client_y <= btn_y + btn_h {
                return BTN_SUBMIT_PASSWORD;
            }
            if client_x >= btn2_x && client_x <= btn2_x + btn2_w && client_y >= btn_y && client_y <= btn_y + btn_h {
                return BTN_CANCEL_CONNECTION;
            }

            // Return to Home link
            let link_w = 140;
            let link_h = 24;
            let link_x = card_x + (card_w - link_w) / 2;
            let link_y = card_y + 288;
            if client_x >= link_x && client_x <= link_x + link_w && client_y >= link_y && client_y <= link_y + link_h {
                return BTN_RETURN_TO_HOME;
            }
        }
        SessionStatus::Connecting(_)
        | SessionStatus::WaitingApproval(_)
        | SessionStatus::Connected(_) => {
            let btn_w = 190;
            let btn_h = 40;
            let btn_x = card_x + (card_w - btn_w) / 2;
            let btn_y = card_y + 238;

            if client_x >= btn_x && client_x <= btn_x + btn_w && client_y >= btn_y && client_y <= btn_y + btn_h {
                return BTN_CANCEL_CONNECTION;
            }

            // Return to Home link
            let link_w = 140;
            let link_h = 24;
            let link_x = card_x + (card_w - link_w) / 2;
            let link_y = card_y + 288;
            if client_x >= link_x && client_x <= link_x + link_w && client_y >= link_y && client_y <= link_y + link_h {
                return BTN_RETURN_TO_HOME;
            }
        }
        _ => {
            // Error, Idle, Disconnected
            let btn1_w = 175;
            let btn2_w = 150;
            let btn_gap = 14;
            let total_w = btn1_w + btn_gap + btn2_w;
            let btn1_x = card_x + (card_w - total_w) / 2;
            let btn2_x = btn1_x + btn1_w + btn_gap;
            let btn_y = card_y + 236;
            let btn_h = 42;

            if client_x >= btn1_x && client_x <= btn1_x + btn1_w && client_y >= btn_y && client_y <= btn_y + btn_h {
                return BTN_RETRY_CONNECTION;
            }
            if client_x >= btn2_x && client_x <= btn2_x + btn2_w && client_y >= btn_y && client_y <= btn_y + btn_h {
                return BTN_CLOSE_SESSION;
            }

            // Return to Home link
            let link_w = 140;
            let link_h = 24;
            let link_x = card_x + (card_w - link_w) / 2;
            let link_y = card_y + 288;
            if client_x >= link_x && client_x <= link_x + link_w && client_y >= link_y && client_y <= link_y + link_h {
                return BTN_RETURN_TO_HOME;
            }
        }
    }

    -1
}

pub fn handle_status_mousemove(
    hwnd: HWND,
    client_x: i32,
    client_y: i32,
    client_w: i32,
    client_h: i32,
    toolbar_h: i32,
    status: &SessionStatus,
) {
    let hit = hit_test_session_status_view(client_x, client_y, client_w, client_h, toolbar_h, status);
    let old = STATUS_HOVER_STATE.swap(hit, Ordering::Relaxed);
    if old != hit {
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

/// Renders the complete, polished session status / alert view
pub unsafe fn draw_session_status_view(
    hdc: HDC,
    client_w: i32,
    client_h: i32,
    toolbar_h: i32,
    target_id: &str,
    status: &SessionStatus,
    hover_state: i32,
) {
    // 1. Fill entire viewport canvas below toolbar with pkasoft paper background
    let view_rc = RECT {
        left: 0,
        top: toolbar_h,
        right: client_w,
        bottom: client_h,
    };
    let bg_brush = CreateSolidBrush(PK_BG);
    FillRect(hdc, &view_rc, bg_brush);
    let _ = DeleteObject(HGDIOBJ(bg_brush.0));

    // 2. Compute card bounding box
    let (card_x, card_y, card_w, card_h) = compute_card_layout(client_w, client_h, toolbar_h);

    // Draw main white card box
    draw_rounded_box(hdc, card_x, card_y, card_w, card_h, PK_CARD, PK_CARD_BORDER, 8);

    // 3. Status accent strip & theme determination
    let (accent_color, is_err, is_waiting, is_pwd) = match status {
        SessionStatus::Error(_) => (PK_RED, true, false, false),
        SessionStatus::WaitingApproval(_) => (COLORREF(0x001B85D9), false, true, false),
        SessionStatus::PasswordRequired(_) => (COLORREF(0x001B85D9), false, false, true),
        _ => (PK_GREEN, false, false, false),
    };

    let strip_rc = RECT {
        left: card_x + 1,
        top: card_y + 1,
        right: card_x + card_w - 1,
        bottom: card_y + 5,
    };
    let strip_brush = CreateSolidBrush(accent_color);
    FillRect(hdc, &strip_rc, strip_brush);
    let _ = DeleteObject(HGDIOBJ(strip_brush.0));

    // 4. Vector Icon in Header
    let icon_sz = 36;
    let icon_x = card_x + (card_w - icon_sz) / 2;
    let icon_y = card_y + 22;

    if is_err {
        crate::ui::draw_alert_triangle(hdc, icon_x, icon_y, icon_sz, PK_RED, 2);
    } else if is_waiting {
        crate::ui::draw_bell(hdc, icon_x, icon_y, icon_sz, COLORREF(0x001B85D9), 2);
    } else if is_pwd {
        crate::ui::draw_lock(hdc, icon_x, icon_y, icon_sz, COLORREF(0x001B85D9), 2);
    } else {
        match status {
            SessionStatus::Connecting(_) => {
                crate::ui::draw_refresh(hdc, icon_x, icon_y, icon_sz, PK_GREEN, 2);
            }
            _ => {
                crate::ui::draw_info(hdc, icon_x, icon_y, icon_sz, PK_MUTED, 2);
            }
        }
    }

    // 5. Title Text
    let title_font = create_segoe_font(17, FW_BOLD);
    let old_f = SelectObject(hdc, HGDIOBJ(title_font.0));
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, if is_err { PK_RED } else { PK_TEXT });

    let title_str = match status {
        SessionStatus::Error(_) => "Connection Failed",
        SessionStatus::WaitingApproval(_) => "Waiting for Host Authorization",
        SessionStatus::PasswordRequired(_) => "Unattended Access",
        SessionStatus::Connecting(_) => "Connecting to Remote Desk",
        SessionStatus::Connected(_) => "Authorized — Starting Video Stream",
        _ => "Session Offline",
    };
    let mut title_utf16: Vec<u16> = title_str.encode_utf16().collect();
    let mut title_rc = RECT {
        left: card_x + 16,
        top: card_y + 68,
        right: card_x + card_w - 16,
        bottom: card_y + 92,
    };
    let _ = DrawTextW(hdc, &mut title_utf16, &mut title_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_NOPREFIX);

    // 6. Target ID Badge Pill
    let pill_w = 170;
    let pill_h = 28;
    let pill_x = card_x + (card_w - pill_w) / 2;
    let pill_y = card_y + 100;

    let pill_bg = if is_err {
        COLORREF(0x00F0F2FD) // Soft light-red tint
    } else {
        PK_GREEN_TINT
    };
    let pill_border = if is_err {
        COLORREF(0x00D0D5FA)
    } else {
        PK_CARD_BORDER
    };
    draw_rounded_box(hdc, pill_x, pill_y, pill_w, pill_h, pill_bg, pill_border, 6);

    let id_font = create_mono_font(13, FW_BOLD);
    let _ = SelectObject(hdc, HGDIOBJ(id_font.0));
    SetTextColor(hdc, if is_err { PK_RED } else { PK_GREEN_DARK });

    let display_id = if target_id.chars().all(|c| c.is_ascii_digit()) && target_id.len() == 9 {
        format!("{} {} {}", &target_id[0..3], &target_id[3..6], &target_id[6..9])
    } else {
        target_id.to_string()
    };
    let mut id_utf16: Vec<u16> = display_id.encode_utf16().collect();
    let mut id_rc = RECT {
        left: pill_x,
        top: pill_y,
        right: pill_x + pill_w,
        bottom: pill_y + pill_h,
    };
    let _ = DrawTextW(hdc, &mut id_utf16, &mut id_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_NOPREFIX);

    // 7. Message Box / Password Input Box
    let msg_box_w = card_w - 48;
    let msg_box_h = 74;
    let msg_box_x = card_x + 24;
    let msg_box_y = card_y + 142;

    if is_pwd {
        let (box_bg, box_border) = (PK_INPUT_BG, PK_GREEN);
        draw_rounded_box(hdc, msg_box_x, msg_box_y, msg_box_w, msg_box_h, box_bg, box_border, 6);

        let desc_font = create_segoe_font(10, FW_SEMIBOLD);
        let _ = SelectObject(hdc, HGDIOBJ(desc_font.0));
        SetTextColor(hdc, PK_MUTED);
        let mut prompt_txt: Vec<u16> = "Enter remote master password (type on keyboard):".encode_utf16().collect();
        let mut prompt_rc = RECT {
            left: msg_box_x + 12,
            top: msg_box_y + 8,
            right: msg_box_x + msg_box_w - 12,
            bottom: msg_box_y + 26,
        };
        let _ = DrawTextW(hdc, &mut prompt_txt, &mut prompt_rc, DT_SINGLELINE | DT_CENTER | DT_NOPREFIX);

        // Masked bullet characters
        let cur_pwd = get_password_input();
        let masked = if cur_pwd.is_empty() {
            "Type password here...".to_string()
        } else {
            "• ".repeat(cur_pwd.len())
        };

        let pwd_font = create_mono_font(15, FW_BOLD);
        let _ = SelectObject(hdc, HGDIOBJ(pwd_font.0));
        SetTextColor(hdc, if cur_pwd.is_empty() { PK_MUTED } else { PK_TEXT });
        let mut pwd_txt: Vec<u16> = masked.encode_utf16().collect();
        let mut pwd_rc = RECT {
            left: msg_box_x + 12,
            top: msg_box_y + 32,
            right: msg_box_x + msg_box_w - 12,
            bottom: msg_box_y + 64,
        };
        let _ = DrawTextW(hdc, &mut pwd_txt, &mut pwd_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_NOPREFIX);
        let _ = DeleteObject(HGDIOBJ(desc_font.0));
        let _ = DeleteObject(HGDIOBJ(pwd_font.0));
    } else {
        let (box_bg, box_border, text_color) = if is_err {
            (COLORREF(0x00F9FAFE), COLORREF(0x00DCE0FA), PK_RED)
        } else {
            (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
        };
        draw_rounded_box(hdc, msg_box_x, msg_box_y, msg_box_w, msg_box_h, box_bg, box_border, 6);

        let desc_font = create_segoe_font(11, FW_SEMIBOLD);
        let _ = SelectObject(hdc, HGDIOBJ(desc_font.0));
        SetTextColor(hdc, text_color);

        let message_text = match status {
            SessionStatus::Error(err) => err.clone(),
            SessionStatus::WaitingApproval(_) => {
                "An authorization prompt was displayed on the remote computer's screen. Waiting for the operator to accept your request...".to_string()
            }
            SessionStatus::Connecting(_) => {
                "Contacting rendezvous server...\nEstablishing secure P2P hole-punching and relay fallback link...".to_string()
            }
            SessionStatus::Connected(_) => {
                "Authorization granted! Establishing encrypted stream and waiting for the initial video frame...".to_string()
            }
            _ => {
                "This session is currently inactive or closed. Click Retry to reconnect to this Resky ID.".to_string()
            }
        };

        let mut msg_utf16: Vec<u16> = message_text.encode_utf16().collect();
        let mut msg_rc = RECT {
            left: msg_box_x + 12,
            top: msg_box_y + 8,
            right: msg_box_x + msg_box_w - 12,
            bottom: msg_box_y + msg_box_h - 8,
        };
        let _ = DrawTextW(hdc, &mut msg_utf16, &mut msg_rc, DT_WORDBREAK | DT_CENTER | DT_VCENTER | DT_NOPREFIX);
        let _ = DeleteObject(HGDIOBJ(desc_font.0));
    }

    // 8. Action Buttons
    let btn_font = create_segoe_font(12, FW_SEMIBOLD);
    let _ = SelectObject(hdc, HGDIOBJ(btn_font.0));

    match status {
        SessionStatus::PasswordRequired(_) => {
            // [ Unlock & Connect ] and [ Cancel ]
            let btn1_w = 175;
            let btn2_w = 110;
            let btn_gap = 14;
            let total_w = btn1_w + btn_gap + btn2_w;
            let btn1_x = card_x + (card_w - total_w) / 2;
            let btn2_x = btn1_x + btn1_w + btn_gap;
            let btn_y = card_y + 236;
            let btn_h = 42;

            // Button 1: Submit Password
            let is_b1_hover = hover_state == BTN_SUBMIT_PASSWORD;
            let b1_bg = if is_b1_hover { PK_GREEN_DARK } else { PK_GREEN };
            draw_rounded_box(hdc, btn1_x, btn_y, btn1_w, btn_h, b1_bg, b1_bg, 6);

            crate::ui::draw_lock(hdc, btn1_x + 14, btn_y + (btn_h - 16) / 2, 16, COLORREF(0x00FFFFFF), 2);

            SetTextColor(hdc, COLORREF(0x00FFFFFF));
            let mut submit_txt: Vec<u16> = "Unlock & Connect".encode_utf16().collect();
            let mut submit_rc = RECT {
                left: btn1_x + 36,
                top: btn_y,
                right: btn1_x + btn1_w - 8,
                bottom: btn_y + btn_h,
            };
            let _ = DrawTextW(hdc, &mut submit_txt, &mut submit_rc, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);

            // Button 2: Cancel
            let is_b2_hover = hover_state == BTN_CANCEL_CONNECTION;
            let b2_bg = if is_b2_hover { PK_BTN_HOVER } else { PK_CARD };
            draw_rounded_box(hdc, btn2_x, btn_y, btn2_w, btn_h, b2_bg, PK_CARD_BORDER, 6);

            SetTextColor(hdc, PK_TEXT);
            let mut cancel_txt: Vec<u16> = "Cancel".encode_utf16().collect();
            let mut cancel_rc = RECT {
                left: btn2_x,
                top: btn_y,
                right: btn2_x + btn2_w,
                bottom: btn_y + btn_h,
            };
            let _ = DrawTextW(hdc, &mut cancel_txt, &mut cancel_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_NOPREFIX);
        }
        SessionStatus::Connecting(_)
        | SessionStatus::WaitingApproval(_)
        | SessionStatus::Connected(_) => {
            // Cancel Connection Button
            let btn_w = 190;
            let btn_h = 40;
            let btn_x = card_x + (card_w - btn_w) / 2;
            let btn_y = card_y + 238;
            let is_hover = hover_state == BTN_CANCEL_CONNECTION;

            let cancel_bg = if is_hover { PK_BTN_HOVER } else { PK_CARD };
            draw_rounded_box(hdc, btn_x, btn_y, btn_w, btn_h, cancel_bg, PK_CARD_BORDER, 6);

            crate::ui::draw_x(hdc, btn_x + 16, btn_y + (btn_h - 14) / 2, 14, PK_TEXT, 2);

            SetTextColor(hdc, PK_TEXT);
            let mut cancel_txt: Vec<u16> = "Cancel Connection".encode_utf16().collect();
            let mut cancel_rc = RECT {
                left: btn_x + 36,
                top: btn_y,
                right: btn_x + btn_w - 12,
                bottom: btn_y + btn_h,
            };
            let _ = DrawTextW(hdc, &mut cancel_txt, &mut cancel_rc, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
        }
        _ => {
            // Error / Offline: [ Retry Connection ] and [ Close Session ]
            let btn1_w = 175;
            let btn2_w = 150;
            let btn_gap = 14;
            let total_w = btn1_w + btn_gap + btn2_w;
            let btn1_x = card_x + (card_w - total_w) / 2;
            let btn2_x = btn1_x + btn1_w + btn_gap;
            let btn_y = card_y + 236;
            let btn_h = 42;

            // Button 1: Retry Connection (Primary Green Button)
            let is_b1_hover = hover_state == BTN_RETRY_CONNECTION;
            let b1_bg = if is_b1_hover { PK_GREEN_DARK } else { PK_GREEN };
            draw_rounded_box(hdc, btn1_x, btn_y, btn1_w, btn_h, b1_bg, b1_bg, 6);

            crate::ui::draw_refresh(hdc, btn1_x + 14, btn_y + (btn_h - 16) / 2, 16, COLORREF(0x00FFFFFF), 2);

            SetTextColor(hdc, COLORREF(0x00FFFFFF));
            let mut retry_txt: Vec<u16> = "Retry Connection".encode_utf16().collect();
            let mut retry_rc = RECT {
                left: btn1_x + 36,
                top: btn_y,
                right: btn1_x + btn1_w - 8,
                bottom: btn_y + btn_h,
            };
            let _ = DrawTextW(hdc, &mut retry_txt, &mut retry_rc, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);

            // Button 2: Close Session (Secondary Outline Button)
            let is_b2_hover = hover_state == BTN_CLOSE_SESSION;
            let b2_bg = if is_b2_hover { PK_BTN_HOVER } else { PK_CARD };
            draw_rounded_box(hdc, btn2_x, btn_y, btn2_w, btn_h, b2_bg, PK_CARD_BORDER, 6);

            crate::ui::draw_x(hdc, btn2_x + 14, btn_y + (btn_h - 14) / 2, 14, PK_MUTED, 2);

            SetTextColor(hdc, PK_TEXT);
            let mut close_txt: Vec<u16> = "Close Session".encode_utf16().collect();
            let mut close_rc = RECT {
                left: btn2_x + 34,
                top: btn_y,
                right: btn2_x + btn2_w - 8,
                bottom: btn_y + btn_h,
            };
            let _ = DrawTextW(hdc, &mut close_txt, &mut close_rc, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
        }
    }

    // 9. Return to Home Ghost Link
    let link_w = 140;
    let link_h = 24;
    let link_x = card_x + (card_w - link_w) / 2;
    let link_y = card_y + 288;
    let is_link_hover = hover_state == BTN_RETURN_TO_HOME;

    let link_font = create_segoe_font(11, FW_SEMIBOLD);
    let _ = SelectObject(hdc, HGDIOBJ(link_font.0));
    SetTextColor(hdc, if is_link_hover { PK_GREEN_DARK } else { PK_MUTED });

    crate::ui::draw_home(hdc, link_x + 4, link_y + (link_h - 13) / 2, 13, if is_link_hover { PK_GREEN_DARK } else { PK_MUTED }, 2);

    let mut home_txt: Vec<u16> = "Return to Home".encode_utf16().collect();
    let mut home_rc = RECT {
        left: link_x + 22,
        top: link_y,
        right: link_x + link_w,
        bottom: link_y + link_h,
    };
    let _ = DrawTextW(hdc, &mut home_txt, &mut home_rc, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);

    // Cleanup fonts
    let _ = SelectObject(hdc, old_f);
    let _ = DeleteObject(HGDIOBJ(title_font.0));
    let _ = DeleteObject(HGDIOBJ(id_font.0));
    let _ = DeleteObject(HGDIOBJ(btn_font.0));
    let _ = DeleteObject(HGDIOBJ(link_font.0));
}
