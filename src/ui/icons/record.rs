//! Lucide Video / Session Recording Icon
use super::{IconCanvas, IconContext};

pub fn render_record(canvas: &mut impl IconCanvas, ctx: &IconContext, is_recording: bool) {
    if is_recording {
        let rec_red = 0x00DF372B;
        ctx.draw_circle(canvas, 12.0, 12.0, 9.0);
        let (cx, cy) = ctx.pt(12.0, 12.0);
        let r = ctx.dim(5.0);
        canvas.draw_fill_circle(cx, cy, r, rec_red);
    } else {
        ctx.draw_rounded_rect(canvas, 2.0, 6.0, 14.0, 12.0, 2.0);
        ctx.draw_polyline(canvas, &[
            (16.0, 10.0),
            (22.0, 7.0),
            (22.0, 17.0),
            (16.0, 14.0),
            (16.0, 10.0),
        ]);
    }
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_record(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
    is_recording: bool,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_record(&mut target, &ctx, is_recording);
}
