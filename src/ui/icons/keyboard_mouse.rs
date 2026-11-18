//! Keyboard & Mouse Vector Icon
use super::{IconCanvas, IconContext};

pub fn render_keyboard_mouse(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_rounded_rect(canvas, 1.0, 7.0, 13.0, 11.0, 2.0);
    ctx.draw_line(canvas, 4.0, 10.0, 5.5, 10.0);
    ctx.draw_line(canvas, 7.5, 10.0, 11.0, 10.0);
    ctx.draw_line(canvas, 4.0, 14.0, 11.0, 14.0);
    ctx.draw_rounded_rect(canvas, 16.0, 5.0, 6.5, 14.0, 3.0);
    ctx.draw_line(canvas, 19.25, 5.0, 19.25, 10.0);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_keyboard_mouse(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_keyboard_mouse(&mut target, &ctx);
}
