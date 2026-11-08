//! Lucide Settings Icon
//! Standard 24x24 vector: central circular hub and surrounding cog teeth.

use super::{draw_circle, draw_line, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_settings(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Inner hub
    draw_circle(hdc, &ctx, 12.0, 12.0, 3.5);

    // Cog perimeter outline
    draw_circle(hdc, &ctx, 12.0, 12.0, 8.5);

    // 8 radial gear teeth indicators
    let teeth = [
        (12.0, 1.5, 12.0, 4.0),
        (12.0, 20.0, 12.0, 22.5),
        (1.5, 12.0, 4.0, 12.0),
        (20.0, 12.0, 22.5, 12.0),
        (4.5, 4.5, 6.5, 6.5),
        (17.5, 17.5, 19.5, 19.5),
        (4.5, 19.5, 6.5, 17.5),
        (17.5, 6.5, 19.5, 4.5),
    ];
    for (x1, y1, x2, y2) in teeth {
        draw_line(hdc, &ctx, x1, y1, x2, y2);
    }
}
