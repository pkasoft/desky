//! Lucide Trash-2 Icon
//! Standard 24x24 vector: trash bin body, horizontal lid, and vertical ribs.

use super::{draw_line, draw_polyline, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_trash(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Lid bar: (3, 6) -> (21, 6)
    draw_line(hdc, &ctx, 3.0, 6.0, 21.0, 6.0);
    // Lid handle: (8, 6) -> (8, 4) -> (16, 4) -> (16, 6)
    draw_polyline(hdc, &ctx, &[(8.0, 6.0), (8.0, 4.0), (16.0, 4.0), (16.0, 6.0)]);
    // Bin container: (19, 6) -> (18, 20) -> (6, 20) -> (5, 6)
    draw_polyline(hdc, &ctx, &[(19.0, 6.0), (18.0, 20.0), (6.0, 20.0), (5.0, 6.0)]);
    // Inner ribs
    draw_line(hdc, &ctx, 10.0, 11.0, 10.0, 17.0);
    draw_line(hdc, &ctx, 14.0, 11.0, 14.0, 17.0);
}
