//! Lucide Message Square Vector Icon (Official Lucide `message-square.svg`)
//!
//! Vector paths:
//! - `<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />`

#![allow(unsafe_op_in_unsafe_fn)]

use super::{
    draw_polyline, draw_smooth_polyline, GraphicsScope, IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_message_square(
    hdc: HDC,
    x: i32,
    y: i32,
    size: i32,
    color: COLORREF,
    stroke_width: i32,
) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        draw_smooth_polyline(
            &scope,
            &ctx,
            &[
                (21.0, 15.0),
                (21.0, 5.0),
                (3.0, 5.0),
                (3.0, 19.0),
                (7.0, 15.0),
                (19.0, 15.0),
                (21.0, 15.0),
            ],
        );
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_polyline(
            hdc,
            &ctx,
            &[
                (21.0, 15.0),
                (21.0, 5.0),
                (3.0, 5.0),
                (3.0, 19.0),
                (7.0, 15.0),
                (19.0, 15.0),
                (21.0, 15.0),
            ],
        );
    }
}
