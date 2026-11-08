//! Lucide Monitor & Numbered Display Vector Icons (Official Lucide `monitor.svg`)
//!
//! Vector paths:
//! - Screen: `<rect width="20" height="14" x="2" y="3" rx="2" />`
//! - Stem: `<line x1="12" x2="12" y1="17" y2="21" />`
//! - Base: `<line x1="8" x2="16" y1="21" y2="21" />`

use super::{
    draw_line, draw_rect, draw_smooth_line, draw_smooth_polyline, draw_smooth_rounded_rect,
    GraphicsScope, IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_monitor(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Display frame (20x14 at 2,3 with r=2)
        draw_smooth_rounded_rect(&scope, &ctx, 2.0, 3.0, 20.0, 14.0, 2.0);

        // Stand stem
        draw_smooth_line(&scope, &ctx, 12.0, 17.0, 12.0, 21.0);

        // Base foot
        draw_smooth_line(&scope, &ctx, 8.0, 21.0, 16.0, 21.0);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_rect(hdc, &ctx, 2.0, 3.0, 20.0, 14.0, 2.0);
        draw_line(hdc, &ctx, 12.0, 17.0, 12.0, 21.0);
        draw_line(hdc, &ctx, 8.0, 21.0, 16.0, 21.0);
    }
}

pub unsafe fn draw_monitor_num(
    hdc: HDC,
    x: i32,
    y: i32,
    size: i32,
    number: u32,
    color: COLORREF,
    stroke_width: i32,
) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Monitor housing
        draw_smooth_rounded_rect(&scope, &ctx, 2.0, 3.0, 20.0, 14.0, 2.0);
        draw_smooth_line(&scope, &ctx, 12.0, 17.0, 12.0, 21.0);
        draw_smooth_line(&scope, &ctx, 8.0, 21.0, 16.0, 21.0);

        // Centered vector numeral inside screen (y: 6.5 .. 13.5)
        match number {
            1 => {
                draw_smooth_line(&scope, &ctx, 12.0, 6.5, 12.0, 13.5);
                draw_smooth_line(&scope, &ctx, 10.0, 8.5, 12.0, 6.5);
            }
            2 => {
                draw_smooth_polyline(
                    &scope,
                    &ctx,
                    &[
                        (10.0, 8.0),
                        (10.5, 6.5),
                        (13.5, 6.5),
                        (14.0, 8.0),
                        (10.0, 13.5),
                        (14.0, 13.5),
                    ],
                );
            }
            3 => {
                draw_smooth_polyline(
                    &scope,
                    &ctx,
                    &[
                        (10.0, 6.5),
                        (14.0, 6.5),
                        (12.0, 9.8),
                        (14.0, 10.5),
                        (14.0, 12.5),
                        (10.0, 13.5),
                    ],
                );
            }
            _ => {
                draw_smooth_line(&scope, &ctx, 10.0, 6.5, 10.0, 10.0);
                draw_smooth_line(&scope, &ctx, 10.0, 10.0, 14.0, 10.0);
                draw_smooth_line(&scope, &ctx, 14.0, 6.5, 14.0, 13.5);
            }
        }
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_rect(hdc, &ctx, 2.0, 3.0, 20.0, 14.0, 2.0);
        draw_line(hdc, &ctx, 12.0, 17.0, 12.0, 21.0);
        draw_line(hdc, &ctx, 8.0, 21.0, 16.0, 21.0);
        if number == 1 {
            draw_line(hdc, &ctx, 12.0, 6.0, 12.0, 14.0);
            draw_line(hdc, &ctx, 10.0, 8.0, 12.0, 6.0);
        } else if number == 2 {
            draw_line(hdc, &ctx, 10.0, 6.0, 14.0, 6.0);
            draw_line(hdc, &ctx, 14.0, 6.0, 14.0, 10.0);
            draw_line(hdc, &ctx, 10.0, 10.0, 14.0, 10.0);
            draw_line(hdc, &ctx, 10.0, 10.0, 10.0, 14.0);
            draw_line(hdc, &ctx, 10.0, 14.0, 14.0, 14.0);
        } else {
            draw_line(hdc, &ctx, 10.0, 6.0, 14.0, 6.0);
            draw_line(hdc, &ctx, 14.0, 6.0, 14.0, 14.0);
            draw_line(hdc, &ctx, 10.0, 10.0, 14.0, 10.0);
            draw_line(hdc, &ctx, 10.0, 14.0, 14.0, 14.0);
        }
    }
}
