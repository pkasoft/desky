//! Lucide Bell Icon
//! Standard 24x24 vector: notification bell body and clapper.

use super::{draw_arc, draw_line, draw_polyline, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_bell(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Bell dome
    draw_arc(hdc, &ctx, 12.0, 8.0, 6.0, 18.0, 8.0, 6.0, 8.0);
    // Left flare: (6, 8) -> (6, 12) -> (3, 17) -> (21, 17) -> (18, 12) -> (18, 8)
    draw_polyline(
        hdc,
        &ctx,
        &[
            (6.0, 8.0),
            (6.0, 12.0),
            (3.0, 17.0),
            (21.0, 17.0),
            (18.0, 12.0),
            (18.0, 8.0),
        ],
    );
    // Base rim line
    draw_line(hdc, &ctx, 2.0, 17.0, 22.0, 17.0);
    // Clapper arc
    draw_arc(hdc, &ctx, 12.0, 18.0, 3.0, 14.5, 20.0, 9.5, 20.0);
}
