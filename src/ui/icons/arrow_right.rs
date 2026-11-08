//! Lucide Arrow Right Vector Icon
//! Standard 24x24 vector: sleek horizontal arrow pointing forward (5,12)->(19,12) with chevron head.

use super::{draw_line, draw_polyline, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_arrow_right(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Stem: (5, 12) -> (19, 12)
    draw_line(hdc, &ctx, 5.0, 12.0, 19.0, 12.0);

    // Chevron head: (12, 5) -> (19, 12) -> (12, 19)
    draw_polyline(hdc, &ctx, &[(12.0, 5.0), (19.0, 12.0), (12.0, 19.0)]);
}
