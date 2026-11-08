//! Lucide Server Vector Icon
//! Standard 24x24 vector: dual stacked server blades with status indicator lights and drive slots.
//! Perfectly symmetric across both horizontal and vertical axes (center at 12.0, 12.0).

use super::{draw_circle, draw_line, draw_rect, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_server(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // 1. Top Server Blade: [3.0, 3.0] -> [21.0, 10.5], corner radius 1.5
    draw_rect(hdc, &ctx, 3.0, 3.0, 18.0, 7.5, 1.5);
    // Status LED dot: (6.5, 6.75)
    draw_circle(hdc, &ctx, 6.5, 6.75, 1.0);
    // Drive bay / slot: (11.0, 6.75) -> (17.5, 6.75)
    draw_line(hdc, &ctx, 11.0, 6.75, 17.5, 6.75);

    // 2. Bottom Server Blade: [3.0, 13.5] -> [21.0, 21.0], corner radius 1.5
    draw_rect(hdc, &ctx, 3.0, 13.5, 18.0, 7.5, 1.5);
    // Status LED dot: (6.5, 17.25)
    draw_circle(hdc, &ctx, 6.5, 17.25, 1.0);
    // Drive bay / slot: (11.0, 17.25) -> (17.5, 17.25)
    draw_line(hdc, &ctx, 11.0, 17.25, 17.5, 17.25);
}
