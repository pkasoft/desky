//! Keyboard & Mouse Vector Icon
//!
//! Standardized 24x24 vector icon representing remote input sovereignty
//! (Mouse & Keyboard control) for cross-platform UI.

#![allow(unsafe_op_in_unsafe_fn)]

use super::{
    draw_line, draw_rect, draw_smooth_line, draw_smooth_rounded_rect, GraphicsScope,
    IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_keyboard_mouse(
    hdc: HDC,
    x: i32,
    y: i32,
    size: i32,
    color: COLORREF,
    stroke_width: i32,
) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Keyboard body (13x11 at 1.0, 7.0)
        draw_smooth_rounded_rect(&scope, &ctx, 1.0, 7.0, 13.0, 11.0, 2.0);

        // Keyboard key rows
        draw_smooth_line(&scope, &ctx, 4.0, 10.0, 5.5, 10.0);
        draw_smooth_line(&scope, &ctx, 7.5, 10.0, 11.0, 10.0);
        draw_smooth_line(&scope, &ctx, 4.0, 14.0, 11.0, 14.0);

        // Mouse body (6x14 at 16.0, 5.0 with rx=3)
        draw_smooth_rounded_rect(&scope, &ctx, 16.0, 5.0, 6.5, 14.0, 3.0);
        // Mouse divider
        draw_smooth_line(&scope, &ctx, 19.25, 5.0, 19.25, 10.0);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_rect(hdc, &ctx, 1.0, 7.0, 13.0, 11.0, 2.0);
        draw_line(hdc, &ctx, 4.0, 10.0, 6.0, 10.0);
        draw_line(hdc, &ctx, 8.0, 10.0, 11.0, 10.0);
        draw_line(hdc, &ctx, 4.0, 14.0, 11.0, 14.0);
        draw_rect(hdc, &ctx, 16.0, 5.0, 6.5, 14.0, 3.0);
        draw_line(hdc, &ctx, 19.25, 5.0, 19.25, 10.0);
    }
}
