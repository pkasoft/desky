//! Lucide Radio Vector Icon
use super::{IconCanvas, IconContext};

pub fn render_radio(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_circle(canvas, 12.0, 12.0, 2.0);
    ctx.draw_arc(canvas, 12.0, 12.0, 4.9, 215.0, 110.0);
    ctx.draw_arc(canvas, 12.0, 12.0, 4.9, 35.0, 110.0);
    ctx.draw_arc(canvas, 12.0, 12.0, 8.5, 215.0, 110.0);
    ctx.draw_arc(canvas, 12.0, 12.0, 8.5, 35.0, 110.0);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_radio(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_radio(&mut target, &ctx);
}
