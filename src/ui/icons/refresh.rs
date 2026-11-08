//! Lucide Refresh-Cw Icon
//! Standard 24x24 vector: dual rotating circular arrows.

use super::{draw_arc, draw_polyline, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_refresh(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Top arrow arc
    draw_arc(hdc, &ctx, 12.0, 12.0, 8.0, 20.0, 12.0, 4.0, 12.0);
    // Top arrow tip
    draw_polyline(hdc, &ctx, &[(17.0, 1.0), (21.0, 5.0), (17.0, 9.0)]);

    // Bottom arrow arc
    draw_arc(hdc, &ctx, 12.0, 12.0, 8.0, 4.0, 12.0, 20.0, 12.0);
    // Bottom arrow tip
    draw_polyline(hdc, &ctx, &[(7.0, 23.0), (3.0, 19.0), (7.0, 15.0)]);
}
