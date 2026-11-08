//! Lucide X Icon
//! Standard 24x24 vector: clean crisp diagonal cross.

use super::{draw_line, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_x(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    draw_line(hdc, &ctx, 18.0, 6.0, 6.0, 18.0);
    draw_line(hdc, &ctx, 6.0, 6.0, 18.0, 18.0);
}
