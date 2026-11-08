//! Lucide Radio Broadcast Vector Icon (Official Lucide `radio.svg`)
//!
//! Vector paths:
//! - `<circle cx="12" cy="12" r="2" />`
//! - `<path d="M4.9 19.1C1 15.2 1 8.8 4.9 4.9" />`
//! - `<path d="M7.8 16.2c-2.3-2.3-2.3-6.1 0-8.5" />`
//! - `<path d="M16.2 7.8c2.3 2.3 2.3 6.1 0 8.5" />`
//! - `<path d="M19.1 4.9C23 8.8 23 15.1 19.1 19" />`

use super::{
    draw_arc, draw_circle, draw_smooth_arc, draw_smooth_filled_circle, GraphicsScope, IconContext,
    PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_radio(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Center transmitter broadcast beacon dot
        draw_smooth_filled_circle(&scope, &ctx, 12.0, 12.0, 2.0, color);

        // Inner wave signals (r=4.5)
        draw_smooth_arc(&scope, &ctx, 12.0, 12.0, 4.5, 135.0, 90.0);
        draw_smooth_arc(&scope, &ctx, 12.0, 12.0, 4.5, 315.0, 90.0);

        // Outer wave signals (r=9.0)
        draw_smooth_arc(&scope, &ctx, 12.0, 12.0, 9.0, 135.0, 90.0);
        draw_smooth_arc(&scope, &ctx, 12.0, 12.0, 9.0, 315.0, 90.0);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_circle(hdc, &ctx, 12.0, 12.0, 2.0);
        draw_arc(hdc, &ctx, 12.0, 12.0, 4.0, 8.0, 8.0, 8.0, 16.0);
        draw_arc(hdc, &ctx, 12.0, 12.0, 4.0, 16.0, 16.0, 16.0, 8.0);
        draw_arc(hdc, &ctx, 12.0, 12.0, 8.0, 5.0, 5.0, 5.0, 19.0);
        draw_arc(hdc, &ctx, 12.0, 12.0, 8.0, 19.0, 19.0, 19.0, 5.0);
    }
}
