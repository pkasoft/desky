//! Lucide Icon Vector Engine (Unified Cross-Platform Vector Suite)
//! Resolution-independent 24x24 vector icons directly rendered on native canvas.
//! Supports Windows (GDI/GDI+) and Linux (PixelCanvas) with shared vector definitions.

#![allow(unsafe_op_in_unsafe_fn)]
#![allow(unused_imports)]

pub mod alert_triangle;
pub mod arrow_right;
pub mod bell;
pub mod check;
pub mod chevron;
pub mod clock;
pub mod copy;
pub mod cpu;
pub mod desky_logo;
pub mod file;
pub mod folder;
pub mod home;
pub mod info;
pub mod keyboard_mouse;
pub mod lock;
pub mod message_square;
pub mod mic;
pub mod monitor;
pub mod more;
pub mod pen;
pub mod plus;
pub mod radio;
pub mod record;
pub mod refresh;
pub mod search;
pub mod server;
pub mod settings;
pub mod terminal;
pub mod trash;
pub mod users;
pub mod volume;
pub mod wifi;
pub mod x;
pub mod zap;

// Re-export cross-platform vector render functions
pub use alert_triangle::render_alert_triangle;
pub use arrow_right::render_arrow_right;
pub use bell::render_bell;
pub use check::render_check;
pub use chevron::{render_chevron_left, render_chevron_right};
pub use clock::render_clock;
pub use copy::render_copy;
pub use cpu::render_cpu;
pub use desky_logo::render_desky_logo;
pub use file::render_file;
pub use folder::render_folder;
pub use home::render_home;
pub use info::render_info;
pub use keyboard_mouse::render_keyboard_mouse;
pub use lock::render_lock;
pub use message_square::render_message_square;
pub use mic::{render_mic, render_mic_off};
pub use monitor::{render_monitor, render_monitor_num};
pub use more::render_more_horizontal;
pub use pen::render_pen;
pub use plus::render_plus;
pub use radio::render_radio;
pub use record::render_record;
pub use refresh::render_refresh;
pub use search::render_search;
pub use server::render_server;
pub use settings::render_settings;
pub use terminal::render_terminal;
pub use trash::render_trash;
pub use users::render_users;
pub use volume::{render_volume, render_volume_x};
pub use wifi::render_wifi;
pub use x::render_x;
pub use zap::render_zap;

// Re-export Windows native GDI draw functions for Windows callers
#[cfg(target_os = "windows")]
pub use alert_triangle::draw_alert_triangle;
#[cfg(target_os = "windows")]
pub use arrow_right::draw_arrow_right;
#[cfg(target_os = "windows")]
pub use bell::draw_bell;
#[cfg(target_os = "windows")]
pub use check::draw_check;
#[cfg(target_os = "windows")]
pub use chevron::{draw_chevron_left, draw_chevron_right};
#[cfg(target_os = "windows")]
pub use clock::draw_clock;
#[cfg(target_os = "windows")]
pub use copy::draw_copy;
#[cfg(target_os = "windows")]
pub use cpu::draw_cpu;
#[cfg(target_os = "windows")]
pub use desky_logo::draw_desky_logo;
#[cfg(target_os = "windows")]
pub use file::draw_file;
#[cfg(target_os = "windows")]
pub use folder::draw_folder;
#[cfg(target_os = "windows")]
pub use home::draw_home;
#[cfg(target_os = "windows")]
pub use info::draw_info;
#[cfg(target_os = "windows")]
pub use keyboard_mouse::draw_keyboard_mouse;
#[cfg(target_os = "windows")]
pub use lock::draw_lock;
#[cfg(target_os = "windows")]
pub use message_square::draw_message_square;
#[cfg(target_os = "windows")]
pub use mic::{draw_mic, draw_mic_off};
#[cfg(target_os = "windows")]
pub use monitor::{draw_monitor, draw_monitor_num};
#[cfg(target_os = "windows")]
pub use more::draw_more_horizontal;
#[cfg(target_os = "windows")]
pub use pen::draw_pen;
#[cfg(target_os = "windows")]
pub use plus::draw_plus;
#[cfg(target_os = "windows")]
pub use radio::draw_radio;
#[cfg(target_os = "windows")]
pub use record::draw_record;
#[cfg(target_os = "windows")]
pub use refresh::draw_refresh;
#[cfg(target_os = "windows")]
pub use search::draw_search;
#[cfg(target_os = "windows")]
pub use server::draw_server;
#[cfg(target_os = "windows")]
pub use settings::draw_settings;
#[cfg(target_os = "windows")]
pub use terminal::draw_terminal;
#[cfg(target_os = "windows")]
pub use trash::draw_trash;
#[cfg(target_os = "windows")]
pub use users::draw_users;
#[cfg(target_os = "windows")]
pub use volume::{draw_volume, draw_volume_x};
#[cfg(target_os = "windows")]
pub use wifi::draw_wifi;
#[cfg(target_os = "windows")]
pub use x::draw_x;
#[cfg(target_os = "windows")]
pub use zap::draw_zap;

