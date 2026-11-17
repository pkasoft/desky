//! Lucide `chevron-left` and `chevron-right` vector icons
#![allow(unsafe_op_in_unsafe_fn)]

use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::{CreatePen, DeleteObject, LineTo, MoveToEx, SelectObject, HDC, HGDIOBJ, PS_SOLID};

pub unsafe fn draw_chevron_left(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let pen = CreatePen(PS_SOLID, stroke_width, color);
    let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));

    let pad_x = size * 3 / 8;
    let pad_y = size / 4;

    let p1_x = x + size - pad_x;
    let p1_y = y + pad_y;
    let p2_x = x + pad_x;
    let p2_y = y + size / 2;
    let p3_x = x + size - pad_x;
    let p3_y = y + size - pad_y;

    let _ = MoveToEx(hdc, p1_x, p1_y, None);
    let _ = LineTo(hdc, p2_x, p2_y);
    let _ = LineTo(hdc, p3_x, p3_y);

    let _ = SelectObject(hdc, old_pen);
    let _ = DeleteObject(HGDIOBJ(pen.0));
}

pub unsafe fn draw_chevron_right(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let pen = CreatePen(PS_SOLID, stroke_width, color);
    let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));

    let pad_x = size * 3 / 8;
    let pad_y = size / 4;

    let p1_x = x + pad_x;
    let p1_y = y + pad_y;
    let p2_x = x + size - pad_x;
    let p2_y = y + size / 2;
    let p3_x = x + pad_x;
    let p3_y = y + size - pad_y;

    let _ = MoveToEx(hdc, p1_x, p1_y, None);
    let _ = LineTo(hdc, p2_x, p2_y);
    let _ = LineTo(hdc, p3_x, p3_y);

    let _ = SelectObject(hdc, old_pen);
    let _ = DeleteObject(HGDIOBJ(pen.0));
}
