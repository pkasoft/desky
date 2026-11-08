//! Lucide Close X Vector Icon (Official Lucide `x.svg`)
//!
//! Vector paths:
//! - `<path d="M18 6 6 18" />`
//! - `<path d="m6 6 12 12" />`

use super::{
    draw_line, draw_smooth_line, GraphicsScope, IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_x(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        draw_smooth_line(&scope, &ctx, 18.0, 6.0, 6.0, 18.0);
        draw_smooth_line(&scope, &ctx, 6.0, 6.0, 18.0, 18.0);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_line(hdc, &ctx, 18.0, 6.0, 6.0, 18.0);
        draw_line(hdc, &ctx, 6.0, 6.0, 18.0, 18.0);
    }
}
