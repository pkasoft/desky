//! Viewport Top Tab Bar & Responsive Interactive Toolbar (Linux X11)
//!
//! Mirrors `src/windows/ui/viewport/toolbar.rs` by rendering session tabs,
//! active codec/relay status pill, and host session banner.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::Drawable;

use super::tabs::{close_session_tab, get_session_tabs, switch_session_tab, SessionTab};
use crate::linux::ui::host_widget::{draw_host_session_banner, handle_host_banner_click};
use crate::linux::ui::theme::{draw_str, fill_bordered_box, X11Palette};

pub const TOOLBAR_HEIGHT: u16 = 42;

pub fn draw_tab_bar<C: Connection>(
    conn: &C,
    canvas: Drawable,
    win_w: u16,
    tabs: &[SessionTab],
    active_tab: usize,
    pal: &X11Palette,
) {
    fill_bordered_box(
        conn,
        canvas,
        0,
        0,
        win_w,
        TOOLBAR_HEIGHT,
        pal.gc_card,
        pal.gc_border,
    );
    draw_str(conn, canvas, pal.txt_green_on_card, 18, 26, "DESKY");

    let mut tab_x = 96i16;
    for (idx, tab) in tabs.iter().enumerate() {
        let is_active = idx == active_tab;
        let tw = if idx == 0 { 82u16 } else { 154u16 };
        let fill = if is_active {
            pal.gc_green_tint
        } else {
            pal.gc_input_bg
        };
        let border = if is_active {
            pal.gc_green
        } else {
            pal.gc_border
        };
        let txt_gc = if is_active {
            pal.txt_green_on_tint
        } else {
            pal.txt_muted_on_input
        };

        fill_bordered_box(conn, canvas, tab_x, 7, tw, 28, fill, border);
        let label = if idx == 0 {
            "Home".to_string()
        } else {
            format!("{}  [x]", tab.title)
        };
        draw_str(conn, canvas, txt_gc, tab_x + 12, 26, &label);
        tab_x += tw as i16 + 6;
    }

    // Server online / ping badge on top-right
    let online = crate::p2p::is_server_online();
    let ping = crate::p2p::get_server_ping_ms();
    let srv_w = 166u16;
    let srv_x = (win_w as i16) - (srv_w as i16) - 16;
    if srv_x > tab_x {
        let (s_fill, s_border, s_txt, label) = if online {
            (
                pal.gc_green_tint,
                pal.gc_green,
                pal.txt_green_on_tint,
                if ping > 0 {
                    format!("* Relay Online ({}ms)", ping)
                } else {
                    "* Relay Online".to_string()
                },
            )
        } else {
            (
                pal.gc_input_bg,
                pal.gc_border,
                pal.txt_muted_on_input,
                "o Connecting Relay...".to_string(),
            )
        };
        fill_bordered_box(conn, canvas, srv_x, 8, srv_w, 26, s_fill, s_border);
        draw_str(conn, canvas, s_txt, srv_x + 10, 25, &label);
    }

    // Active Host Session Banner (if someone is viewing THIS machine)
    let _ = draw_host_session_banner(conn, canvas, pal, srv_x, tab_x);
}

pub fn handle_toolbar_click(mx: i16, my: i16, win_w: u16) -> bool {
    if my >= TOOLBAR_HEIGHT as i16 {
        return false;
    }

    let tabs = get_session_tabs();
    let mut tab_x = 96i16;
    for (idx, _) in tabs.iter().enumerate() {
        let tw = if idx == 0 { 82i16 } else { 154i16 };
        if mx >= tab_x && mx <= tab_x + tw && (7..=35).contains(&my) {
            if idx > 0 && mx >= tab_x + tw - 34 {
                close_session_tab(idx);
            } else {
                switch_session_tab(idx);
            }
            return true;
        }
        tab_x += tw + 6;
    }

    let srv_x = (win_w as i16) - 166 - 16;
    if handle_host_banner_click(mx, my, srv_x, tab_x) {
        return true;
    }

    false
}
