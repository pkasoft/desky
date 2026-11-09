//! Lucide Alert Triangle Vector Icon (Official Lucide `triangle-alert.svg`)
//!
//! Vector paths:
//! - `<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z" />`
//! - `<line x1="12" y1="9" x2="12" y2="13" />`
//! - `<line x1="12" y1="17" x2="12.01" y2="17" />`

use super::{
    draw_line, draw_polyline, draw_smooth_filled_circle, draw_smooth_line, draw_smooth_polyline,
    GraphicsScope, IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_alert_triangle(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Warning triangle
        draw_smooth_polyline(
            &scope,
            &ctx,
            &[
                (12.0, 3.5),
                (21.5, 19.5),
                (2.5, 19.5),
                (12.0, 3.5),
            ],
        );
        // Exclamation mark line
        draw_smooth_line(&scope, &ctx, 12.0, 9.0, 12.0, 13.5);
        // Exclamation dot
        draw_smooth_filled_circle(&scope, &ctx, 12.0, 16.5, 1.2, color);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_polyline(
            hdc,
            &ctx,
            &[
                (12.0, 3.5),
                (21.5, 19.5),
                (2.5, 19.5),
                (12.0, 3.5),
            ],
        );
        draw_line(hdc, &ctx, 12.0, 9.0, 12.0, 13.5);
        draw_line(hdc, &ctx, 12.0, 16.0, 12.0, 17.0);
    }
}
