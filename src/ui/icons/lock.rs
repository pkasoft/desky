//! Lucide Lock Vector Icon (Official Lucide `lock.svg`)
//!
//! Vector paths:
//! - `<rect width="18" height="11" x="3" y="11" rx="2" ry="2" />`
//! - `<path d="M7 11V7a5 5 0 0 1 10 0v4" />`

use super::{
    draw_line, draw_rect, draw_smooth_arc, draw_smooth_line, draw_smooth_rounded_rect,
    GraphicsScope, IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_lock(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Lower lock body (18x11 at 3,11 with r=2)
        draw_smooth_rounded_rect(&scope, &ctx, 3.0, 11.0, 18.0, 11.0, 2.0);

        // Upper shackle (legs from 7,11 to 7,7 and 17,11 to 17,7, arc over top from 180° to 360° with r=5 at center 12,7)
        draw_smooth_line(&scope, &ctx, 7.0, 11.0, 7.0, 7.0);
        draw_smooth_line(&scope, &ctx, 17.0, 11.0, 17.0, 7.0);
        draw_smooth_arc(&scope, &ctx, 12.0, 7.0, 5.0, 180.0, 180.0);

        // Keyhole pin
        draw_smooth_line(&scope, &ctx, 12.0, 15.0, 12.0, 17.5);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_rect(hdc, &ctx, 3.0, 11.0, 18.0, 11.0, 2.0);
        draw_line(hdc, &ctx, 7.0, 11.0, 7.0, 7.0);
        draw_line(hdc, &ctx, 17.0, 11.0, 17.0, 7.0);
        draw_line(hdc, &ctx, 7.0, 7.0, 17.0, 7.0);
        draw_line(hdc, &ctx, 12.0, 15.0, 12.0, 18.0);
    }
}
