//! Lucide Zap (Lightning Bolt) Vector Icon
//! Standard 24x24 vector: high-speed instant direct P2P link symbol.

use super::{draw_polyline, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_zap(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    draw_polyline(
        hdc,
        &ctx,
        &[
            (13.0, 2.0),
            (4.0, 13.0),
            (12.0, 13.0),
            (11.0, 22.0),
            (20.0, 11.0),
            (12.0, 11.0),
            (13.0, 2.0),
        ],
    );
}