/// Abstract target canvas for resolution-independent vector icon rendering
pub trait IconCanvas {
    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, color: u32, stroke_width: f32);
    fn draw_polyline(&mut self, points: &[(f32, f32)], color: u32, stroke_width: f32) {
        for w in points.windows(2) {
            self.draw_line(w[0].0, w[0].1, w[1].0, w[1].1, color, stroke_width);
        }
    }
    fn draw_circle(&mut self, cx: f32, cy: f32, r: f32, color: u32, stroke_width: f32);
    fn draw_fill_circle(&mut self, cx: f32, cy: f32, r: f32, color: u32);
    fn draw_arc(&mut self, cx: f32, cy: f32, r: f32, start_deg: f32, sweep_deg: f32, color: u32, stroke_width: f32) {
        let steps = ((r * sweep_deg.abs().to_radians()).ceil() as usize).clamp(8, 36);
        let start_rad = start_deg.to_radians();
        let sweep_rad = sweep_deg.to_radians();
        let mut prev_x = cx + r * start_rad.cos();
        let mut prev_y = cy + r * start_rad.sin();
        for i in 1..=steps {
            let angle = start_rad + sweep_rad * (i as f32 / steps as f32);
            let next_x = cx + r * angle.cos();
            let next_y = cy + r * angle.sin();
            self.draw_line(prev_x, prev_y, next_x, next_y, color, stroke_width);
            prev_x = next_x;
            prev_y = next_y;
        }
    }
    fn draw_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, color: u32, stroke_width: f32) {
        let r = r.min(w * 0.5).min(h * 0.5);
        if r <= 0.5 {
            let pts = [(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)];
            self.draw_polyline(&pts, color, stroke_width);
        } else {
            self.draw_line(x + r, y, x + w - r, y, color, stroke_width);
            self.draw_arc(x + w - r, y + r, r, 270.0, 90.0, color, stroke_width);
            self.draw_line(x + w, y + r, x + w, y + h - r, color, stroke_width);
            self.draw_arc(x + w - r, y + h - r, r, 0.0, 90.0, color, stroke_width);
            self.draw_line(x + w - r, y + h, x + r, y + h, color, stroke_width);
            self.draw_arc(x + r, y + h - r, r, 90.0, 90.0, color, stroke_width);
            self.draw_line(x, y + h - r, x, y + r, color, stroke_width);
            self.draw_arc(x + r, y + r, r, 180.0, 90.0, color, stroke_width);
        }
    }
    fn fill_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, fill_color: u32, border_color: u32, stroke_width: f32) {
        self.fill_rect(x, y, w, h, fill_color);
        self.draw_rounded_rect(x, y, w, h, r, border_color, stroke_width);
    }
    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: u32) {
        let pts = [(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)];
        self.draw_polyline(&pts, color, 1.0);
    }
}

