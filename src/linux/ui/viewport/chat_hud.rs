//! In-Session Floating Chat HUD (Linux X11)
//!
//! Mirrors `src/windows/ui/viewport/chat_hud.rs` by providing a slide-out,
//! real-time messaging panel inside the remote desktop viewport.

#![allow(dead_code)]

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Mutex, OnceLock};
use x11rb::protocol::xproto::KeyButMask;

use crate::linux::ui::remote_desk_edit::x11_keycode_to_ascii;
use crate::linux::ui::theme::{
    PixelCanvas, PK_BG, PK_BTN_HOVER, PK_CARD, PK_CARD_BORDER, PK_GREEN, PK_GREEN_DARK,
    PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_TEXT,
};

pub const BTN_CHAT_CLOSE: i32 = 9501;
pub const BTN_CHAT_CLEAR: i32 = 9502;
pub const BTN_CHAT_SEND: i32 = 9503;
pub const AREA_CHAT_INPUT: i32 = 9504;

pub const CHAT_WIDTH: i32 = 310;

static CHAT_INPUT_BUFFER: OnceLock<Mutex<String>> = OnceLock::new();
static CHAT_HOVER_STATE: AtomicI32 = AtomicI32::new(-1);
pub static HUD_CHAT_SCROLL: AtomicI32 = AtomicI32::new(0);
pub static HUD_CHAT_MAX_SCROLL: AtomicI32 = AtomicI32::new(0);

pub fn scroll_chat_hud(delta: i32) {
    let step = 35;
    let cur = HUD_CHAT_SCROLL.load(Ordering::Relaxed);
    let max_s = HUD_CHAT_MAX_SCROLL.load(Ordering::Relaxed);
    let new_val = if delta > 0 {
        (cur + step).min(max_s)
    } else {
        (cur - step).max(0)
    };
    HUD_CHAT_SCROLL.store(new_val, Ordering::Relaxed);
}

pub fn reset_chat_hud_scroll() {
    HUD_CHAT_SCROLL.store(0, Ordering::Relaxed);
}

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

