//! Lucide `more-horizontal` icon (three aligned dots)
#![allow(unsafe_op_in_unsafe_fn)]

use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, DeleteObject, Ellipse, GetStockObject, SelectObject, HDC, HGDIOBJ, NULL_PEN,
};

pub unsafe fn draw_more_horizontal(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF) {
    let dot_r = (size as f32 * 0.12).max(1.5) as i32;
    let cy = y + size / 2;
    let cx = x + size / 2;
    let gap = (size as f32 * 0.30) as i32;

    let brush = CreateSolidBrush(color);
    let old_brush = SelectObject(hdc, HGDIOBJ(brush.0));
    let old_pen = SelectObject(hdc, GetStockObject(NULL_PEN));

    for dx in [-gap, 0, gap] {
        let _ = Ellipse(
            hdc,
            cx + dx - dot_r,
            cy - dot_r,
            cx + dx + dot_r + 1,
            cy + dot_r + 1,
        );
    }

    let _ = SelectObject(hdc, old_pen);
    let _ = SelectObject(hdc, old_brush);
    let _ = DeleteObject(HGDIOBJ(brush.0));
}
