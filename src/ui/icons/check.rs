//! Lucide Check Icon
//! Standard 24x24 vector: clean crisp checkmark polyline.

use super::{draw_polyline, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_check(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Polyline: (4, 12) -> (9, 17) -> (20, 6)
    draw_polyline(hdc, &ctx, &[(4.0, 12.0), (9.0, 17.0), (20.0, 6.0)]);
}
