//! Lucide Folder Vector Icon (Official Lucide `folder.svg`)
//!
//! Vector paths:
//! - `<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 8 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />`

use super::{
    draw_polyline, draw_smooth_polyline, GraphicsScope, IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_folder(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Tabbed folder perimeter
        draw_smooth_polyline(
            &scope,
            &ctx,
            &[
                (4.0, 3.0),
                (8.0, 3.0),
                (9.8, 4.2),
                (11.0, 6.0),
                (20.0, 6.0),
                (22.0, 8.0),
                (22.0, 18.0),
                (20.0, 20.0),
                (4.0, 20.0),
                (2.0, 18.0),
                (2.0, 5.0),
                (4.0, 3.0),
            ],
        );
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_polyline(
            hdc,
            &ctx,
            &[
                (4.0, 3.0),
                (8.0, 3.0),
                (10.5, 6.0),
                (20.0, 6.0),
                (22.0, 8.0),
                (22.0, 18.0),
                (20.0, 20.0),
                (4.0, 20.0),
                (2.0, 18.0),
                (2.0, 5.0),
                (4.0, 3.0),
            ],
        );
    }
}
