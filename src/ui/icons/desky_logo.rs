//! Desky Brand Vector Logo Icon
//!
//! Direct native GDI/GDI+ implementation of `pkasoft.com/static/desky-icon.svg`:
//! - Signature Deep Emerald background canvas (#143828)
//! - Subtle inner gradient glow & emerald outline (#2d6a4f)
//! - Primary Remote Desktop display viewport (#0d1f16) with mint outline (#52b788)
//! - Display stand neck (#2d6a4f) and base (#52b788)
//! - Peer-to-Peer direct connection waves & lightning arrow (#74c69d / #d8f3dc)
//! - Live mint online status indicator dot (#4ade80)

#![allow(unsafe_op_in_unsafe_fn)]
#![allow(unused_must_use)]

use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::{
    Arc, CreatePen, CreateSolidBrush, DeleteObject, Ellipse, LineTo, MoveToEx, RoundRect,
    SelectObject, HDC, HGDIOBJ, PS_SOLID,
};

/// Renders the authentic Desky logo in native GDI
pub unsafe fn draw_desky_logo(hdc: HDC, x: i32, y: i32, size: i32) {
    let r = (size as f32 * 0.22) as i32;

    // Outer Deep Emerald rounded square (#143828) with subtle emerald border (#2d6a4f)
    let pen_border = CreatePen(PS_SOLID, 1, COLORREF(0x004F6A2D)); // #2d6a4f
    let brush_bg = CreateSolidBrush(COLORREF(0x00283814)); // #143828
    let old_p = SelectObject(hdc, HGDIOBJ(pen_border.0));
    let old_b = SelectObject(hdc, HGDIOBJ(brush_bg.0));
    RoundRect(hdc, x, y, x + size, y + size, r, r);
    let _ = SelectObject(hdc, old_b);
    let _ = SelectObject(hdc, old_p);
    let _ = DeleteObject(HGDIOBJ(brush_bg.0));
    let _ = DeleteObject(HGDIOBJ(pen_border.0));

    // Primary Remote Desktop Display (#0d1f16 with #52b788 border)
    let margin = (size as f32 * 0.16) as i32;
    let disp_w = (size - margin * 2).max(10);
    let disp_h = ((disp_w as f32 * 0.65) as i32).max(6);
    let disp_x = x + margin;
    let disp_y = y + margin - (if size > 30 { 2 } else { 1 });

    let pen_screen = CreatePen(PS_SOLID, if size > 30 { 2 } else { 1 }, COLORREF(0x0088B752)); // #52b788
    let brush_screen = CreateSolidBrush(COLORREF(0x00161F0D)); // #0d1f16
    let old_ps = SelectObject(hdc, HGDIOBJ(pen_screen.0));
    let old_bs = SelectObject(hdc, HGDIOBJ(brush_screen.0));
    let screen_r = 2.max((size as f32 * 0.08) as i32);
    RoundRect(hdc, disp_x, disp_y, disp_x + disp_w, disp_y + disp_h, screen_r, screen_r);
    let _ = SelectObject(hdc, old_bs);
    let _ = SelectObject(hdc, old_ps);
    let _ = DeleteObject(HGDIOBJ(brush_screen.0));
    let _ = DeleteObject(HGDIOBJ(pen_screen.0));

    // Display Stand (#2d6a4f stand neck, #52b788 base)
    let stand_cx = x + size / 2;
    let stand_top_y = disp_y + disp_h;
    let stand_h = (size as f32 * 0.14) as i32;
    let base_w = (size as f32 * 0.38) as i32;
    let base_y = stand_top_y + stand_h;

    let pen_stand = CreatePen(PS_SOLID, if size > 30 { 2 } else { 1 }, COLORREF(0x004F6A2D));
    let old_pen = SelectObject(hdc, HGDIOBJ(pen_stand.0));
    MoveToEx(hdc, stand_cx, stand_top_y, None);
    LineTo(hdc, stand_cx, base_y);
    let pen_base = CreatePen(PS_SOLID, if size > 30 { 2 } else { 1 }, COLORREF(0x0088B752));
    let _ = SelectObject(hdc, HGDIOBJ(pen_base.0));
    MoveToEx(hdc, stand_cx - base_w / 2, base_y, None);
    LineTo(hdc, stand_cx + base_w / 2, base_y);
    let _ = SelectObject(hdc, old_pen);
    let _ = DeleteObject(HGDIOBJ(pen_stand.0));
    let _ = DeleteObject(HGDIOBJ(pen_base.0));

    // Screen Center Content: P2P Central Node (#74c69d)
    let cx = disp_x + disp_w / 2;
    let cy = disp_y + disp_h / 2;
    let node_r = 1.max((size as f32 * 0.04) as i32);
    let brush_mint = CreateSolidBrush(COLORREF(0x009DC674)); // #74c69d
    let old_brush = SelectObject(hdc, HGDIOBJ(brush_mint.0));
    Ellipse(hdc, cx - node_r, cy - node_r, cx + node_r + 1, cy + node_r + 1);
    let _ = SelectObject(hdc, old_brush);
    let _ = DeleteObject(HGDIOBJ(brush_mint.0));

    // High-speed Pulse Waves (Left & Right) if size is sufficiently large
    if size >= 36 {
        let pen_wave = CreatePen(PS_SOLID, 1, COLORREF(0x0088B752));
        let old_p = SelectObject(hdc, HGDIOBJ(pen_wave.0));
        let wave_r1 = (size as f32 * 0.09) as i32;
        Arc(hdc, cx - wave_r1, cy - wave_r1, cx + wave_r1, cy + wave_r1, cx, cy - wave_r1, cx, cy + wave_r1);
        Arc(hdc, cx - wave_r1, cy - wave_r1, cx + wave_r1, cy + wave_r1, cx, cy + wave_r1, cx, cy - wave_r1);

        let wave_r2 = (size as f32 * 0.15) as i32;
        let pen_wave2 = CreatePen(PS_SOLID, 1, COLORREF(0x009DC674));
        let _ = SelectObject(hdc, HGDIOBJ(pen_wave2.0));
        Arc(hdc, cx - wave_r2, cy - wave_r2, cx + wave_r2, cy + wave_r2, cx, cy - wave_r2, cx, cy + wave_r2);
        Arc(hdc, cx - wave_r2, cy - wave_r2, cx + wave_r2, cy + wave_r2, cx, cy + wave_r2, cx, cy - wave_r2);

        let _ = SelectObject(hdc, old_p);
        let _ = DeleteObject(HGDIOBJ(pen_wave.0));
        let _ = DeleteObject(HGDIOBJ(pen_wave2.0));
    }

    // Status Indicator Pill: Online Mint Dot (#4ade80)
    let dot_r = 1.max((size as f32 * 0.045) as i32);
    let dot_x = disp_x + disp_w - dot_r - 2;
    let dot_y = disp_y + dot_r + 2;
    let brush_online = CreateSolidBrush(COLORREF(0x0080DE4A)); // #4ade80
    let old_b = SelectObject(hdc, HGDIOBJ(brush_online.0));
    Ellipse(hdc, dot_x - dot_r, dot_y - dot_r, dot_x + dot_r + 1, dot_y + dot_r + 1);
    let _ = SelectObject(hdc, old_b);
    let _ = DeleteObject(HGDIOBJ(brush_online.0));
}
