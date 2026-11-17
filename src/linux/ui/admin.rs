//! Remote Administration View & Process/System Management (Linux X11)
//!
//! Mirrors `src/windows/ui/admin.rs` for remote system administration.

#![allow(dead_code)]

use super::theme::{PixelCanvas, PK_CARD, PK_CARD_BORDER, PK_MUTED, PK_TEXT};

pub fn draw_admin_view(canvas: &mut PixelCanvas, win_w: u16, win_h: u16) {
    let x = 24;
    let y = 56;
    let w = (win_w as i32 - 48).max(100);
    let h = (win_h as i32 - 80).max(100);

    canvas.draw_rounded_box(x, y, w, h, PK_CARD, PK_CARD_BORDER, 10);
    canvas.draw_text(
        x + 22,
        y + 20,
        "Remote System Administration",
        18.0,
        true,
        PK_TEXT,
        Some(w - 44),
    );
    canvas.draw_text(
        x + 22,
        y + 50,
        "Manage remote processes, system telemetry, and power controls.",
        13.5,
        false,
        PK_MUTED,
        Some(w - 44),
    );
}
