//! Lucide Folder Vector Icon
use super::{IconCanvas, IconContext};

pub fn render_folder(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_polyline(canvas, &[
        (4.0, 3.0),
        (8.0, 3.0),
        (9.8, 4.2),
        (11.0, 6.0),
        (20.0, 6.0),
        (22.0, 8.0),
        (22.0, 18.0),
        (20.0, 20.0),
        (4.0, 20.0),
        (2.0, 18.0),
        (2.0, 5.0),
        (4.0, 3.0),
    ]);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_folder(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_folder(&mut target, &ctx);
}
