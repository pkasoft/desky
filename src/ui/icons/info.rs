//! Lucide Info Vector Icon (Official Lucide `info.svg`)
//!
//! Vector paths:
//! - `<circle cx="12" cy="12" r="10" />`
//! - `<path d="M12 16v-4" />`
//! - `<path d="M12 8h.01" />`

use super::{
    draw_circle, draw_line, draw_smooth_circle, draw_smooth_filled_circle, draw_smooth_line,
    GraphicsScope, IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_info(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Enclosing circular ring
        draw_smooth_circle(&scope, &ctx, 12.0, 12.0, 10.0);

        // Information stem
        draw_smooth_line(&scope, &ctx, 12.0, 12.0, 12.0, 16.5);

        // Dot above stem
        draw_smooth_filled_circle(&scope, &ctx, 12.0, 8.0, 1.2, color);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_circle(hdc, &ctx, 12.0, 12.0, 10.0);
        draw_line(hdc, &ctx, 12.0, 12.0, 12.0, 16.0);
        draw_circle(hdc, &ctx, 12.0, 8.0, 1.0);
    }
}
