//! Lucide Microphone Icon
//! Standard 24x24 vector: microphone capsule, cradle, and base.

use super::{draw_line, draw_rect, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_mic(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Mic capsule
    draw_rect(hdc, &ctx, 8.0, 3.0, 8.0, 11.0, 4.0);

    // Cradle U-shape
    draw_line(hdc, &ctx, 5.0, 10.0, 5.0, 13.0);
    draw_line(hdc, &ctx, 5.0, 13.0, 12.0, 18.0);
    draw_line(hdc, &ctx, 12.0, 18.0, 19.0, 13.0);
    draw_line(hdc, &ctx, 19.0, 13.0, 19.0, 10.0);

    // Stand & base
    draw_line(hdc, &ctx, 12.0, 18.0, 12.0, 22.0);
    draw_line(hdc, &ctx, 8.0, 22.0, 16.0, 22.0);
}

pub unsafe fn draw_mic_off(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Mic capsule
    draw_rect(hdc, &ctx, 8.0, 3.0, 8.0, 11.0, 4.0);

    // Stand & base
    draw_line(hdc, &ctx, 12.0, 18.0, 12.0, 22.0);
    draw_line(hdc, &ctx, 8.0, 22.0, 16.0, 22.0);

    // Diagonal slash
    draw_line(hdc, &ctx, 3.0, 3.0, 21.0, 21.0);
}
