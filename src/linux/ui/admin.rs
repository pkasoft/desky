//! Remote Administration View & Process/System Management (Linux X11)
//!
//! Mirrors `src/windows/ui/admin.rs` for remote system administration.

#![allow(dead_code)]

use x11rb::connection::Connection;
use x11rb::protocol::xproto::Drawable;

use super::theme::{draw_str, fill_bordered_box, X11Palette};

pub fn draw_admin_view<C: Connection>(
    conn: &C,
    canvas: Drawable,
    win_w: u16,
    win_h: u16,
    pal: &X11Palette,
) {
    let x = 24i16;
    let y = 56i16;
    let w = win_w.saturating_sub(48);
    let h = win_h.saturating_sub(80);

    fill_bordered_box(conn, canvas, x, y, w, h, pal.gc_card, pal.gc_border);
    draw_str(
        conn,
        canvas,
        pal.txt_large_dark_on_card,
        x + 20,
        y + 36,
        "Remote System Administration",
    );
    draw_str(
        conn,
        canvas,
        pal.txt_muted_on_card,
        x + 20,
        y + 64,
        "Manage remote processes, system telemetry, and power controls.",
    );
}
