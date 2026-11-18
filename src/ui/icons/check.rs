//! Lucide Check Vector Icon
use super::{IconCanvas, IconContext};

pub fn render_check(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_polyline(canvas, &[(4.0, 12.0), (9.0, 17.0), (20.0, 6.0)]);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_check(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_check(&mut target, &ctx);
}
