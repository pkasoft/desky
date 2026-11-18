//! Lucide Monitor & Numbered Display Vector Icons
use super::{IconCanvas, IconContext};

pub fn render_monitor(canvas: &mut impl IconCanvas, ctx: &IconContext) {
    ctx.draw_rounded_rect(canvas, 2.0, 3.0, 20.0, 14.0, 2.0);
    ctx.draw_line(canvas, 12.0, 17.0, 12.0, 21.0);
    ctx.draw_line(canvas, 8.0, 21.0, 16.0, 21.0);
}

pub fn render_monitor_num(canvas: &mut impl IconCanvas, ctx: &IconContext, number: u32) {
    render_monitor(canvas, ctx);
    match number {
        1 => {
            ctx.draw_line(canvas, 12.0, 6.5, 12.0, 13.5);
            ctx.draw_line(canvas, 10.0, 8.5, 12.0, 6.5);
        }
        2 => {
            ctx.draw_polyline(canvas, &[
                (10.0, 8.0),
                (10.5, 6.5),
                (13.5, 6.5),
                (14.0, 8.0),
                (10.0, 13.5),
                (14.0, 13.5),
            ]);
        }
        3 => {
            ctx.draw_polyline(canvas, &[
                (10.0, 6.5),
                (14.0, 6.5),
                (12.0, 9.8),
                (14.0, 10.5),
                (14.0, 12.5),
                (10.0, 13.5),
            ]);
        }
        _ => {
            ctx.draw_line(canvas, 10.0, 6.5, 10.0, 10.0);
            ctx.draw_line(canvas, 10.0, 10.0, 14.0, 10.0);
            ctx.draw_line(canvas, 14.0, 6.5, 14.0, 13.5);
        }
    }
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_monitor(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_monitor(&mut target, &ctx);
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_monitor_num(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    size: i32,
    number: u32,
    color: windows::Win32::Foundation::COLORREF,
    stroke_width: i32,
) {
    let mut target = super::GdiIconTarget::new(hdc, color, stroke_width as f32);
    let ctx = IconContext::new(x as f32, y as f32, size as f32, color.0, stroke_width as f32);
    render_monitor_num(&mut target, &ctx, number);
}
