//! In-Session Floating Chat HUD
//! Provides a slide-out, real-time messaging panel inside the remote desktop viewport.

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
    create_segoe_font, draw_rounded_box, PK_BG, PK_BTN_HOVER, PK_CARD, PK_CARD_BORDER, PK_GREEN,
    PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_TEXT,
};

pub const BTN_CHAT_CLOSE: i32 = 9501;
pub const BTN_CHAT_CLEAR: i32 = 9502;
pub const BTN_CHAT_SEND: i32 = 9503;
pub const AREA_CHAT_INPUT: i32 = 9504;

pub const CHAT_WIDTH: i32 = 310;

static CHAT_INPUT_BUFFER: OnceLock<Mutex<String>> = OnceLock::new();
static CHAT_HOVER_STATE: AtomicI32 = AtomicI32::new(-1);

fn input_buf() -> &'static Mutex<String> {
    CHAT_INPUT_BUFFER.get_or_init(|| Mutex::new(String::new()))
}

pub fn get_chat_input() -> String {
    input_buf().lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn append_chat_char(c: char) {
    if let Ok(mut g) = input_buf().lock() {
        if !c.is_control() {
            g.push(c);
        }
    }
}

pub fn pop_chat_char() {
    if let Ok(mut g) = input_buf().lock() {
        g.pop();
    }
}

pub fn clear_chat_input() {
    if let Ok(mut g) = input_buf().lock() {
        g.clear();
    }
}

pub fn get_chat_hover_state() -> i32 {
    CHAT_HOVER_STATE.load(Ordering::Relaxed)
}

pub fn submit_chat_input() {
    let input = get_chat_input();
    let trimmed = input.trim();
    if !trimmed.is_empty() {
        crate::p2p::send_chat_message(trimmed);
        clear_chat_input();
    }
}

pub fn hit_test_chat_hud(
    client_x: i32,
    client_y: i32,
    client_w: i32,
    client_h: i32,
    toolbar_h: i32,
) -> i32 {
    if !crate::p2p::is_chat_open() {
        return -1;
    }
    let panel_x = client_w - CHAT_WIDTH;
    if client_x < panel_x || client_y < toolbar_h {
        return -1;
    }

    // Header buttons (y: toolbar_h + 8 .. toolbar_h + 36)
    let header_y = toolbar_h + 8;
    if client_y >= header_y && client_y <= header_y + 28 {
        let close_x = client_w - 34;
        if client_x >= close_x && client_x <= close_x + 26 {
            return BTN_CHAT_CLOSE;
        }
        let clear_x = close_x - 32;
        if client_x >= clear_x && client_x <= clear_x + 26 {
            return BTN_CHAT_CLEAR;
        }
    }

    // Bottom input area
    let bottom_y = client_h - 48;
    if client_y >= bottom_y && client_y <= client_h - 8 {
        let send_btn_x = client_w - 44;
        if client_x >= send_btn_x && client_x <= client_w - 10 {
            return BTN_CHAT_SEND;
        }
        return AREA_CHAT_INPUT;
    }

    9500 // Generic inside chat click
}

pub fn is_mouse_over_chat_hud(
    client_x: i32,
    client_y: i32,
    client_w: i32,
    client_h: i32,
    toolbar_h: i32,
) -> bool {
    hit_test_chat_hud(client_x, client_y, client_w, client_h, toolbar_h) >= 0
}

pub fn handle_chat_mousemove(
    hwnd: HWND,
    client_x: i32,
    client_y: i32,
    client_w: i32,
    client_h: i32,
    toolbar_h: i32,
) {
    let hit = hit_test_chat_hud(client_x, client_y, client_w, client_h, toolbar_h);
    let old = CHAT_HOVER_STATE.swap(hit, Ordering::Relaxed);
    if old != hit {
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

pub unsafe fn draw_chat_hud(
    hdc: HDC,
    client_w: i32,
    client_h: i32,
    toolbar_h: i32,
    hover_state: i32,
) {
    if !crate::p2p::is_chat_open() {
        return;
    }

    let panel_x = client_w - CHAT_WIDTH;
    let panel_y = toolbar_h;
    let panel_w = CHAT_WIDTH;
    let panel_h = client_h - toolbar_h;

    // Main background card
    draw_rounded_box(hdc, panel_x, panel_y, panel_w, panel_h, PK_CARD, PK_CARD_BORDER, 0);

    // Left dividing border line
    let border_brush = CreateSolidBrush(PK_CARD_BORDER);
    let div_rc = RECT {
        left: panel_x,
        top: panel_y,
        right: panel_x + 1,
        bottom: client_h,
    };
    FillRect(hdc, &div_rc, border_brush);
    let _ = DeleteObject(HGDIOBJ(border_brush.0));

    // Header area (height 44px)
    let header_rc = RECT {
        left: panel_x + 1,
        top: panel_y,
        right: client_w,
        bottom: panel_y + 44,
    };
    let head_bg = CreateSolidBrush(PK_BG);
    FillRect(hdc, &header_rc, head_bg);
    let _ = DeleteObject(HGDIOBJ(head_bg.0));

    // Header bottom border
    let head_border = CreateSolidBrush(PK_CARD_BORDER);
    let h_line = RECT {
        left: panel_x,
        top: panel_y + 43,
        right: client_w,
        bottom: panel_y + 44,
    };
    FillRect(hdc, &h_line, head_border);
    let _ = DeleteObject(HGDIOBJ(head_border.0));

    // Header Icon & Title
    crate::ui::draw_message_square(hdc, panel_x + 14, panel_y + 13, 18, PK_GREEN_DARK, 2);

    let title_font = create_segoe_font(13, FW_BOLD);
    let old_f = SelectObject(hdc, HGDIOBJ(title_font.0));
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, PK_TEXT);

    let mut title_txt: Vec<u16> = "Session Chat".encode_utf16().collect();
    let mut title_rc = RECT {
        left: panel_x + 38,
        top: panel_y,
        right: panel_x + 180,
        bottom: panel_y + 44,
    };
    let _ = DrawTextW(hdc, &mut title_txt, &mut title_rc, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);

    // Header buttons: Clear History (Trash) and Close (X)
    let close_x = client_w - 34;
    let is_close_hover = hover_state == BTN_CHAT_CLOSE;
    draw_rounded_box(
        hdc,
        close_x,
        panel_y + 8,
        26,
        26,
        if is_close_hover { PK_BTN_HOVER } else { PK_CARD },
        PK_CARD_BORDER,
        4,
    );
    crate::ui::draw_x(hdc, close_x + 7, panel_y + 15, 12, PK_MUTED, 2);

    let clear_x = close_x - 30;
    let is_clear_hover = hover_state == BTN_CHAT_CLEAR;
    draw_rounded_box(
        hdc,
        clear_x,
        panel_y + 8,
        26,
        26,
        if is_clear_hover { PK_BTN_HOVER } else { PK_CARD },
        PK_CARD_BORDER,
        4,
    );
    crate::ui::draw_trash(hdc, clear_x + 7, panel_y + 15, 12, PK_MUTED, 2);

    // Messages scrollable container
    let msgs = crate::p2p::get_chat_messages();
    let msg_area_top = panel_y + 50;
    let msg_area_bot = client_h - 52;
    let mut cur_y = msg_area_bot;

    let msg_font = create_segoe_font(11, FW_SEMIBOLD);
    let time_font = create_segoe_font(9, FW_SEMIBOLD);

    // Render messages from bottom to top
    for msg in msgs.iter().rev() {
        if cur_y < msg_area_top + 20 {
            break;
        }

        let max_bubble_w = 210;
        let mut text_utf16: Vec<u16> = msg.text.encode_utf16().collect();
        let _ = SelectObject(hdc, HGDIOBJ(msg_font.0));
        let mut calc_rc = RECT {
            left: 0,
            top: 0,
            right: max_bubble_w - 16,
            bottom: 0,
        };
        let txt_h = DrawTextW(
            hdc,
            &mut text_utf16,
            &mut calc_rc,
            windows::Win32::Graphics::Gdi::DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX,
        );

        let bubble_w = (calc_rc.right - calc_rc.left + 20).clamp(60, max_bubble_w);
        let bubble_h = (txt_h + 14).max(30);

        cur_y -= bubble_h + 8;
        if cur_y < msg_area_top {
            break;
        }

        let bubble_x = if msg.is_me {
            client_w - bubble_w - 14
        } else {
            panel_x + 14
        };

        let (fill_c, border_c, text_c) = if msg.is_me {
            (PK_GREEN_TINT, PK_CARD_BORDER, PK_GREEN_DARK)
        } else {
            (PK_INPUT_BG, PK_CARD_BORDER, PK_TEXT)
        };

        draw_rounded_box(hdc, bubble_x, cur_y, bubble_w, bubble_h, fill_c, border_c, 6);

        let _ = SelectObject(hdc, HGDIOBJ(msg_font.0));
        SetTextColor(hdc, text_c);
        let mut text_rc = RECT {
            left: bubble_x + 10,
            top: cur_y + 6,
            right: bubble_x + bubble_w - 10,
            bottom: cur_y + bubble_h - 4,
        };
        let _ = DrawTextW(hdc, &mut text_utf16, &mut text_rc, DT_WORDBREAK | DT_NOPREFIX);

        // Timestamp & sender
        let _ = SelectObject(hdc, HGDIOBJ(time_font.0));
        SetTextColor(hdc, PK_MUTED);
        let time_label = if msg.is_me {
            msg.timestamp_str.clone()
        } else {
            format!("{} • {}", msg.sender, msg.timestamp_str)
        };
        let mut time_utf16: Vec<u16> = time_label.encode_utf16().collect();
        let mut time_rc = RECT {
            left: bubble_x,
            top: cur_y + bubble_h + 1,
            right: bubble_x + bubble_w,
            bottom: cur_y + bubble_h + 12,
        };
        let _ = DrawTextW(
            hdc,
            &mut time_utf16,
            &mut time_rc,
            DT_SINGLELINE | if msg.is_me { windows::Win32::Graphics::Gdi::DT_RIGHT } else { windows::Win32::Graphics::Gdi::DT_LEFT } | DT_NOPREFIX,
        );
        cur_y -= 8;
    }

    if msgs.is_empty() {
        let _ = SelectObject(hdc, HGDIOBJ(msg_font.0));
        SetTextColor(hdc, PK_MUTED);
        let mut empty_txt: Vec<u16> = "No messages yet.\nType below to chat!".encode_utf16().collect();
        let mut empty_rc = RECT {
            left: panel_x + 20,
            top: panel_y + 120,
            right: client_w - 20,
            bottom: panel_y + 180,
        };
        let _ = DrawTextW(hdc, &mut empty_txt, &mut empty_rc, DT_CENTER | DT_WORDBREAK | DT_NOPREFIX);
    }

    // Bottom Input Bar
    let input_area_y = client_h - 48;
    draw_rounded_box(
        hdc,
        panel_x + 12,
        input_area_y,
        panel_w - 56,
        36,
        PK_INPUT_BG,
        PK_CARD_BORDER,
        6,
    );

    let cur_input = get_chat_input();
    let display_input = if cur_input.is_empty() {
        "Type message..."
    } else {
        &cur_input
    };

    let _ = SelectObject(hdc, HGDIOBJ(msg_font.0));
    SetTextColor(hdc, if cur_input.is_empty() { PK_MUTED } else { PK_TEXT });

    let mut in_txt: Vec<u16> = display_input.encode_utf16().collect();
    let mut in_rc = RECT {
        left: panel_x + 22,
        top: input_area_y,
        right: panel_x + panel_w - 60,
        bottom: input_area_y + 36,
    };
    let _ = DrawTextW(hdc, &mut in_txt, &mut in_rc, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);

    // Send Button
    let send_x = client_w - 40;
    let is_send_hover = hover_state == BTN_CHAT_SEND;
    let send_bg = if is_send_hover { PK_GREEN_DARK } else { PK_GREEN };
    draw_rounded_box(hdc, send_x, input_area_y, 30, 36, send_bg, send_bg, 6);

    crate::ui::draw_arrow_right(hdc, send_x + 8, input_area_y + 10, 14, COLORREF(0x00FFFFFF), 2);

    let _ = SelectObject(hdc, old_f);
    let _ = DeleteObject(HGDIOBJ(title_font.0));
    let _ = DeleteObject(HGDIOBJ(msg_font.0));
    let _ = DeleteObject(HGDIOBJ(time_font.0));
}
