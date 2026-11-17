//! Active Host Session Banner & Control Widget (Linux X11)
//!
//! Mirrors `src/windows/ui/host_widget.rs` for monitoring active incoming
//! host sessions and disconnecting connected remote clients.

#![allow(dead_code)]

use x11rb::connection::Connection;
use x11rb::protocol::xproto::Drawable;

use super::launcher::{append_status_log, request_main_redraw};
use super::theme::{draw_str, fill_bordered_box, X11Palette};

pub fn sync_host_session_ui() {
    request_main_redraw();
}

pub fn draw_host_session_banner<C: Connection>(
    conn: &C,
    canvas: Drawable,
    pal: &X11Palette,
    right_edge_x: i16,
    min_left_x: i16,
) -> Option<(i16, u16)> {
    if let Some((client_id, secs)) = crate::p2p::get_active_host_session_info() {
        let banner_w = 340u16;
        let banner_x = (right_edge_x - banner_w as i16 - 10).max(min_left_x);
        fill_bordered_box(
            conn,
            canvas,
            banner_x,
            8,
            banner_w,
            26,
            pal.gc_red_tint,
            pal.gc_red,
        );
        let msg = format!("Hosting: {} ({}s) [Disconnect]", client_id, secs);
        draw_str(conn, canvas, pal.txt_red_on_tint, banner_x + 10, 25, &msg);
        Some((banner_x, banner_w))
    } else {
        None
    }
}

pub fn handle_host_banner_click(mx: i16, my: i16, right_edge_x: i16, min_left_x: i16) -> bool {
    if crate::p2p::get_active_host_session_info().is_some() {
        let banner_w = 340i16;
        let banner_x = (right_edge_x - banner_w - 10).max(min_left_x);
        if mx >= banner_x && mx <= banner_x + banner_w && (8..=34).contains(&my) {
            crate::p2p::disconnect_host_session();
            append_status_log("Disconnected active remote client.");
            return true;
        }
    }
    false
}
