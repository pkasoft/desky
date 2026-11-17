//! File Transfer In-Viewport HUD Overlay (Linux X11)
//!
//! Mirrors `src/windows/ui/viewport/file_hud.rs`.

#![allow(dead_code)]

use crate::linux::ui::theme::{
    PixelCanvas, PK_CARD, PK_GREEN, PK_GREEN_DARK, PK_INPUT_BG, PK_TEXT,
};

pub fn draw_file_transfer_hud(canvas: &mut PixelCanvas, width: i32, height: i32) {
    let prog = crate::p2p::get_transfer_progress();
    if prog.total_bytes == 0 {
        return;
    }

    let hud_w = 400.min(width - 40);
    let hud_h = 52;
    let hud_x = (width - hud_w) / 2;
    let hud_y = height - hud_h - 16;

    canvas.draw_rounded_box(hud_x, hud_y, hud_w, hud_h, PK_CARD, PK_GREEN, 8);

    let prefix = if prog.is_complete {
        "Transferred: "
    } else if prog.is_sending {
        "Uploading: "
    } else {
        "Receiving: "
    };
    let full_name = format!("{}{}", prefix, prog.file_name);
    canvas.draw_text(
        hud_x + 16,
        hud_y + 8,
        &full_name,
        12.5,
        true,
        PK_TEXT,
        Some(hud_w - 80),
    );

    let pct_str = format!("{}%", prog.percentage);
    canvas.draw_text(
        hud_x + hud_w - 52,
        hud_y + 8,
        &pct_str,
        12.5,
        true,
        PK_GREEN_DARK,
        Some(44),
    );

    let bar_x = hud_x + 16;
    let bar_y = hud_y + 34;
    let bar_w = hud_w - 32;
    let bar_h = 6;
    canvas.fill_rect(bar_x, bar_y, bar_w, bar_h, PK_INPUT_BG);

    let fill_w =
        ((bar_w as f64) * (prog.percentage as f64 / 100.0)).clamp(0.0, bar_w as f64) as i32;
    if fill_w > 0 {
        canvas.fill_rect(bar_x, bar_y, fill_w, bar_h, PK_GREEN);
    }
}
