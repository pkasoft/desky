//! Active Host Session Banner & Control Widget (Linux X11)
//!
//! Mirrors `src/windows/ui/host_widget.rs` for monitoring active incoming
//! host sessions and disconnecting connected remote clients.

#![allow(dead_code)]

use super::launcher::{append_status_log, request_main_redraw};
use super::theme::{PixelCanvas, PK_RED, PK_RED_TINT};

pub fn sync_host_session_ui() {
    request_main_redraw();
}

pub fn draw_host_session_banner(
    canvas: &mut PixelCanvas,
    right_edge_x: i32,
    min_left_x: i32,
) -> Option<(i32, i32)> {
    if let Some((client_id, secs)) = crate::p2p::get_active_host_session_info() {
        let banner_w = 270;
        let banner_x = (right_edge_x - banner_w - 8).max(min_left_x);
        if banner_x + banner_w <= right_edge_x {
            canvas.draw_rounded_box(banner_x, 6, banner_w, 28, PK_RED_TINT, PK_RED, 6);
            let msg = format!("Hosting: {} ({}s)  [Disconnect]", client_id, secs);
            canvas.draw_text_centered(banner_x, 6, banner_w, 28, &msg, 12.5, true, PK_RED);
            return Some((banner_x, banner_w));
        }
    }
    None
}

pub fn handle_host_banner_click(mx: i16, my: i16, right_edge_x: i32, min_left_x: i32) -> bool {
    if crate::p2p::get_active_host_session_info().is_some() {
        let banner_w = 270;
        let banner_x = (right_edge_x - banner_w - 8).max(min_left_x);
        let x = mx as i32;
        let y = my as i32;
        if x >= banner_x && x <= banner_x + banner_w && (6..=34).contains(&y) {
            crate::p2p::disconnect_host_session();
            append_status_log("Disconnected active remote client.");
            return true;
        }
    }
    false
}
