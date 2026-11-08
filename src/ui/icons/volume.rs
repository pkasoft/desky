//! Lucide Volume / Speaker Icon
//! Standard 24x24 vector: speaker cone and sound wave / mute X.

use super::{draw_line, draw_polyline, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_volume(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Speaker body
    draw_polyline(
        hdc,
        &ctx,
        &[
            (11.0, 5.0),
            (6.0, 9.0),
            (2.0, 9.0),
            (2.0, 15.0),
            (6.0, 15.0),
            (11.0, 19.0),
            (11.0, 5.0),
        ],
    );

    // Sound wave arcs / chevrons
    draw_polyline(hdc, &ctx, &[(15.0, 9.0), (17.5, 12.0), (15.0, 15.0)]);
    draw_polyline(hdc, &ctx, &[(19.0, 6.0), (22.5, 12.0), (19.0, 18.0)]);
}

pub unsafe fn draw_volume_x(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Speaker body
    draw_polyline(
        hdc,
        &ctx,
        &[
            (11.0, 5.0),
            (6.0, 9.0),
            (2.0, 9.0),
            (2.0, 15.0),
            (6.0, 15.0),
            (11.0, 19.0),
            (11.0, 5.0),
        ],
    );

    // Mute 'X'
    draw_line(hdc, &ctx, 16.0, 9.0, 22.0, 15.0);
    draw_line(hdc, &ctx, 22.0, 9.0, 16.0, 15.0);
}
