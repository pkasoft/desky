//! Lucide Settings Cog Vector Icon (Official Lucide `settings.svg`)
//!
//! Vector paths:
//! - 8-tooth mechanical gear perimeter with rounded cogs
//! - Center hub: `<circle cx="12" cy="12" r="3" />`

use super::{
    draw_circle, draw_line, draw_smooth_circle, draw_smooth_polyline, GraphicsScope, IconContext,
    PenScope,
};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_settings(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let sw = stroke_width as f32;

    if let Some(scope) = GraphicsScope::new(hdc, color, sw) {
        // Inner hub hole (radius 3.2 at 12,12)
        draw_smooth_circle(&scope, &ctx, 12.0, 12.0, 3.2);

        // 8-tooth radial gear profile (r_in = 7.5, r_out = 9.8)
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
        // Close polygon loop
        pts.push(pts[0]);

        draw_smooth_polyline(&scope, &ctx, &pts);
    } else {
        let _pen = PenScope::new(hdc, color, stroke_width);
        draw_circle(hdc, &ctx, 12.0, 12.0, 3.5);
        draw_circle(hdc, &ctx, 12.0, 12.0, 8.5);
        let teeth = [
            (12.0, 1.5, 12.0, 4.0),
            (12.0, 20.0, 12.0, 22.5),
            (1.5, 12.0, 4.0, 12.0),
            (20.0, 12.0, 22.5, 12.0),
            (4.5, 4.5, 6.5, 6.5),
            (17.5, 17.5, 19.5, 19.5),
            (4.5, 19.5, 6.5, 17.5),
            (17.5, 6.5, 19.5, 4.5),
        ];
        for (x1, y1, x2, y2) in teeth {
            draw_line(hdc, &ctx, x1, y1, x2, y2);
        }
    }
}
