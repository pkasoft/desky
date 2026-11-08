//! Lucide Radio / Radar Broadcast Icon
//! Standard 24x24 vector: central emitter and radiating concentric wave arcs.

use super::{draw_arc, draw_circle, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_radio(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Center transmitter core: (12, 12) r=2
    draw_circle(hdc, &ctx, 12.0, 12.0, 2.0);

    // Inner wave arcs
    draw_arc(hdc, &ctx, 12.0, 12.0, 6.0, 16.0, 8.0, 16.0, 16.0);
    draw_arc(hdc, &ctx, 12.0, 12.0, 6.0, 8.0, 16.0, 8.0, 8.0);

    // Outer wave arcs
    draw_arc(hdc, &ctx, 12.0, 12.0, 10.0, 19.0, 5.0, 19.0, 19.0);
    draw_arc(hdc, &ctx, 12.0, 12.0, 10.0, 5.0, 19.0, 5.0, 5.0);
}
