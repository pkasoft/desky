//! Lucide Copy Icon
//! Standard 24x24 vector: overlapping clipboard documents.

use super::{draw_polyline, draw_rect, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_copy(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Front card: (9, 9) to (21, 21) r=2
    draw_rect(hdc, &ctx, 9.0, 9.0, 12.0, 12.0, 2.0);

    // Back card contour: (5, 15) -> (5, 5) -> (15, 5)
    draw_polyline(hdc, &ctx, &[(5.0, 15.0), (5.0, 5.0), (15.0, 5.0)]);
}
