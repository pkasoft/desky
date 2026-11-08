//! Lucide Copy Vector Icon (Official Lucide `copy.svg`)
//!
//! Vector paths:
//! - `<rect width="14" height="14" x="8" y="8" rx="2" ry="2" />`
//! - `<path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />`

use super::{
    draw_polyline, draw_rect, draw_smooth_polyline, draw_smooth_rounded_rect, GraphicsScope,
    IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_copy(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Front page card (14x14 at 8,8 with r=2)
        draw_smooth_rounded_rect(&scope, &ctx, 8.0, 8.0, 14.0, 14.0, 2.0);

        // Back page offset outline with rounded top-left corner
        draw_smooth_polyline(
            &scope,
            &ctx,
            &[
                (16.0, 4.0),
                (15.5, 2.5),
                (14.0, 2.0),
                (4.0, 2.0),
                (2.5, 2.5),
                (2.0, 4.0),
                (2.0, 14.0),
                (2.5, 15.5),
                (4.0, 16.0),
            ],
        );
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_rect(hdc, &ctx, 8.0, 8.0, 14.0, 14.0, 2.0);
        draw_polyline(hdc, &ctx, &[(16.0, 4.0), (14.0, 2.0), (4.0, 2.0), (2.0, 4.0), (2.0, 14.0), (4.0, 16.0)]);
    }
}
