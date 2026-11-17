//! Viewport Top Tab Bar & Responsive Interactive Toolbar (Linux X11)
//!
//! Mirrors `src/windows/ui/viewport/toolbar.rs` by rendering session tabs,
//! active codec/relay status pill, and host session banner with crisp TrueType fonts.

use super::tabs::{close_session_tab, get_session_tabs, switch_session_tab, SessionTab};
use crate::linux::ui::host_widget::{draw_host_session_banner, handle_host_banner_click};
use crate::linux::ui::theme::{
    PixelCanvas, PK_CARD, PK_CARD_BORDER, PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG,
    PK_MUTED, PK_TEXT,
};

pub const TOOLBAR_HEIGHT: u16 = 40;

pub fn draw_tab_bar(
    canvas: &mut PixelCanvas,
    win_w: u16,
    tabs: &[SessionTab],
    active_tab: usize,
) {
    let w = win_w as i32;
    let h = TOOLBAR_HEIGHT as i32;

    // Top bar white background + 1px bottom border
    canvas.fill_rect(0, 0, w, h, PK_CARD);
    canvas.fill_rect(0, h - 1, w, 1, PK_CARD_BORDER);

    // Brand logo text
    canvas.draw_text_vcenter(18, 0, 74, h, "DESKY", 15.0, true, PK_GREEN);

    let mut tab_x = 92;
    for (idx, tab) in tabs.iter().enumerate() {
        let is_active = idx == active_tab;
        let tw = if idx == 0 { 76 } else { 150 };
        let (fill, border, fg) = if is_active {
            (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
        } else {
            (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
        };

        canvas.draw_rounded_box(tab_x, 6, tw, 28, fill, border, 6);
        if idx == 0 {
            canvas.draw_text_centered(tab_x, 6, tw, 28, "Home", 13.0, true, fg);
        } else {
            canvas.draw_text_vcenter(
                tab_x + 12,
                6,
                tw - 34,
                28,
                &tab.title,
                13.0,
                is_active,
                if is_active { PK_GREEN_DARK } else { PK_TEXT },
            );
            // Close [x] button inside tab
            let close_x = tab_x + tw - 24;
            canvas.draw_text_centered(close_x, 6, 18, 28, "×", 15.0, true, PK_MUTED);
        }
        tab_x += tw + 6;
    }

    // Server online / ping badge on top-right
    let online = crate::p2p::is_server_online();
    let ping = crate::p2p::get_server_ping_ms();
    let srv_w = 148;
    let srv_x = w - srv_w - 16;
    if srv_x > tab_x {
        let (s_fill, s_border, s_fg, label) = if online {
            (
                PK_GREEN_TINT,
                PK_GREEN,
                PK_GREEN_DARK,
                if ping > 0 {
                    format!("● Online ({}ms)", ping)
                } else {
                    "● Server Online".to_string()
                },
            )
        } else {
            (
                PK_INPUT_BG,
                PK_CARD_BORDER,
                PK_MUTED,
                "○ Connecting...".to_string(),
            )
        };
        canvas.draw_rounded_box(srv_x, 6, srv_w, 28, s_fill, s_border, 14);
        canvas.draw_text_centered(srv_x, 6, srv_w, 28, &label, 12.0, true, s_fg);
    }

    // Active Host Session Banner (if someone is viewing THIS machine)
    let _ = draw_host_session_banner(canvas, srv_x, tab_x);
}

pub fn handle_toolbar_click(mx: i16, my: i16, win_w: u16) -> bool {
    if my >= TOOLBAR_HEIGHT as i16 {
        return false;
    }

    let x = mx as i32;
    let y = my as i32;
    let tabs = get_session_tabs();
    let mut tab_x = 92;
    for (idx, _) in tabs.iter().enumerate() {
        let tw = if idx == 0 { 76 } else { 150 };
        if x >= tab_x && x <= tab_x + tw && (6..=34).contains(&y) {
            if idx > 0 && x >= tab_x + tw - 26 {
                close_session_tab(idx);
            } else {
                switch_session_tab(idx);
            }
            return true;
        }
        tab_x += tw + 6;
    }

    let srv_x = (win_w as i32) - 148 - 16;
    if handle_host_banner_click(mx, my, srv_x, tab_x) {
        return true;
    }

    false
}
