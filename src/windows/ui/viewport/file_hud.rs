//! File Transfer In-Viewport HUD Overlay

#![allow(unsafe_op_in_unsafe_fn)]

use super::theme::{
    create_segoe_font, draw_rounded_box, PK_CARD, PK_GREEN, PK_GREEN_DARK, PK_INPUT_BG, PK_TEXT,
};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, DeleteObject, DrawTextW, FillRect, SelectObject, SetBkMode, SetTextColor,
    DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FW_SEMIBOLD, HDC, HGDIOBJ,
    TRANSPARENT,
};

pub unsafe fn draw_file_transfer_hud(hdc: HDC, width: i32, height: i32) {
    let prog = crate::p2p::get_transfer_progress();
    if prog.total_bytes == 0 {
        return;
    }

    let hud_w = 400.min(width - 40);
    let hud_h = 52;
    let hud_x = (width - hud_w) / 2;
    let hud_y = height - hud_h - 16;

    draw_rounded_box(hdc, hud_x, hud_y, hud_w, hud_h, PK_CARD, PK_GREEN, 8);

    // File icon
    crate::ui::draw_file(hdc, hud_x + 14, hud_y + 16, 18, PK_GREEN_DARK, 2);

    let font_name = create_segoe_font(12, FW_SEMIBOLD);
    let old_f = SelectObject(hdc, HGDIOBJ(font_name.0));
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, PK_TEXT);

    let prefix = if prog.is_complete {
        "Transferred: "
    } else if prog.is_sending {
        "Uploading: "
    } else {
        "Receiving: "
    };
    let full_name = format!("{}{}", prefix, prog.file_name);
    let mut t_name: Vec<u16> = full_name.encode_utf16().collect();
    let mut rc_name = RECT {
        left: hud_x + 38,
        top: hud_y + 7,
        right: hud_x + hud_w - 70,
        bottom: hud_y + 26,
    };
    let _ = DrawTextW(
        hdc,
        &mut t_name,
        &mut rc_name,
        DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS | DT_VCENTER,
    );

    // Percentage
    let pct_str = format!("{}%", prog.percentage);
    let mut t_pct: Vec<u16> = pct_str.encode_utf16().collect();
    let mut rc_pct = RECT {
        left: hud_x + hud_w - 65,
        top: hud_y + 7,
        right: hud_x + hud_w - 14,
        bottom: hud_y + 26,
    };
    SetTextColor(hdc, PK_GREEN_DARK);
    let _ = DrawTextW(hdc, &mut t_pct, &mut rc_pct, DT_RIGHT | DT_SINGLELINE | DT_VCENTER);

    // Progress bar track
    let bar_x = hud_x + 38;
    let bar_y = hud_y + 32;
    let bar_w = hud_w - 52;
    let bar_h = 6;
    let track_rc = RECT {
        left: bar_x,
        top: bar_y,
        right: bar_x + bar_w,
        bottom: bar_y + bar_h,
    };
    let tb = CreateSolidBrush(PK_INPUT_BG);
    FillRect(hdc, &track_rc, tb);
    let _ = DeleteObject(HGDIOBJ(tb.0));

    // Progress bar fill
    let fill_w =
        ((bar_w as f64) * (prog.percentage as f64 / 100.0)).clamp(0.0, bar_w as f64) as i32;
    if fill_w > 0 {
        let fill_rc = RECT {
            left: bar_x,
            top: bar_y,
            right: bar_x + fill_w,
            bottom: bar_y + bar_h,
        };
        let fb = CreateSolidBrush(PK_GREEN);
        FillRect(hdc, &fill_rc, fb);
        let _ = DeleteObject(HGDIOBJ(fb.0));
    }

    let _ = SelectObject(hdc, old_f);
    let _ = DeleteObject(HGDIOBJ(font_name.0));
}
