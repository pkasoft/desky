//! Lucide Plus Vector Icon (Official Lucide `plus.svg`)
//!
//! Vector paths:
//! - `<path d="M5 12h14" />`
//! - `<path d="M12 5v14" />`

use super::{
    draw_line, draw_smooth_line, GraphicsScope, IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_plus(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        draw_smooth_line(&scope, &ctx, 5.0, 12.0, 19.0, 12.0);
        draw_smooth_line(&scope, &ctx, 12.0, 5.0, 12.0, 19.0);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_line(hdc, &ctx, 5.0, 12.0, 19.0, 12.0);
        draw_line(hdc, &ctx, 12.0, 5.0, 12.0, 19.0);
    }
}
