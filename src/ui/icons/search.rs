//! Lucide Search Icon
//! Standard 24x24 vector: circular lens and diagonal handle.

use super::{draw_circle, draw_line, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_search(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Lens circle at (11, 11) r=7.5
    draw_circle(hdc, &ctx, 11.0, 11.0, 7.5);
    // Handle line from (16.5, 16.5) -> (21.5, 21.5)
    draw_line(hdc, &ctx, 16.5, 16.5, 21.5, 21.5);
}
