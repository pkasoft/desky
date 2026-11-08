//! Lucide Microphone & Mute Vector Icons (Official Lucide `mic.svg` & `mic-off.svg`)
//!
//! Vector paths:
//! - Capsule: `<path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />`
//! - Cradle: `<path d="M19 10v2a7 7 0 0 1-14 0v-2" />`
//! - Stand: `<line x1="12" x2="12" y1="19" y2="22" />`
//! - Base: `<line x1="8" x2="16" y1="22" y2="22" />`
//! - Slash (mic-off): `<line x1="2" x2="22" y1="2" y2="22" />`

use super::{
    draw_line, draw_rect, draw_smooth_arc, draw_smooth_line, draw_smooth_rounded_rect,
    GraphicsScope, IconContext, PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_mic(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Microphone capsule (width 6, height 11, rx=3 at 9,2)
        draw_smooth_rounded_rect(&scope, &ctx, 9.0, 2.0, 6.0, 11.0, 3.0);

        // Cradle U-shape: vertical legs from 5,10 to 5,12 and 19,10 to 19,12, bottom arc with r=7 at center 12,12
        draw_smooth_line(&scope, &ctx, 5.0, 10.0, 5.0, 12.0);
        draw_smooth_line(&scope, &ctx, 19.0, 10.0, 19.0, 12.0);
        draw_smooth_arc(&scope, &ctx, 12.0, 12.0, 7.0, 0.0, 180.0);

        // Stand stem
        draw_smooth_line(&scope, &ctx, 12.0, 19.0, 12.0, 22.0);

        // Base foot
        draw_smooth_line(&scope, &ctx, 8.0, 22.0, 16.0, 22.0);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_rect(hdc, &ctx, 9.0, 2.0, 6.0, 11.0, 3.0);
        draw_line(hdc, &ctx, 5.0, 10.0, 5.0, 13.0);
        draw_line(hdc, &ctx, 5.0, 13.0, 12.0, 19.0);
        draw_line(hdc, &ctx, 12.0, 19.0, 19.0, 13.0);
        draw_line(hdc, &ctx, 19.0, 13.0, 19.0, 10.0);
        draw_line(hdc, &ctx, 12.0, 19.0, 12.0, 22.0);
        draw_line(hdc, &ctx, 8.0, 22.0, 16.0, 22.0);
    }
}

pub unsafe fn draw_mic_off(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Microphone capsule outline
        draw_smooth_rounded_rect(&scope, &ctx, 9.0, 2.0, 6.0, 11.0, 3.0);

        // Stand stem & base
        draw_smooth_line(&scope, &ctx, 12.0, 19.0, 12.0, 22.0);
        draw_smooth_line(&scope, &ctx, 8.0, 22.0, 16.0, 22.0);

        // Cradle arc
        draw_smooth_arc(&scope, &ctx, 12.0, 12.0, 7.0, 30.0, 135.0);

        // Diagonal slash through microphone
        draw_smooth_line(&scope, &ctx, 2.5, 2.5, 21.5, 21.5);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_rect(hdc, &ctx, 9.0, 2.0, 6.0, 11.0, 3.0);
        draw_line(hdc, &ctx, 12.0, 19.0, 12.0, 22.0);
        draw_line(hdc, &ctx, 8.0, 22.0, 16.0, 22.0);
        draw_line(hdc, &ctx, 2.0, 2.0, 22.0, 22.0);
    }
}
