//! Lucide CPU Vector Icon
use super::{IconCanvas, IconContext};

pub fn render_cpu(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_rounded_rect(canvas, 4.0, 4.0, 16.0, 16.0, 2.0);
    ctx.draw_rounded_rect(canvas, 9.0, 9.0, 6.0, 6.0, 1.0);
    ctx.draw_line(canvas, 9.0, 1.5, 9.0, 4.0);
    ctx.draw_line(canvas, 15.0, 1.5, 15.0, 4.0);
    ctx.draw_line(canvas, 9.0, 20.0, 9.0, 22.5);
    ctx.draw_line(canvas, 15.0, 20.0, 15.0, 22.5);
    ctx.draw_line(canvas, 1.5, 9.0, 4.0, 9.0);
    ctx.draw_line(canvas, 1.5, 15.0, 4.0, 15.0);
    ctx.draw_line(canvas, 20.0, 9.0, 22.5, 9.0);
    ctx.draw_line(canvas, 20.0, 15.0, 22.5, 15.0);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_cpu(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_cpu(&mut target, &ctx);
}
