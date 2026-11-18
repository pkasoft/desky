//! Lucide Settings Vector Icon
use super::{IconCanvas, IconContext};

pub fn render_settings(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_circle(canvas, 12.0, 12.0, 3.2);

    let cx = 12.0f32;
    let cy = 12.0f32;
    let r_in = 7.3f32;
    let r_out = 9.8f32;

    let mut pts = Vec::with_capacity(33);
    for i in 0..8 {
        let base_deg = (i as f32) * 45.0;
        let a1 = (base_deg - 14.0).to_radians();
        let a2 = (base_deg - 8.0).to_radians();
        let a3 = (base_deg + 8.0).to_radians();
        let a4 = (base_deg + 14.0).to_radians();

        pts.push((cx + r_in * a1.cos(), cy + r_in * a1.sin()));
        pts.push((cx + r_out * a2.cos(), cy + r_out * a2.sin()));
        pts.push((cx + r_out * a3.cos(), cy + r_out * a3.sin()));
        pts.push((cx + r_in * a4.cos(), cy + r_in * a4.sin()));
    }
    pts.push(pts[0]);
    ctx.draw_polyline(canvas, &pts);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_settings(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_settings(&mut target, &ctx);
}