/// 24x24 Vector Coordinate Context
#[derive(Clone, Copy, Debug)]
pub struct IconContext {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub color: u32,
    pub stroke_width: f32,
}

impl IconContext {
    #[inline]
    pub fn new(x: f32, y: f32, size: f32, color: u32, stroke_width: f32) -> Self {
        Self { x, y, size, color, stroke_width }
    }

    #[inline]
    pub fn pt(&self, vx: f32, vy: f32) -> (f32, f32) {
        let scale = self.size / 24.0;
        (self.x + vx * scale, self.y + vy * scale)
    }

    #[inline]
    pub fn dim(&self, v: f32) -> f32 {
        v * (self.size / 24.0)
    }

    #[inline]
    pub fn draw_line(&self, canvas: &mut impl IconCanvas, x1: f32, y1: f32, x2: f32, y2: f32) {
        let (px1, py1) = self.pt(x1, y1);
        let (px2, py2) = self.pt(x2, y2);
        canvas.draw_line(px1, py1, px2, py2, self.color, self.stroke_width);
    }

    #[inline]
    pub fn draw_polyline(&self, canvas: &mut impl IconCanvas, points: &[(f32, f32)]) {
        let pts: Vec<(f32, f32)> = points.iter().map(|&(x, y)| self.pt(x, y)).collect();
        canvas.draw_polyline(&pts, self.color, self.stroke_width);
    }

    #[inline]
    pub fn draw_circle(&self, canvas: &mut impl IconCanvas, cx: f32, cy: f32, r: f32) {
        let (pcx, pcy) = self.pt(cx, cy);
        let pr = self.dim(r);
        canvas.draw_circle(pcx, pcy, pr, self.color, self.stroke_width);
    }

    #[inline]
    pub fn draw_fill_circle(&self, canvas: &mut impl IconCanvas, cx: f32, cy: f32, r: f32) {
        let (pcx, pcy) = self.pt(cx, cy);
        let pr = self.dim(r);
        canvas.draw_fill_circle(pcx, pcy, pr, self.color);
    }

    #[inline]
    pub fn draw_arc(&self, canvas: &mut impl IconCanvas, cx: f32, cy: f32, r: f32, start_deg: f32, sweep_deg: f32) {
        let (pcx, pcy) = self.pt(cx, cy);
        let pr = self.dim(r);
        canvas.draw_arc(pcx, pcy, pr, start_deg, sweep_deg, self.color, self.stroke_width);
    }

    #[inline]
    pub fn draw_rounded_rect(&self, canvas: &mut impl IconCanvas, x: f32, y: f32, w: f32, h: f32, r: f32) {
        let (px, py) = self.pt(x, y);
        let pw = self.dim(w);
        let ph = self.dim(h);
        let pr = self.dim(r);
        canvas.draw_rounded_rect(px, py, pw, ph, pr, self.color, self.stroke_width);
    }
}

// =========================================================================
// Windows Native GDI / GDI+ Vector Surface Adapter
// =========================================================================

#[cfg(target_os = "windows")]
use std::sync::Once;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{COLORREF, POINT};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CreatePen, CreateSolidBrush, DeleteObject, Ellipse, GetStockObject, LineTo, MoveToEx, Polyline, RoundRect,
    SelectObject, HDC, HGDIOBJ, NULL_BRUSH, PS_SOLID,
};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::GdiPlus::{
    GdiplusStartup, GdiplusStartupInput, GdipCreateFromHDC, GdipDeleteGraphics,
    GdipDeletePen, GdipSetPenEndCap, GdipSetPenLineJoin, GdipSetPenStartCap,
    GdipSetSmoothingMode, GdipCreatePen1, GdipDrawLine, GdipDrawLines,
    GdipDrawArc, GdipDrawEllipse, GdipFillEllipse, GdipCreateSolidFill, GdipDeleteBrush,
    LineCapRound, LineJoinRound, PointF, SmoothingModeHighQuality,
    UnitPixel, GpGraphics, GpPen,
};

