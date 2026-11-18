//! Lucide Mic & Mic-Off Vector Icons
use super::{IconCanvas, IconContext};

pub fn render_mic(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_rounded_rect(canvas, 9.0, 2.0, 6.0, 11.0, 3.0);
    ctx.draw_arc(canvas, 12.0, 10.0, 7.0, 0.0, 180.0);
    ctx.draw_line(canvas, 12.0, 17.0, 12.0, 22.0);
    ctx.draw_line(canvas, 8.0, 22.0, 16.0, 22.0);
}

pub fn render_mic_off(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    render_mic(canvas, ctx);
    ctx.draw_line(canvas, 2.0, 2.0, 22.0, 22.0);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_mic(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_mic(&mut target, &ctx);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_mic_off(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_mic_off(&mut target, &ctx);
}
