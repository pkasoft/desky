//! Lucide Volume Speaker & Mute Vector Icons (Official Lucide `volume-2.svg` & `volume-x.svg`)
//!
//! Vector paths:
//! - Speaker horn: `<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" />`
//! - Soundwave 1: `<path d="M15.54 8.46a5 5 0 0 1 0 7.07" />`
//! - Soundwave 2: `<path d="M19.07 4.93a10 10 0 0 1 0 14.14" />`
//! - Mute cross: `<line x1="22" x2="16" y1="9" y2="15" />`, `<line x1="16" x2="22" y1="9" y2="15" />`

use super::{
    draw_line, draw_polyline, draw_smooth_arc, draw_smooth_line, draw_smooth_polyline,
    GraphicsScope, IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_volume(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Speaker horn body polygon
        draw_smooth_polyline(
            &scope,
            &ctx,
            &[
                (11.0, 5.0),
                (6.0, 9.0),
                (2.0, 9.0),
                (2.0, 15.0),
                (6.0, 15.0),
                (11.0, 19.0),
                (11.0, 5.0),
            ],
        );

        // Soundwave 1 (inner arc: center 12.0, 12.0, radius 5.0, from -45° to +45°)
        draw_smooth_arc(&scope, &ctx, 12.0, 12.0, 5.0, 315.0, 90.0);

        // Soundwave 2 (outer arc: center 12.0, 12.0, radius 9.5, from -45° to +45°)
        draw_smooth_arc(&scope, &ctx, 12.0, 12.0, 9.5, 315.0, 90.0);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_polyline(
            hdc,
            &ctx,
            &[
                (11.0, 5.0),
                (6.0, 9.0),
                (2.0, 9.0),
                (2.0, 15.0),
                (6.0, 15.0),
                (11.0, 19.0),
                (11.0, 5.0),
            ],
        );
        draw_line(hdc, &ctx, 15.5, 9.0, 17.0, 12.0);
        draw_line(hdc, &ctx, 17.0, 12.0, 15.5, 15.0);
        draw_line(hdc, &ctx, 19.0, 6.0, 21.0, 12.0);
        draw_line(hdc, &ctx, 21.0, 12.0, 19.0, 18.0);
    }
}

pub unsafe fn draw_volume_x(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Speaker horn body polygon
        draw_smooth_polyline(
            &scope,
            &ctx,
            &[
                (11.0, 5.0),
                (6.0, 9.0),
                (2.0, 9.0),
                (2.0, 15.0),
                (6.0, 15.0),
                (11.0, 19.0),
                (11.0, 5.0),
            ],
        );

        // Mute `X` cross lines
        draw_smooth_line(&scope, &ctx, 16.0, 9.0, 22.0, 15.0);
        draw_smooth_line(&scope, &ctx, 22.0, 9.0, 16.0, 15.0);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_polyline(
            hdc,
            &ctx,
            &[
                (11.0, 5.0),
                (6.0, 9.0),
                (2.0, 9.0),
                (2.0, 15.0),
                (6.0, 15.0),
                (11.0, 19.0),
                (11.0, 5.0),
            ],
        );
        draw_line(hdc, &ctx, 16.0, 9.0, 22.0, 15.0);
        draw_line(hdc, &ctx, 22.0, 9.0, 16.0, 15.0);
    }
}
