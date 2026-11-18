//! Lucide Copy Vector Icon
use super::{IconCanvas, IconContext};

pub fn render_copy(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_rounded_rect(canvas, 8.0, 8.0, 14.0, 14.0, 2.0);
    ctx.draw_polyline(canvas, &[
        (16.0, 4.0),
        (15.5, 2.5),
        (14.0, 2.0),
        (4.0, 2.0),
        (2.5, 2.5),
        (2.0, 4.0),
        (2.0, 14.0),
        (2.5, 15.5),
        (4.0, 16.0),
    ]);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_copy(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_copy(&mut target, &ctx);
}
