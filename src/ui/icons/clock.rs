//! Lucide Clock Vector Icon
use super::{IconCanvas, IconContext};

pub fn render_clock(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_circle(canvas, 12.0, 12.0, 9.5);
    let hands = [(12.0, 6.5), (12.0, 12.0), (16.0, 14.0)];
    ctx.draw_polyline(canvas, &hands);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_clock(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_clock(&mut target, &ctx);
}
