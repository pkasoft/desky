//! Lucide Plus Icon
//! Standard 24x24 vector: clean addition cross.

use super::{draw_line, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_plus(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Vertical line: (12, 5) -> (12, 19)
    draw_line(hdc, &ctx, 12.0, 5.0, 12.0, 19.0);
    // Horizontal line: (5, 12) -> (19, 12)
    draw_line(hdc, &ctx, 5.0, 12.0, 19.0, 12.0);
}
