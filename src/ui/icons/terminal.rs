//! Lucide Terminal Console Vector Icon (Official Lucide `terminal.svg`)
//!
//! Vector paths:
//! - `<polyline points="4 17 10 11 4 5" />`
//! - `<line x1="12" x2="20" y1="19" y2="19" />`

use super::{
    draw_line, draw_polyline, draw_smooth_line, draw_smooth_polyline, GraphicsScope, IconContext,
    PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_terminal(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // CLI command prompt chevron `>`
        draw_smooth_polyline(&scope, &ctx, &[(4.0, 5.0), (10.0, 11.0), (4.0, 17.0)]);

        // Underscore cursor `_`
        draw_smooth_line(&scope, &ctx, 12.0, 19.0, 20.0, 19.0);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_polyline(hdc, &ctx, &[(4.0, 5.0), (10.0, 11.0), (4.0, 17.0)]);
        draw_line(hdc, &ctx, 12.0, 19.0, 20.0, 19.0);
    }
}
