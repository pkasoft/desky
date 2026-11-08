//! Lucide WiFi Signal Vector Icon (Official Lucide `wifi.svg`)
//!
//! Vector paths:
//! - `<path d="M12 20h.01" />`
//! - `<path d="M2 8.82a15 15 0 0 1 20 0" />`
//! - `<path d="M5 12.86a10 10 0 0 1 14 0" />`
//! - `<path d="M8.5 16.43a5 5 0 0 1 7 0" />`

use super::{
    draw_arc, draw_circle, draw_smooth_arc, draw_smooth_filled_circle, GraphicsScope, IconContext,
    PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_wifi(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Base focal dot
        draw_smooth_filled_circle(&scope, &ctx, 12.0, 19.5, 1.3, color);

        // Wave 1 (inner arc, r=4.5 at center 12, 20 from 220° to 320° => 100° sweep)
        draw_smooth_arc(&scope, &ctx, 12.0, 19.5, 4.2, 225.0, 90.0);

        // Wave 2 (middle arc, r=8.5 at center 12, 20 from 220° to 320°)
        draw_smooth_arc(&scope, &ctx, 12.0, 19.5, 8.2, 225.0, 90.0);

        // Wave 3 (outer arc, r=12.5 at center 12, 20 from 220° to 320°)
        draw_smooth_arc(&scope, &ctx, 12.0, 19.5, 12.2, 225.0, 90.0);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_circle(hdc, &ctx, 12.0, 19.5, 1.2);
        draw_arc(hdc, &ctx, 12.0, 19.5, 4.0, 8.5, 16.5, 15.5, 16.5);
        draw_arc(hdc, &ctx, 12.0, 19.5, 8.0, 5.0, 13.0, 19.0, 13.0);
        draw_arc(hdc, &ctx, 12.0, 19.5, 12.0, 2.0, 9.0, 22.0, 9.0);
    }
}
