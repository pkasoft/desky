//! Lucide Trash Can Vector Icon (Official Lucide `trash-2.svg`)
//!
//! Vector paths:
//! - `<path d="M3 6h18" />`
//! - `<path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />`
//! - `<path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />`
//! - `<line x1="10" x2="10" y1="11" y2="17" />`
//! - `<line x1="14" x2="14" y1="11" y2="17" />`

use super::{
    draw_line, draw_polyline, draw_smooth_line, draw_smooth_polyline, GraphicsScope, IconContext,
    PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_trash(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Can rim bar
        draw_smooth_line(&scope, &ctx, 3.0, 6.0, 21.0, 6.0);

        // Can basket body with rounded bottom corners
        draw_smooth_polyline(
            &scope,
            &ctx,
            &[
                (5.0, 6.0),
                (5.5, 19.0),
                (6.5, 21.0),
                (17.5, 21.0),
                (18.5, 19.0),
                (19.0, 6.0),
            ],
        );

        // Lid handle top
        draw_smooth_polyline(
            &scope,
            &ctx,
            &[
                (8.0, 6.0),
                (8.0, 3.5),
                (9.0, 2.0),
                (15.0, 2.0),
                (16.0, 3.5),
                (16.0, 6.0),
            ],
        );

        // Vertical inner ribs
        draw_smooth_line(&scope, &ctx, 10.0, 10.0, 10.0, 17.0);
        draw_smooth_line(&scope, &ctx, 14.0, 10.0, 14.0, 17.0);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_line(hdc, &ctx, 3.0, 6.0, 21.0, 6.0);
        draw_polyline(hdc, &ctx, &[(5.0, 6.0), (6.0, 20.0), (18.0, 20.0), (19.0, 6.0)]);
        draw_polyline(hdc, &ctx, &[(8.0, 6.0), (8.0, 3.0), (16.0, 3.0), (16.0, 6.0)]);
        draw_line(hdc, &ctx, 10.0, 9.0, 10.0, 17.0);
        draw_line(hdc, &ctx, 14.0, 9.0, 14.0, 17.0);
    }
}
