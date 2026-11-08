//! Lucide Cpu Icon
//! Standard 24x24 vector: central processor chip, core die, and connector pins.

use super::{draw_line, draw_rect, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_cpu(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Outer chip body: rect (4, 4) to (20, 20) r=2
    draw_rect(hdc, &ctx, 4.0, 4.0, 16.0, 16.0, 2.0);
    // Inner core die: rect (9, 9) to (15, 15)
    draw_rect(hdc, &ctx, 9.0, 9.0, 6.0, 6.0, 1.0);

    // Pins top
    draw_line(hdc, &ctx, 9.0, 1.0, 9.0, 4.0);
    draw_line(hdc, &ctx, 15.0, 1.0, 15.0, 4.0);

    // Pins bottom
    draw_line(hdc, &ctx, 9.0, 20.0, 9.0, 23.0);
    draw_line(hdc, &ctx, 15.0, 20.0, 15.0, 23.0);

    // Pins left
    draw_line(hdc, &ctx, 1.0, 9.0, 4.0, 9.0);
    draw_line(hdc, &ctx, 1.0, 15.0, 4.0, 15.0);

    // Pins right
    draw_line(hdc, &ctx, 20.0, 9.0, 23.0, 9.0);
    draw_line(hdc, &ctx, 20.0, 15.0, 23.0, 15.0);
}
