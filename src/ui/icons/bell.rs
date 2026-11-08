//! Lucide Bell Vector Icon (Official Lucide `bell.svg`)
//!
//! Vector paths:
//! - `<path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9" />`
//! - `<path d="M10.3 21a1.94 1.94 0 0 0 3.4 0" />`

use super::{
    draw_arc, draw_line, draw_polyline, draw_smooth_arc, draw_smooth_line, draw_smooth_polyline,
    GraphicsScope, IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_bell(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Bell dome top arc: 180° to 360° (center at 12, 8 with radius 6)
        draw_smooth_arc(&scope, &ctx, 12.0, 8.0, 6.0, 180.0, 180.0);

        // Sides flaring out to base
        draw_smooth_polyline(&scope, &ctx, &[(6.0, 8.0), (6.0, 11.0), (4.5, 14.5), (3.0, 17.0)]);
        draw_smooth_polyline(&scope, &ctx, &[(18.0, 8.0), (18.0, 11.0), (19.5, 14.5), (21.0, 17.0)]);

        // Horizontal base rim
        draw_smooth_line(&scope, &ctx, 3.0, 17.0, 21.0, 17.0);

        // Clapper at bottom (radius ~2.2)
        draw_smooth_arc(&scope, &ctx, 12.0, 19.2, 2.2, 10.0, 160.0);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_arc(hdc, &ctx, 12.0, 8.0, 6.0, 18.0, 8.0, 6.0, 8.0);
        draw_polyline(hdc, &ctx, &[(6.0, 8.0), (5.0, 13.0), (3.0, 17.0)]);
        draw_polyline(hdc, &ctx, &[(18.0, 8.0), (19.0, 13.0), (21.0, 17.0)]);
        draw_line(hdc, &ctx, 3.0, 17.0, 21.0, 17.0);
        draw_arc(hdc, &ctx, 12.0, 18.0, 3.0, 14.5, 20.0, 9.5, 20.0);
    }
}
