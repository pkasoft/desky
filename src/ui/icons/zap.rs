//! Lucide Lightning Zap Vector Icon (Official Lucide `zap.svg`)
//!
//! Vector paths:
//! - `<path d="M13 2 3 14h9l-1 8 10-12h-9l1-8z" />`

use super::{
    draw_polyline, draw_smooth_polyline, GraphicsScope, IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_zap(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Lightning bolt closed perimeter
        draw_smooth_polyline(
            &scope,
            &ctx,
            &[
                (13.0, 2.0),
                (3.0, 14.0),
                (12.0, 14.0),
                (11.0, 22.0),
                (21.0, 10.0),
                (12.0, 10.0),
                (13.0, 2.0),
            ],
        );
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_polyline(
            hdc,
            &ctx,
            &[
                (13.0, 2.0),
                (3.0, 14.0),
                (12.0, 14.0),
                (11.0, 22.0),
                (21.0, 10.0),
                (12.0, 10.0),
                (13.0, 2.0),
            ],
        );
    }
}
