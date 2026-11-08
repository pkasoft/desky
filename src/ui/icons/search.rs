//! Lucide Search Vector Icon (Official Lucide `search.svg`)
//!
//! Vector paths:
//! - `<circle cx="11" cy="11" r="8" />`
//! - `<path d="m21 21-4.3-4.3" />`

use super::{
    draw_circle, draw_line, draw_smooth_circle, draw_smooth_line, GraphicsScope, IconContext,
    PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_search(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Lens circle (center 10.5, 10.5, radius 7.5)
        draw_smooth_circle(&scope, &ctx, 10.5, 10.5, 7.5);

        // Handle line (from rim at 16,16 down to 21,21)
        draw_smooth_line(&scope, &ctx, 15.8, 15.8, 21.0, 21.0);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_circle(hdc, &ctx, 11.0, 11.0, 7.5);
        draw_line(hdc, &ctx, 16.0, 16.0, 21.0, 21.0);
    }
}
