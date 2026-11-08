//! Lucide Check Vector Icon (Official Lucide `check.svg`)
//!
//! Vector paths:
//! - `<path d="M20 6 9 17l-5-5" />`

use super::{
    draw_polyline, draw_smooth_polyline, GraphicsScope, IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_check(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        draw_smooth_polyline(&scope, &ctx, &[(4.0, 12.0), (9.0, 17.0), (20.0, 6.0)]);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_polyline(hdc, &ctx, &[(4.0, 12.0), (9.0, 17.0), (20.0, 6.0)]);
    }
}
