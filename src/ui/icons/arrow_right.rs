//! Lucide Arrow Right Vector Icon (Official Lucide `arrow-right.svg`)
//! <path d="M5 12h14"/>
//! <path d="m12 5 7 7-7 7"/>

use super::{
    draw_line, draw_polyline, draw_smooth_line, draw_smooth_polyline, GraphicsScope, IconContext,
    PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_arrow_right(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // High-Quality Anti-Aliased Vector rendering with round caps & round join
        draw_smooth_line(&scope, &ctx, 5.0, 12.0, 19.0, 12.0);
        draw_smooth_polyline(&scope, &ctx, &[(12.0, 5.0), (19.0, 12.0), (12.0, 19.0)]);
    } else {
        // GDI fallback
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_line(hdc, &ctx, 5.0, 12.0, 19.0, 12.0);
        draw_polyline(hdc, &ctx, &[(12.0, 5.0), (19.0, 12.0), (12.0, 19.0)]);
    }
}
