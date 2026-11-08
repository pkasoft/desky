//! Lucide Pencil / Pen Tool Vector Icon (Official Lucide `pencil.svg`)
//!
//! Vector paths:
//! - `<path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z" />`
//! - `<path d="m15 5 4 4" />`

use super::{
    draw_line, draw_polyline, draw_smooth_line, draw_smooth_polyline, GraphicsScope, IconContext,
    PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_pen(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Main pencil shaft and nib
        draw_smooth_polyline(
            &scope,
            &ctx,
            &[
                (17.0, 3.0),
                (21.0, 7.0),
                (7.0, 21.0),
                (3.0, 21.0),
                (3.0, 17.0),
                (17.0, 3.0),
            ],
        );
        draw_smooth_line(&scope, &ctx, 14.0, 6.0, 18.0, 10.0);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_polyline(
            hdc,
            &ctx,
            &[
                (17.0, 3.0),
                (21.0, 7.0),
                (7.0, 21.0),
                (3.0, 21.0),
                (3.0, 17.0),
                (17.0, 3.0),
            ],
        );
        draw_line(hdc, &ctx, 14.0, 6.0, 18.0, 10.0);
    }
}
