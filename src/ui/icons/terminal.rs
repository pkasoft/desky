//! Lucide Terminal Icon
//! Standard 24x24 vector: prompt chevron `>` and cursor line `_`.

use super::{draw_line, draw_polyline, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_terminal(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Chevron prompt: (4, 17) -> (10, 12) -> (4, 7)
    draw_polyline(hdc, &ctx, &[(4.0, 7.0), (10.0, 12.0), (4.0, 17.0)]);
    // Cursor underline: (12, 19) -> (20, 19)
    draw_line(hdc, &ctx, 12.0, 19.0, 20.0, 19.0);
}
