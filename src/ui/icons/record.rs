//! Lucide Video / Session Recording Icon (Official Lucide `video.svg`)
//!
//! Vector paths:
//! - `<path d="m22 7-6 5 6 5V7z" />`
//! - `<rect width="14" height="12" x="2" y="6" rx="2" ry="2" />`

use super::{
    draw_circle, draw_polyline, draw_rect, draw_smooth_circle, draw_smooth_filled_circle,
    draw_smooth_polyline, draw_smooth_rounded_rect, GraphicsScope, IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_record(
    hdc: HDC,
    x: i32,
    y: i32,
    size: i32,
    color: COLORREF,
    stroke_width: i32,
    is_recording: bool,
) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if is_recording {
        // Red recording badge: Outer circular ring with solid glowing center record dot
        let rec_red = COLORREF(0x002B37DF); // Crimson Red BGR
        if let Some(scope) = GraphicsScope::new(hdc, rec_red, sw) {
            draw_smooth_circle(&scope, &ctx, 12.0, 12.0, 9.0);
            draw_smooth_filled_circle(&scope, &ctx, 12.0, 12.0, 5.0, rec_red);
        } else {
            let _pen = PenScope::new(hdc, rec_red, stroke_width);
            draw_circle(hdc, &ctx, 12.0, 12.0, 9.0);
        }
        return;
    }

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Camera main body: rounded rectangle (x: 2, y: 6, w: 14, h: 12, r: 2)
        draw_smooth_rounded_rect(&scope, &ctx, 2.0, 6.0, 14.0, 12.0, 2.0);

        // Lens cone: polygon (16, 12) -> (22, 7) -> (22, 17) -> (16, 12)
        draw_smooth_polyline(
            &scope,
            &ctx,
            &[
                (16.0, 10.0),
                (22.0, 7.0),
                (22.0, 17.0),
                (16.0, 14.0),
                (16.0, 10.0),
            ],
        );
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_rect(hdc, &ctx, 2.0, 6.0, 14.0, 12.0, 2.0);
        draw_polyline(
            hdc,
            &ctx,
            &[
                (16.0, 10.0),
                (22.0, 7.0),
                (22.0, 17.0),
                (16.0, 14.0),
                (16.0, 10.0),
            ],
        );
    }
}
