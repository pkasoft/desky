//! Lucide Refresh Clockwise Vector Icon (Official Lucide `refresh-cw.svg`)
//!
//! Vector paths:
//! - `<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" />`
//! - `<path d="M21 3v5h-5" />`
//! - `<path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" />`
//! - `<path d="M8 16H3v5" />`

use super::{
    draw_arc, draw_polyline, draw_smooth_arc, draw_smooth_polyline, GraphicsScope, IconContext,
    PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_refresh(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Top circular arc (from 180° around through top to ~330°)
        draw_smooth_arc(&scope, &ctx, 12.0, 12.0, 8.5, 180.0, 150.0);

        // Top arrowhead at (20.5, 7.8)
        draw_smooth_polyline(&scope, &ctx, &[(21.0, 3.0), (21.0, 8.0), (16.0, 8.0)]);

        // Bottom circular arc (from 0° around through bottom to ~150°)
        draw_smooth_arc(&scope, &ctx, 12.0, 12.0, 8.5, 0.0, 150.0);

        // Bottom arrowhead at (3.5, 16.2)
        draw_smooth_polyline(&scope, &ctx, &[(8.0, 16.0), (3.0, 16.0), (3.0, 21.0)]);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_arc(hdc, &ctx, 12.0, 12.0, 8.0, 3.0, 12.0, 20.0, 8.0);
        draw_polyline(hdc, &ctx, &[(21.0, 3.0), (21.0, 8.0), (16.0, 8.0)]);
        draw_arc(hdc, &ctx, 12.0, 12.0, 8.0, 21.0, 12.0, 4.0, 16.0);
        draw_polyline(hdc, &ctx, &[(8.0, 16.0), (3.0, 16.0), (3.0, 21.0)]);
    }
}