#[cfg(target_os = "windows")]
static GDI_PLUS_INIT: Once = Once::new();

#[cfg(target_os = "windows")]
pub fn ensure_gdiplus() {
    GDI_PLUS_INIT.call_once(|| unsafe {
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: 0,
            SuppressBackgroundThread: false.into(),
            SuppressExternalCodecs: false.into(),
        };
        let mut token = 0;
        let _ = GdiplusStartup(&mut token, &input, std::ptr::null_mut());
    });
}

#[cfg(target_os = "windows")]
pub struct GdiIconTarget {
    pub hdc: HDC,
    pub graphics: *mut GpGraphics,
    pub pen: *mut GpPen,
    pub current_color: u32,
    pub current_sw: f32,
    pub gdi_pen: Option<HGDIOBJ>,
    pub old_pen: Option<HGDIOBJ>,
    pub old_brush: Option<HGDIOBJ>,
}

#[cfg(target_os = "windows")]
impl GdiIconTarget {
    pub unsafe fn new(hdc: HDC, color: COLORREF, stroke_width: f32) -> Self {
        ensure_gdiplus();
        let mut graphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut graphics) == windows::Win32::Graphics::GdiPlus::Ok {
            let _ = GdipSetSmoothingMode(graphics, SmoothingModeHighQuality);
            let r = (color.0 & 0xFF) as u32;
            let g = ((color.0 >> 8) & 0xFF) as u32;
            let b = ((color.0 >> 16) & 0xFF) as u32;
            let argb = 0xFF000000 | (r << 16) | (g << 8) | b;
            let mut pen = std::ptr::null_mut();
            if GdipCreatePen1(argb, stroke_width, UnitPixel, &mut pen) == windows::Win32::Graphics::GdiPlus::Ok {
                let _ = GdipSetPenStartCap(pen, LineCapRound);
                let _ = GdipSetPenEndCap(pen, LineCapRound);
                let _ = GdipSetPenLineJoin(pen, LineJoinRound);
                return Self {
                    hdc,
                    graphics,
                    pen,
                    current_color: color.0,
                    current_sw: stroke_width,
                    gdi_pen: None,
                    old_pen: None,
                    old_brush: None,
                };
            }
            let _ = GdipDeleteGraphics(graphics);
        }
        let pen = CreatePen(PS_SOLID, stroke_width.max(1.0) as i32, color);
        let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));
        let null_brush = GetStockObject(NULL_BRUSH);
        let old_brush = SelectObject(hdc, null_brush);
        Self {
            hdc,
            graphics: std::ptr::null_mut(),
            pen: std::ptr::null_mut(),
            current_color: color.0,
            current_sw: stroke_width,
            gdi_pen: Some(HGDIOBJ(pen.0)),
            old_pen: Some(old_pen),
            old_brush: Some(old_brush),
        }
    }

    unsafe fn ensure_pen(&mut self, color: u32, stroke_width: f32) {
        if !self.pen.is_null() && (self.current_color != color || (self.current_sw - stroke_width).abs() > 0.01) {
            let _ = GdipDeletePen(self.pen);
            self.pen = std::ptr::null_mut();
            let r = (color & 0xFF) as u32;
            let g = ((color >> 8) & 0xFF) as u32;
            let b = ((color >> 16) & 0xFF) as u32;
            let argb = 0xFF000000 | (r << 16) | (g << 8) | b;
            if GdipCreatePen1(argb, stroke_width, UnitPixel, &mut self.pen) == windows::Win32::Graphics::GdiPlus::Ok {
                let _ = GdipSetPenStartCap(self.pen, LineCapRound);
                let _ = GdipSetPenEndCap(self.pen, LineCapRound);
                let _ = GdipSetPenLineJoin(self.pen, LineJoinRound);
                self.current_color = color;
                self.current_sw = stroke_width;
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for GdiIconTarget {
    fn drop(&mut self) {
        unsafe {
            if !self.pen.is_null() {
                let _ = GdipDeletePen(self.pen);
            }
            if !self.graphics.is_null() {
                let _ = GdipDeleteGraphics(self.graphics);
            }
            if let Some(pen) = self.gdi_pen {
                if let Some(old) = self.old_pen {
                    let _ = SelectObject(self.hdc, old);
                }
                if let Some(old) = self.old_brush {
                    let _ = SelectObject(self.hdc, old);
                }
                let _ = DeleteObject(pen);
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl IconCanvas for GdiIconTarget {
    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, color: u32, stroke_width: f32) {
        unsafe {
            if !self.graphics.is_null() {
                self.ensure_pen(color, stroke_width);
                if !self.pen.is_null() {
                    let _ = GdipDrawLine(self.graphics, self.pen, x1, y1, x2, y2);
                }
            } else {
                let _ = MoveToEx(self.hdc, x1.round() as i32, y1.round() as i32, None);
                let _ = LineTo(self.hdc, x2.round() as i32, y2.round() as i32);
            }
        }
    }

    fn draw_polyline(&mut self, points: &[(f32, f32)], color: u32, stroke_width: f32) {
        if points.len() < 2 {
            return;
        }
        unsafe {
            if !self.graphics.is_null() {
                self.ensure_pen(color, stroke_width);
                if !self.pen.is_null() {
                    let pts: Vec<PointF> = points.iter().map(|&(x, y)| PointF { X: x, Y: y }).collect();
                    let _ = GdipDrawLines(self.graphics, self.pen, pts.as_ptr(), pts.len() as i32);
                }
            } else {
                let pts: Vec<POINT> = points.iter().map(|&(x, y)| POINT { x: x.round() as i32, y: y.round() as i32 }).collect();
                let _ = Polyline(self.hdc, &pts);
            }
        }
    }

    fn draw_circle(&mut self, cx: f32, cy: f32, r: f32, color: u32, stroke_width: f32) {
        unsafe {
            if !self.graphics.is_null() {
                self.ensure_pen(color, stroke_width);
                if !self.pen.is_null() {
                    let _ = GdipDrawEllipse(self.graphics, self.pen, cx - r, cy - r, r * 2.0, r * 2.0);
                }
            } else {
                let _ = Ellipse(self.hdc, (cx - r).round() as i32, (cy - r).round() as i32, (cx + r).round() as i32, (cy + r).round() as i32);
            }
        }
    }

    fn draw_fill_circle(&mut self, cx: f32, cy: f32, r: f32, color: u32) {
        unsafe {
            if !self.graphics.is_null() {
                let r_c = (color & 0xFF) as u32;
                let g_c = ((color >> 8) & 0xFF) as u32;
                let b_c = ((color >> 16) & 0xFF) as u32;
                let argb = 0xFF000000 | (r_c << 16) | (g_c << 8) | b_c;
                let mut brush = std::ptr::null_mut();
                if GdipCreateSolidFill(argb, &mut brush) == windows::Win32::Graphics::GdiPlus::Ok {
                    let b_ptr = brush as *mut windows::Win32::Graphics::GdiPlus::GpBrush;
                    let _ = GdipFillEllipse(self.graphics, b_ptr, cx - r, cy - r, r * 2.0, r * 2.0);
                    let _ = GdipDeleteBrush(b_ptr);
                }
            } else {
                let brush = CreateSolidBrush(COLORREF(color));
                let old_b = SelectObject(self.hdc, HGDIOBJ(brush.0));
                let _ = Ellipse(self.hdc, (cx - r).round() as i32, (cy - r).round() as i32, (cx + r).round() as i32, (cy + r).round() as i32);
                let _ = SelectObject(self.hdc, old_b);
                let _ = DeleteObject(HGDIOBJ(brush.0));
            }
        }
    }

    fn draw_arc(&mut self, cx: f32, cy: f32, r: f32, start_deg: f32, sweep_deg: f32, color: u32, stroke_width: f32) {
        unsafe {
            if !self.graphics.is_null() {
                self.ensure_pen(color, stroke_width);
                if !self.pen.is_null() {
                    let _ = GdipDrawArc(self.graphics, self.pen, cx - r, cy - r, r * 2.0, r * 2.0, start_deg, sweep_deg);
                }
            } else {
                let steps = 12;
                let start_rad = start_deg.to_radians();
                let sweep_rad = sweep_deg.to_radians();
                let mut prev_x = cx + r * start_rad.cos();
                let mut prev_y = cy + r * start_rad.sin();
                for i in 1..=steps {
                    let angle = start_rad + sweep_rad * (i as f32 / steps as f32);
                    let next_x = cx + r * angle.cos();
                    let next_y = cy + r * angle.sin();
                    self.draw_line(prev_x, prev_y, next_x, next_y, color, stroke_width);
                    prev_x = next_x;
                    prev_y = next_y;
                }
            }
        }
    }

    fn draw_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, color: u32, stroke_width: f32) {
        let pr = r.min(w * 0.5).min(h * 0.5);
        if pr <= 0.5 {
            let pts = [(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)];
            self.draw_polyline(&pts, color, stroke_width);
            return;
        }
        unsafe {
            if !self.graphics.is_null() {
                self.ensure_pen(color, stroke_width);
                if !self.pen.is_null() {
                    let d = pr * 2.0;
                    let _ = GdipDrawLine(self.graphics, self.pen, x + pr, y, x + w - pr, y);
                    let _ = GdipDrawArc(self.graphics, self.pen, x + w - d, y, d, d, 270.0, 90.0);
                    let _ = GdipDrawLine(self.graphics, self.pen, x + w, y + pr, x + w, y + h - pr);
                    let _ = GdipDrawArc(self.graphics, self.pen, x + w - d, y + h - d, d, d, 0.0, 90.0);
                    let _ = GdipDrawLine(self.graphics, self.pen, x + w - pr, y + h, x + pr, y + h);
                    let _ = GdipDrawArc(self.graphics, self.pen, x, y + h - d, d, d, 90.0, 90.0);
                    let _ = GdipDrawLine(self.graphics, self.pen, x, y + h - pr, x, y + pr);
                    let _ = GdipDrawArc(self.graphics, self.pen, x, y, d, d, 180.0, 90.0);
                }
            } else {
                let rad = (pr * 2.0).round() as i32;
                let _ = RoundRect(self.hdc, x.round() as i32, y.round() as i32, (x + w).round() as i32, (y + h).round() as i32, rad, rad);
            }
        }
    }

    fn fill_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, fill_color: u32, border_color: u32, stroke_width: f32) {
        unsafe {
            let brush = CreateSolidBrush(COLORREF(fill_color));
            let pen = CreatePen(PS_SOLID, stroke_width.max(1.0) as i32, COLORREF(border_color));
            let old_b = SelectObject(self.hdc, HGDIOBJ(brush.0));
            let old_p = SelectObject(self.hdc, HGDIOBJ(pen.0));
            let rad = (r * 2.0).round() as i32;
            let _ = RoundRect(self.hdc, x.round() as i32, y.round() as i32, (x + w).round() as i32, (y + h).round() as i32, rad, rad);
            let _ = SelectObject(self.hdc, old_p);
            let _ = SelectObject(self.hdc, old_b);
            let _ = DeleteObject(HGDIOBJ(pen.0));
            let _ = DeleteObject(HGDIOBJ(brush.0));
        }
    }
}
