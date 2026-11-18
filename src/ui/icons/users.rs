//! Lucide Users Vector Icon
use super::{IconCanvas, IconContext};

pub fn render_users(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_circle(canvas, 9.0, 7.0, 3.5);
    ctx.draw_line(canvas, 4.0, 21.0, 4.0, 19.0);
    ctx.draw_line(canvas, 14.0, 21.0, 14.0, 19.0);
    ctx.draw_arc(canvas, 9.0, 19.0, 5.0, 180.0, 180.0);
    ctx.draw_arc(canvas, 16.0, 6.8, 3.2, 280.0, 140.0);
    ctx.draw_line(canvas, 20.0, 21.0, 20.0, 19.0);
    ctx.draw_arc(canvas, 16.0, 19.0, 4.0, 270.0, 90.0);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_users(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_users(&mut target, &ctx);
}
