//! Lucide More Horizontal Vector Icon
use super::{IconCanvas, IconContext};

pub fn render_more_horizontal(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    let dot_r = ctx.dim(2.2);
    let (cx, cy) = ctx.pt(12.0, 12.0);
    let gap = ctx.dim(6.5);
    canvas.draw_fill_circle(cx - gap, cy, dot_r, ctx.color);
    canvas.draw_fill_circle(cx, cy, dot_r, ctx.color);
    canvas.draw_fill_circle(cx + gap, cy, dot_r, ctx.color);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_more_horizontal(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, 1.0);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, 1.0);
    render_more_horizontal(&mut target, &ctx);
}