pub fn submit_chat_input() {
    let input = get_chat_input();
    let trimmed = input.trim();
    if !trimmed.is_empty() {
        crate::p2p::send_chat_message(trimmed); reset_chat_hud_scroll();
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

    let bottom_y = client_h - 48;
    if client_y >= bottom_y && client_y <= client_h - 8 {
        let send_btn_x = client_w - 44;
        if client_x >= send_btn_x && client_x <= client_w - 10 {
            return BTN_CHAT_SEND;
        }
        return AREA_CHAT_INPUT;
    }

    9500
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

pub fn handle_chat_click(
    client_x: i32,
    client_y: i32,
    client_w: i32,
    client_h: i32,
    toolbar_h: i32,
) -> bool {
    let hit = hit_test_chat_hud(client_x, client_y, client_w, client_h, toolbar_h);
    if hit < 0 {
        return false;
    }
    match hit {
        BTN_CHAT_CLOSE => crate::p2p::set_chat_open(false),
        BTN_CHAT_CLEAR => { crate::p2p::clear_chat_history(); reset_chat_hud_scroll(); },
        BTN_CHAT_SEND => submit_chat_input(),
        _ => {}
    }
    true
}

pub fn handle_chat_key(keycode: u8, state: KeyButMask) {
    match keycode {
        22 => pop_chat_char(),
        36 | 104 => submit_chat_input(),
        9 => crate::p2p::set_chat_open(false),
        _ => {
            let shift = state.contains(KeyButMask::SHIFT);
            if let Some(ch) = x11_keycode_to_ascii(keycode, shift) {
                append_chat_char(ch);
            }
        }
    }
}

pub fn draw_chat_hud(
    canvas: &mut PixelCanvas,
    client_w: i32,
    client_h: i32,
    toolbar_h: i32,
) {
    if !crate::p2p::is_chat_open() {
        return;
    }

    let hover_state = CHAT_HOVER_STATE.load(Ordering::Relaxed);
    let panel_x = client_w - CHAT_WIDTH;
    let panel_y = toolbar_h;
    let panel_w = CHAT_WIDTH;
    let panel_h = client_h - toolbar_h;

    canvas.draw_tb_button_box(panel_x, panel_y, panel_w, panel_h, PK_CARD, PK_CARD_BORDER, 1);
    canvas.fill_rect(panel_x + 1, panel_y, panel_w - 1, 44, PK_BG);
    canvas.fill_rect(panel_x, panel_y + 43, panel_w, 1, PK_CARD_BORDER);

    canvas.draw_icon_chat(panel_x + 14, panel_y + 13, 18, PK_GREEN_DARK);
    canvas.draw_text_vcenter(panel_x + 38, panel_y, 140, 44, "Session Chat", 13.5, true, PK_TEXT);

    let close_x = client_w - 34;
    let is_close_hover = hover_state == BTN_CHAT_CLOSE;
    canvas.draw_rounded_box(
        close_x,
        panel_y + 9,
        26,
        26,
        if is_close_hover { PK_BTN_HOVER } else { PK_CARD },
        PK_CARD_BORDER,
        4,
    );
    canvas.draw_icon_x(close_x + 7, panel_y + 16, 12, PK_MUTED);

    let clear_x = close_x - 30;
    let is_clear_hover = hover_state == BTN_CHAT_CLEAR;
    canvas.draw_rounded_box(
        clear_x,
        panel_y + 9,
        26,
        26,
        if is_clear_hover { PK_BTN_HOVER } else { PK_CARD },
        PK_CARD_BORDER,
        4,
    );
    canvas.draw_icon_trash(clear_x + 7, panel_y + 16, 12, PK_MUTED);

    let msgs = crate::p2p::get_chat_messages();
    let msg_area_top = panel_y + 50;
    let msg_area_bot = client_h - 56;
    let avail_h = (msg_area_bot - msg_area_top).max(30);

    let mut item_heights = Vec::with_capacity(msgs.len());
    let mut total_h = 0;
    for msg in msgs.iter() {
        let (tw, th) = canvas.measure_text(&msg.text, 12.0, false);
        let max_bubble_w = 210;
        let line_count = ((tw as f32 / (max_bubble_w - 20) as f32).ceil() as i32).max(1);
        let bubble_h = (line_count * th + 14).max(36);
        let block_h = bubble_h + 16;
        item_heights.push((bubble_h, block_h));
        total_h += block_h;
    }

    let max_scroll = (total_h - avail_h).max(0);
    HUD_CHAT_MAX_SCROLL.store(max_scroll, Ordering::Relaxed);
    let scroll = HUD_CHAT_SCROLL.load(Ordering::Relaxed).min(max_scroll);
    HUD_CHAT_SCROLL.store(scroll, Ordering::Relaxed);

    canvas.set_clip_rect(Some((panel_x, msg_area_top, client_w, msg_area_bot)));

    let mut cur_y = msg_area_bot + scroll;

    for (msg, (bubble_h, block_h)) in msgs.iter().zip(item_heights.iter()).rev() {
        cur_y -= block_h;
        if cur_y > msg_area_bot {
            continue;
        }
        if cur_y + block_h < msg_area_top {
            break;
        }
        let bubble_w = 210;
        let bubble_x = if msg.is_me {
            client_w - bubble_w - 14
        } else {
            panel_x + 14
        };
        let (fill_c, text_c) = if msg.is_me {
            (PK_GREEN_TINT, PK_GREEN_DARK)
        } else {
            (PK_INPUT_BG, PK_TEXT)
        };
        canvas.draw_rounded_box(bubble_x, cur_y, bubble_w, *bubble_h, fill_c, PK_CARD_BORDER, 6);
        canvas.draw_text_vcenter(
            bubble_x + 10,
            cur_y,
            bubble_w - 20,
            *bubble_h,
            &msg.text,
            12.0,
            false,
            text_c,
        );
        let time_label = if msg.is_me {
            msg.timestamp_str.clone()
        } else {
            format!("{} • {}", msg.sender, msg.timestamp_str)
        };
        canvas.draw_text(
            bubble_x + 4,
            cur_y + bubble_h + 2,
            &time_label,
            10.0,
            false,
            PK_MUTED,
            Some(bubble_w),
        );
    }

    canvas.set_clip_rect(None);

    if max_scroll > 0 {
        let sb_w = 4;
        let sb_x = client_w - 6;
        let sb_y = msg_area_top;
        let sb_h = avail_h;
        canvas.draw_rounded_box(sb_x, sb_y, sb_w, sb_h, PK_INPUT_BG, PK_CARD_BORDER, 2);
        let thumb_h = ((avail_h as f32 / total_h as f32) * sb_h as f32).clamp(20.0, sb_h as f32) as i32;
        let thumb_travel = sb_h - thumb_h;
        let thumb_y = sb_y + ((max_scroll - scroll) as f32 / max_scroll as f32 * thumb_travel as f32) as i32;
        canvas.draw_rounded_box(sb_x, thumb_y, sb_w, thumb_h, PK_MUTED, PK_MUTED, 2);
    }

    if msgs.is_empty() {
        canvas.draw_text_centered(
            panel_x + 20,
            panel_y + 120,
            panel_w - 40,
            30,
            "No messages yet. Type below to chat!",
            12.5,
            false,
            PK_MUTED,
        );
    }

    let input_area_y = client_h - 46;
    canvas.draw_rounded_box(
        panel_x + 12,
        input_area_y,
        panel_w - 56,
        34,
        PK_INPUT_BG,
        PK_CARD_BORDER,
        6,
    );
    let cur_input = get_chat_input();
    let display_input = if cur_input.is_empty() {
        "Type message...".to_string()
    } else {
        format!("{}_", cur_input)
    };
    canvas.draw_text_vcenter(
        panel_x + 22,
        input_area_y,
        panel_w - 72,
        34,
        &display_input,
        12.5,
        false,
        if cur_input.is_empty() { PK_MUTED } else { PK_TEXT },
    );

    let send_x = client_w - 40;
    canvas.draw_rounded_box(send_x, input_area_y, 30, 34, PK_GREEN, PK_GREEN, 6);
    canvas.draw_icon_arrow_right(send_x + 8, input_area_y + 10, 14, PK_CARD);
}
