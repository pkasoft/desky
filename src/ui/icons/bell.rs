//! Lucide Bell Vector Icon
use super::{IconCanvas, IconContext};

pub fn render_bell(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_arc(canvas, 12.0, 8.0, 6.0, 180.0, 180.0);
    ctx.draw_polyline(canvas, &[(6.0, 8.0), (6.0, 11.0), (4.5, 14.5), (3.0, 17.0)]);
    ctx.draw_polyline(canvas, &[(18.0, 8.0), (18.0, 11.0), (19.5, 14.5), (21.0, 17.0)]);
    ctx.draw_line(canvas, 3.0, 17.0, 21.0, 17.0);
    ctx.draw_arc(canvas, 12.0, 19.2, 2.2, 10.0, 160.0);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_bell(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_bell(&mut target, &ctx);
}
