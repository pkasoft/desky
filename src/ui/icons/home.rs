//! Lucide Home Vector Icon (Official Lucide `home.svg`)
//!
//! Vector paths:
//! - `<path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>`
//! - `<polyline points="9 22 9 12 15 12 15 22"/>`

use super::{
    draw_line, draw_polyline, draw_smooth_polyline, GraphicsScope, IconContext,
    PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_home(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Roof gable + House body perimeter
        draw_smooth_polyline(
            &scope,
            &ctx,
            &[
                (3.0, 9.0),
                (12.0, 2.0),
                (21.0, 9.0),
                (20.0, 10.0),
                (20.0, 20.0),
                (19.0, 21.0),
                (5.0, 21.0),
                (4.0, 20.0),
                (4.0, 10.0),
                (3.0, 9.0),
            ],
        );

        // Doorway
        draw_smooth_polyline(
            &scope,
            &ctx,
            &[(9.0, 21.0), (9.0, 12.0), (15.0, 12.0), (15.0, 21.0)],
        );
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_line(hdc, &ctx, 3.0, 9.0, 12.0, 2.0);
        draw_line(hdc, &ctx, 12.0, 2.0, 21.0, 9.0);
        draw_line(hdc, &ctx, 20.0, 9.0, 20.0, 21.0);
        draw_line(hdc, &ctx, 20.0, 21.0, 4.0, 21.0);
        draw_line(hdc, &ctx, 4.0, 21.0, 4.0, 9.0);
        draw_polyline(hdc, &ctx, &[(9.0, 21.0), (9.0, 12.0), (15.0, 12.0), (15.0, 21.0)]);
    }
}
