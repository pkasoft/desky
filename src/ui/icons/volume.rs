//! Lucide Volume & Volume-X Vector Icons
use super::{IconCanvas, IconContext};

pub fn render_volume(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_polyline(canvas, &[
        (11.0, 5.0),
        (6.0, 9.0),
        (2.0, 9.0),
        (2.0, 15.0),
        (6.0, 15.0),
        (11.0, 19.0),
        (11.0, 5.0),
    ]);
    ctx.draw_arc(canvas, 12.0, 12.0, 5.0, 315.0, 90.0);
    ctx.draw_arc(canvas, 12.0, 12.0, 9.5, 315.0, 90.0);
}

pub fn render_volume_x(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_polyline(canvas, &[
        (11.0, 5.0),
        (6.0, 9.0),
        (2.0, 9.0),
        (2.0, 15.0),
        (6.0, 15.0),
        (11.0, 19.0),
        (11.0, 5.0),
    ]);
    ctx.draw_line(canvas, 16.0, 9.0, 22.0, 15.0);
    ctx.draw_line(canvas, 22.0, 9.0, 16.0, 15.0);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_volume(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_volume(&mut target, &ctx);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_volume_x(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_volume_x(&mut target, &ctx);
}
