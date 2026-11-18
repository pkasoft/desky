//! Lucide Lock Vector Icon
use super::{IconCanvas, IconContext};

pub fn render_lock(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_rounded_rect(canvas, 3.0, 11.0, 18.0, 11.0, 2.0);
    ctx.draw_arc(canvas, 12.0, 11.0, 5.0, 180.0, 180.0);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_lock(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_lock(&mut target, &ctx);
}
