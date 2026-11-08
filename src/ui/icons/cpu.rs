//! Lucide CPU Vector Icon (Official Lucide `cpu.svg`)
//!
//! Vector paths:
//! - `<rect width="16" height="16" x="4" y="4" rx="2" />`
//! - `<rect width="6" height="6" x="9" y="9" rx="1" />`
//! - Top pins: `(9, 2)-(9, 4)`, `(15, 2)-(15, 4)`
//! - Bottom pins: `(9, 20)-(9, 22)`, `(15, 20)-(15, 22)`
//! - Left pins: `(2, 9)-(4, 9)`, `(2, 15)-(4, 15)`
//! - Right pins: `(20, 9)-(22, 9)`, `(20, 15)-(22, 15)`

use super::{
    draw_line, draw_rect, draw_smooth_line, draw_smooth_rounded_rect, GraphicsScope, IconContext,
    PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_cpu(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Outer package
        draw_smooth_rounded_rect(&scope, &ctx, 4.0, 4.0, 16.0, 16.0, 2.0);

        // Core die
        draw_smooth_rounded_rect(&scope, &ctx, 9.0, 9.0, 6.0, 6.0, 1.0);

        // Top pins
        draw_smooth_line(&scope, &ctx, 9.0, 1.5, 9.0, 4.0);
        draw_smooth_line(&scope, &ctx, 15.0, 1.5, 15.0, 4.0);

        // Bottom pins
        draw_smooth_line(&scope, &ctx, 9.0, 20.0, 9.0, 22.5);
        draw_smooth_line(&scope, &ctx, 15.0, 20.0, 15.0, 22.5);

        // Left pins
        draw_smooth_line(&scope, &ctx, 1.5, 9.0, 4.0, 9.0);
        draw_smooth_line(&scope, &ctx, 1.5, 15.0, 4.0, 15.0);

        // Right pins
        draw_smooth_line(&scope, &ctx, 20.0, 9.0, 22.5, 9.0);
        draw_smooth_line(&scope, &ctx, 20.0, 15.0, 22.5, 15.0);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_rect(hdc, &ctx, 4.0, 4.0, 16.0, 16.0, 2.0);
        draw_rect(hdc, &ctx, 9.0, 9.0, 6.0, 6.0, 1.0);
        draw_line(hdc, &ctx, 9.0, 1.0, 9.0, 4.0);
        draw_line(hdc, &ctx, 15.0, 1.0, 15.0, 4.0);
        draw_line(hdc, &ctx, 9.0, 20.0, 9.0, 23.0);
        draw_line(hdc, &ctx, 15.0, 20.0, 15.0, 23.0);
        draw_line(hdc, &ctx, 1.0, 9.0, 4.0, 9.0);
        draw_line(hdc, &ctx, 1.0, 15.0, 4.0, 15.0);
        draw_line(hdc, &ctx, 20.0, 9.0, 23.0, 9.0);
        draw_line(hdc, &ctx, 20.0, 15.0, 23.0, 15.0);
    }
}
