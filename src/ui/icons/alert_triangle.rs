//! Lucide Alert Triangle Vector Icon
use super::{IconCanvas, IconContext};

pub fn render_alert_triangle(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_polyline(canvas, &[
        (12.0, 3.5),
        (21.5, 19.5),
        (2.5, 19.5),
        (12.0, 3.5),
    ]);
    ctx.draw_line(canvas, 12.0, 9.0, 12.0, 13.5);
    ctx.draw_fill_circle(canvas, 12.0, 16.5, 1.2);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_alert_triangle(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_alert_triangle(&mut target, &ctx);
}
