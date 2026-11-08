//! Lucide Folder Icon
//! Standard 24x24 vector: folder tab and body contour.

use super::{draw_polyline, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_folder(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Folder outline: (4, 20) -> (20, 20) -> (22, 9) -> (12, 9) -> (10, 5) -> (2, 5) -> (2, 18) -> (4, 20)
    draw_polyline(
        hdc,
        &ctx,
        &[
            (4.0, 20.0),
            (20.0, 20.0),
            (22.0, 9.0),
            (12.0, 9.0),
            (10.0, 5.0),
            (2.0, 5.0),
            (2.0, 18.0),
            (4.0, 20.0),
        ],
    );
}
