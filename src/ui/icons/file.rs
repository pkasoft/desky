//! Lucide File Icon
//! Standard 24x24 vector: document page with folded corner.

use super::{draw_polyline, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_file(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Document border with top-right dog-ear fold:
    // (15, 2) -> (6, 2) -> (6, 22) -> (18, 22) -> (18, 7)
    draw_polyline(
        hdc,
        &ctx,
        &[
            (15.0, 2.0),
            (6.0, 2.0),
            (6.0, 22.0),
            (18.0, 22.0),
            (18.0, 7.0),
        ],
    );

    // Folded corner flap: (14, 2) -> (14, 7) -> (18, 7)
    draw_polyline(hdc, &ctx, &[(14.0, 2.0), (14.0, 7.0), (18.0, 7.0)]);
}
