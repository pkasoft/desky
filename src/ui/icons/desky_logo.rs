//! Resky Brand Vector Logo Icon
use super::IconCanvas;

pub fn render_desky_logo(canvas: &mut impl IconCanvas, x: f32, y: f32, size: f32) {
    let r = size * 0.22;
    // Outer Deep Emerald rounded square (#143828) with subtle emerald border (#2d6a4f)
    canvas.fill_rounded_rect(x, y, size, size, r, 0x00143828, 0x002D6A4F, 1.0);

    // Primary Display (#0d1f16 with #52b788 border)
    let margin = size * 0.16;
    let disp_w = (size - margin * 2.0).max(10.0);
    let disp_h = (disp_w * 0.65).max(6.0);
    let disp_x = x + margin;
    let disp_y = y + margin - (if size > 30.0 { 2.0 } else { 1.0 });
    let screen_r = 2.0f32.max(size * 0.08);
    let sw = if size > 30.0 { 2.0 } else { 1.0 };
    canvas.fill_rounded_rect(disp_x, disp_y, disp_w, disp_h, screen_r, 0x000D1F16, 0x0052B788, sw);

    // Display Stand
    let stand_cx = x + size * 0.5;
    let stand_top_y = disp_y + disp_h;
    let stand_h = size * 0.14;
    let base_w = size * 0.38;
    let base_y = stand_top_y + stand_h;
    canvas.draw_line(stand_cx, stand_top_y, stand_cx, base_y, 0x002D6A4F, sw);
    canvas.draw_line(stand_cx - base_w * 0.5, base_y, stand_cx + base_w * 0.5, base_y, 0x0052B788, sw);

    // Screen Center Content: P2P Central Node (#74c69d)
    let cx = disp_x + disp_w * 0.5;
    let cy = disp_y + disp_h * 0.5;
    let node_r = 1.0f32.max(size * 0.04);
    canvas.draw_fill_circle(cx, cy, node_r, 0x0074C69D);

    // High-speed Pulse Waves (Left & Right) if size >= 36
    if size >= 36.0 {
        let wave_r1 = size * 0.09;
        canvas.draw_arc(cx, cy, wave_r1, 120.0, 120.0, 0x0052B788, 1.0);
        canvas.draw_arc(cx, cy, wave_r1, 300.0, 120.0, 0x0052B788, 1.0);
        let wave_r2 = size * 0.15;
        canvas.draw_arc(cx, cy, wave_r2, 120.0, 120.0, 0x0074C69D, 1.0);
        canvas.draw_arc(cx, cy, wave_r2, 300.0, 120.0, 0x0074C69D, 1.0);
    }
}

#[cfg(target_os = "windows")]
pub unsafe fn draw_desky_logo(hdc: windows::Win32::Graphics::Gdi::HDC, x: i32, y: i32, size: i32) {
    let mut target = super::GdiIconTarget::new(hdc, windows::Win32::Foundation::COLORREF(0), 1.0);
    render_desky_logo(&mut target, x as f32, y as f32, size as f32);
}
