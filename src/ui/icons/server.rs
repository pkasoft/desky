//! Lucide Server Vector Icon (Official Lucide `server.svg`)
//! <rect width="20" height="8" x="2" y="2" rx="2" ry="2"/>
//! <rect width="20" height="8" x="2" y="14" rx="2" ry="2"/>
//! <line x1="6" x2="6.01" y1="6" y2="6"/>
//! <line x1="6" x2="6.01" y1="18" y2="18"/>

use super::{
    draw_circle, draw_line, draw_rect, draw_smooth_filled_circle, draw_smooth_line,
    draw_smooth_rounded_rect, GraphicsScope, IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_server(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // High-Quality Anti-Aliased Vector rendering with round caps & subpixel arcs
        // 1. Top Blade
        draw_smooth_rounded_rect(&scope, &ctx, 2.0, 2.0, 20.0, 8.0, 2.0);
        draw_smooth_filled_circle(&scope, &ctx, 6.0, 6.0, 1.1, color);
        draw_smooth_line(&scope, &ctx, 10.0, 6.0, 18.0, 6.0);

        // 2. Bottom Blade
        draw_smooth_rounded_rect(&scope, &ctx, 2.0, 14.0, 20.0, 8.0, 2.0);
        draw_smooth_filled_circle(&scope, &ctx, 6.0, 18.0, 1.1, color);
        draw_smooth_line(&scope, &ctx, 10.0, 18.0, 18.0, 18.0);
    } else {
        // GDI fallback
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_rect(hdc, &ctx, 2.0, 2.0, 20.0, 8.0, 2.0);
        draw_circle(hdc, &ctx, 6.0, 6.0, 1.0);
        draw_line(hdc, &ctx, 10.0, 6.0, 18.0, 6.0);

        draw_rect(hdc, &ctx, 2.0, 14.0, 20.0, 8.0, 2.0);
        draw_circle(hdc, &ctx, 6.0, 18.0, 1.0);
        draw_line(hdc, &ctx, 10.0, 18.0, 18.0, 18.0);
    }
}
