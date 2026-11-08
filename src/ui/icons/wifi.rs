//! Lucide Wifi Connectivity Vector Icon
//! Standard 24x24 vector: concentric signal waves and broadcast transmitter core.

use super::{draw_circle, draw_polyline, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_wifi(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Outer signal wave
    draw_polyline(
        hdc,
        &ctx,
        &[
            (3.0, 8.5),
            (5.5, 6.5),
            (8.5, 5.2),
            (12.0, 4.8),
            (15.5, 5.2),
            (18.5, 6.5),
            (21.0, 8.5),
        ],
    );

    // Middle signal wave
    draw_polyline(
        hdc,
        &ctx,
        &[
            (6.5, 12.5),
            (9.0, 10.5),
            (12.0, 9.8),
            (15.0, 10.5),
            (17.5, 12.5),
        ],
    );

    // Inner signal wave
    draw_polyline(
        hdc,
        &ctx,
        &[
            (9.5, 16.2),
            (12.0, 15.0),
            (14.5, 16.2),
        ],
    );

    // Center broadcast core (dot)
    draw_circle(hdc, &ctx, 12.0, 19.5, 1.2);
}
