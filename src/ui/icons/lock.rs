//! Lucide Lock Icon
//! Standard 24x24 vector: padlock body and rounded shackle arc.

use super::{draw_arc, draw_line, draw_rect, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_lock(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Padlock body: rect (3, 11) to (21, 22) r=2
    draw_rect(hdc, &ctx, 3.0, 11.0, 18.0, 11.0, 2.0);

    // Shackle semi-circle arc: center (12, 7) r=5
    draw_arc(hdc, &ctx, 12.0, 7.0, 5.0, 17.0, 7.0, 7.0, 7.0);

    // Shackle straight legs down into lock body
    draw_line(hdc, &ctx, 7.0, 7.0, 7.0, 11.0);
    draw_line(hdc, &ctx, 17.0, 7.0, 17.0, 11.0);
}
