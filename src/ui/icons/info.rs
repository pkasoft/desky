//! Lucide Info Icon
//! Standard 24x24 vector: circled information glyph.

use super::{draw_circle, draw_line, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_info(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Outer circle: center (12, 12) r=10
    draw_circle(hdc, &ctx, 12.0, 12.0, 10.0);
    // Vertical stem: (12, 16) -> (12, 12)
    draw_line(hdc, &ctx, 12.0, 16.0, 12.0, 12.0);
    // Dot: (12, 8)
    draw_line(hdc, &ctx, 12.0, 8.0, 12.0, 8.5);
}
