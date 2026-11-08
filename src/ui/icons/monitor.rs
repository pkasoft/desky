//! Lucide Monitor Icon
//! Standard 24x24 vector: display screen, stand, and base.

use super::{draw_line, draw_rect, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_monitor(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Screen border: rect (2, 3) to (22, 17) r=2
    draw_rect(hdc, &ctx, 2.0, 3.0, 20.0, 14.0, 2.0);
    // Vertical stand line: (12, 17) -> (12, 21)
    draw_line(hdc, &ctx, 12.0, 17.0, 12.0, 21.0);
    // Horizontal base line: (8, 21) -> (16, 21)
    draw_line(hdc, &ctx, 8.0, 21.0, 16.0, 21.0);
}
