//! Lucide Server Vector Icon
use super::{IconCanvas, IconContext};

pub fn render_server(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_rounded_rect(canvas, 2.0, 3.0, 20.0, 8.0, 2.0);
    ctx.draw_rounded_rect(canvas, 2.0, 13.0, 20.0, 8.0, 2.0);
    ctx.draw_line(canvas, 6.0, 7.0, 6.5, 7.0);
    ctx.draw_line(canvas, 6.0, 17.0, 6.5, 17.0);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_server(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_server(&mut target, &ctx);
}
