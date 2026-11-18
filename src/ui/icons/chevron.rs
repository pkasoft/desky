//! Lucide Chevron Left & Right Vector Icons
use super::{IconCanvas, IconContext};

pub fn render_chevron_left(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_polyline(canvas, &[(15.0, 18.0), (9.0, 12.0), (15.0, 6.0)]);
}

pub fn render_chevron_right(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_polyline(canvas, &[(9.0, 18.0), (15.0, 12.0), (9.0, 6.0)]);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_chevron_left(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_chevron_left(&mut target, &ctx);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_chevron_right(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_chevron_right(&mut target, &ctx);
}
