//! Lucide File Vector Icon (Official Lucide `file.svg`)
//!
//! Vector paths:
//! - `<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />`
//! - `<path d="M14 2v4a2 2 0 0 0 2 2h4" />`

use super::{
    draw_polyline, draw_smooth_polyline, GraphicsScope, IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_file(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Document page perimeter with top-right corner bevel
        draw_smooth_polyline(
            &scope,
            &ctx,
            &[
                (14.0, 2.0),
                (6.0, 2.0),
                (4.0, 4.0),
                (4.0, 20.0),
                (6.0, 22.0),
                (18.0, 22.0),
                (20.0, 20.0),
                (20.0, 8.0),
                (14.0, 2.0),
            ],
        );

        // Folded flap at top-right
        draw_smooth_polyline(&scope, &ctx, &[(14.0, 2.0), (14.0, 7.5), (14.5, 8.0), (20.0, 8.0)]);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_polyline(
            hdc,
            &ctx,
            &[
                (14.0, 2.0),
                (6.0, 2.0),
                (4.0, 4.0),
                (4.0, 20.0),
                (6.0, 22.0),
                (18.0, 22.0),
                (20.0, 20.0),
                (20.0, 8.0),
                (14.0, 2.0),
            ],
        );
        draw_polyline(hdc, &ctx, &[(14.0, 2.0), (14.0, 8.0), (20.0, 8.0)]);
    }
}
