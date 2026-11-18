//! Lucide Icon Vector Engine (Zero External Dependencies)
//! High-quality, resolution-independent 24x24 vector icons directly rendered on native canvas.

#![allow(unsafe_op_in_unsafe_fn)]

pub mod arrow_right;
pub mod bell;
pub mod check;
pub mod copy;
pub mod cpu;
pub mod file;
pub mod folder;
pub mod home;
pub mod info;
pub mod lock;
pub mod mic;
pub mod monitor;
pub mod plus;
pub mod radio;
pub mod refresh;
pub mod search;
pub mod server;
pub mod settings;
pub mod terminal;
pub mod trash;
pub mod volume;
pub mod wifi;
pub mod x;
pub mod pen;
pub mod record;
pub mod zap;
pub mod alert_triangle;
pub mod message_square;
pub mod more;
pub mod chevron;
pub mod keyboard_mouse;
pub mod desky_logo;

pub use arrow_right::draw_arrow_right;
pub use alert_triangle::draw_alert_triangle;
pub use message_square::draw_message_square;
pub use more::draw_more_horizontal;
pub use chevron::{draw_chevron_left, draw_chevron_right};
pub use bell::draw_bell;
pub use check::draw_check;
pub use copy::draw_copy;
pub use cpu::draw_cpu;
pub use file::draw_file;
pub use folder::draw_folder;
pub use home::draw_home;
pub use info::draw_info;
pub use lock::draw_lock;
pub use mic::{draw_mic, draw_mic_off};
pub use monitor::{draw_monitor, draw_monitor_num};
pub use plus::draw_plus;
pub use radio::draw_radio;
pub use refresh::draw_refresh;
pub use search::draw_search;
pub use server::draw_server;
pub use settings::draw_settings;
pub use terminal::draw_terminal;
pub use trash::draw_trash;
pub use volume::{draw_volume, draw_volume_x};
pub use wifi::draw_wifi;
pub use x::draw_x;
pub use pen::draw_pen;
pub use record::draw_record;
pub use zap::draw_zap;
pub use keyboard_mouse::draw_keyboard_mouse;
pub use desky_logo::draw_desky_logo;

use std::sync::Once;
use windows::Win32::Foundation::{COLORREF, POINT};
use windows::Win32::Graphics::Gdi::{
    Arc, CreatePen, DeleteObject, Ellipse, GetStockObject, LineTo, MoveToEx, Polyline, RoundRect,
    SelectObject, HDC, HGDIOBJ, NULL_BRUSH, PS_SOLID,
};
use windows::Win32::Graphics::GdiPlus::{
    GdiplusStartup, GdiplusStartupInput, GdipCreateFromHDC, GdipDeleteGraphics,
    GdipDeletePen, GdipSetPenEndCap, GdipSetPenLineJoin, GdipSetPenStartCap,
    GdipSetSmoothingMode, GdipCreatePen1, GdipDrawLine, GdipDrawLines,
    GdipDrawArc, GdipDrawEllipse, GdipFillEllipse, GdipCreateSolidFill, GdipDeleteBrush,
    LineCapRound, LineJoinRound, PointF, SmoothingModeHighQuality,
    UnitPixel, GpGraphics, GpPen,
};

static GDI_PLUS_INIT: Once = Once::new();

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

pub struct GraphicsScope {
    pub graphics: *mut GpGraphics,
    pub pen: *mut GpPen,
}

impl GraphicsScope {
    pub unsafe fn new(hdc: HDC, color: COLORREF, stroke_width: f32) -> Option<Self> {
        ensure_gdiplus();
        let mut graphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut graphics) != windows::Win32::Graphics::GdiPlus::Ok {
            return None;
        }
        let _ = GdipSetSmoothingMode(graphics, SmoothingModeHighQuality);

        let r = (color.0 & 0xFF) as u32;
        let g = ((color.0 >> 8) & 0xFF) as u32;
        let b = ((color.0 >> 16) & 0xFF) as u32;
        let argb = 0xFF000000 | (r << 16) | (g << 8) | b;

        let mut pen = std::ptr::null_mut();
        if GdipCreatePen1(argb, stroke_width, UnitPixel, &mut pen) != windows::Win32::Graphics::GdiPlus::Ok {
            let _ = GdipDeleteGraphics(graphics);
            return None;
        }

        let _ = GdipSetPenStartCap(pen, LineCapRound);
        let _ = GdipSetPenEndCap(pen, LineCapRound);
        let _ = GdipSetPenLineJoin(pen, LineJoinRound);

        Some(Self { graphics, pen })
    }
}

impl Drop for GraphicsScope {
    fn drop(&mut self) {
        unsafe {
            if !self.pen.is_null() {
                let _ = GdipDeletePen(self.pen);
            }
            if !self.graphics.is_null() {
                let _ = GdipDeleteGraphics(self.graphics);
            }
        }
    }
}

/// 24x24 Vector Coordinate Mapper
pub struct IconContext {
    pub x: i32,
    pub y: i32,
    pub size: i32,
}

impl IconContext {
    pub fn new(x: i32, y: i32, size: i32) -> Self {
        Self { x, y, size }
    }

    #[inline]
    pub fn pt(&self, vx: f32, vy: f32) -> (i32, i32) {
        let scale = self.size as f32 / 24.0;
        (
            self.x + (vx * scale).round() as i32,
            self.y + (vy * scale).round() as i32,
        )
    }

    #[inline]
    pub fn fpt(&self, vx: f32, vy: f32) -> (f32, f32) {
        let scale = self.size as f32 / 24.0;
        (
            self.x as f32 + vx * scale,
            self.y as f32 + vy * scale,
        )
    }

    #[inline]
    pub fn dim(&self, v: f32) -> i32 {
        let scale = self.size as f32 / 24.0;
        (v * scale).round().max(1.0) as i32
    }
}

pub struct PenScope {
    pub hdc: HDC,
    pub old_pen: HGDIOBJ,
    pub old_brush: HGDIOBJ,
    pub pen: HGDIOBJ,
}

impl PenScope {
    pub unsafe fn new(hdc: HDC, color: COLORREF, stroke_width: i32) -> Self {
        let pen = CreatePen(PS_SOLID, stroke_width.max(1), color);
        let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));
        let null_brush = GetStockObject(NULL_BRUSH);
        let old_brush = SelectObject(hdc, null_brush);
        Self {
            hdc,
            old_pen,
            old_brush,
            pen: HGDIOBJ(pen.0),
        }
    }
}

impl Drop for PenScope {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.hdc, self.old_pen);
            let _ = SelectObject(self.hdc, self.old_brush);
            let _ = DeleteObject(self.pen);
        }
    }
}

#[inline]
pub unsafe fn draw_line(hdc: HDC, ctx: &IconContext, x1: f32, y1: f32, x2: f32, y2: f32) {
    let (px1, py1) = ctx.pt(x1, y1);
    let (px2, py2) = ctx.pt(x2, y2);
    let _ = MoveToEx(hdc, px1, py1, None);
    let _ = LineTo(hdc, px2, py2);
}

#[inline]
pub unsafe fn draw_polyline(hdc: HDC, ctx: &IconContext, points: &[(f32, f32)]) {
    if points.len() < 2 {
        return;
    }
    let pts: Vec<POINT> = points
        .iter()
        .map(|&(vx, vy)| {
            let (px, py) = ctx.pt(vx, vy);
            POINT { x: px, y: py }
        })
        .collect();
    let _ = Polyline(hdc, &pts);
}

#[inline]
pub unsafe fn draw_circle(hdc: HDC, ctx: &IconContext, cx: f32, cy: f32, r: f32) {
    let (px1, py1) = ctx.pt(cx - r, cy - r);
    let (px2, py2) = ctx.pt(cx + r, cy + r);
    let _ = Ellipse(hdc, px1, py1, px2, py2);
}

#[inline]
pub unsafe fn draw_rect(hdc: HDC, ctx: &IconContext, x: f32, y: f32, w: f32, h: f32, r: f32) {
    let (px1, py1) = ctx.pt(x, y);
    let (px2, py2) = ctx.pt(x + w, y + h);
    if r > 0.0 {
        let rad = ctx.dim(r * 2.0);
        let _ = RoundRect(hdc, px1, py1, px2, py2, rad, rad);
    } else {
        let pts = [
            POINT { x: px1, y: py1 },
            POINT { x: px2, y: py1 },
            POINT { x: px2, y: py2 },
            POINT { x: px1, y: py2 },
            POINT { x: px1, y: py1 },
        ];
        let _ = Polyline(hdc, &pts);
    }
}

#[inline]
pub unsafe fn draw_arc(
    hdc: HDC,
    ctx: &IconContext,
    cx: f32,
    cy: f32,
    r: f32,
    start_x: f32,
    start_y: f32,
    end_x: f32,
    end_y: f32,
) {
    let (px1, py1) = ctx.pt(cx - r, cy - r);
    let (px2, py2) = ctx.pt(cx + r, cy + r);
    let (sx, sy) = ctx.pt(start_x, start_y);
    let (ex, ey) = ctx.pt(end_x, end_y);
    let _ = Arc(hdc, px1, py1, px2, py2, sx, sy, ex, ey);
}

#[inline]
pub unsafe fn draw_smooth_line(
    scope: &GraphicsScope,
    ctx: &IconContext,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
) {
    let (px1, py1) = ctx.fpt(x1, y1);
    let (px2, py2) = ctx.fpt(x2, y2);
    let _ = GdipDrawLine(scope.graphics, scope.pen, px1, py1, px2, py2);
}

#[inline]
pub unsafe fn draw_smooth_polyline(
    scope: &GraphicsScope,
    ctx: &IconContext,
    points: &[(f32, f32)],
) {
    if points.len() < 2 {
        return;
    }
    let pts: Vec<PointF> = points
        .iter()
        .map(|&(vx, vy)| {
            let (px, py) = ctx.fpt(vx, vy);
            PointF { X: px, Y: py }
        })
        .collect();
    let _ = GdipDrawLines(scope.graphics, scope.pen, pts.as_ptr(), pts.len() as i32);
}

#[inline]
pub unsafe fn draw_smooth_circle(
    scope: &GraphicsScope,
    ctx: &IconContext,
    cx: f32,
    cy: f32,
    r: f32,
) {
    let scale = ctx.size as f32 / 24.0;
    let (px, py) = ctx.fpt(cx - r, cy - r);
    let d = r * 2.0 * scale;
    let _ = GdipDrawEllipse(scope.graphics, scope.pen, px, py, d, d);
}

#[inline]
pub unsafe fn draw_smooth_filled_circle(
    scope: &GraphicsScope,
    ctx: &IconContext,
    cx: f32,
    cy: f32,
    r: f32,
    color: COLORREF,
) {
    let scale = ctx.size as f32 / 24.0;
    let (px, py) = ctx.fpt(cx - r, cy - r);
    let d = r * 2.0 * scale;
    let r_c = (color.0 & 0xFF) as u32;
    let g_c = ((color.0 >> 8) & 0xFF) as u32;
    let b_c = ((color.0 >> 16) & 0xFF) as u32;
    let argb = 0xFF000000 | (r_c << 16) | (g_c << 8) | b_c;
    let mut brush = std::ptr::null_mut();
    if GdipCreateSolidFill(argb, &mut brush) == windows::Win32::Graphics::GdiPlus::Ok {
        let b_ptr = brush as *mut windows::Win32::Graphics::GdiPlus::GpBrush;
        let _ = GdipFillEllipse(scope.graphics, b_ptr, px, py, d, d);
        let _ = GdipDeleteBrush(b_ptr);
    }
}

#[inline]
pub unsafe fn draw_smooth_rounded_rect(
    scope: &GraphicsScope,
    ctx: &IconContext,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
) {
    let scale = ctx.size as f32 / 24.0;
    let px = ctx.x as f32 + x * scale;
    let py = ctx.y as f32 + y * scale;
    let pw = w * scale;
    let ph = h * scale;
    let pr = (r * scale).min(pw / 2.0).min(ph / 2.0);
    let d = pr * 2.0;

    if pr <= 0.5 {
        let pts = [
            PointF { X: px, Y: py },
            PointF { X: px + pw, Y: py },
            PointF { X: px + pw, Y: py + ph },
            PointF { X: px + pw, Y: py + ph },
            PointF { X: px, Y: py },
        ];
        let _ = GdipDrawLines(scope.graphics, scope.pen, pts.as_ptr(), pts.len() as i32);
        return;
    }

    let _ = GdipDrawLine(scope.graphics, scope.pen, px + pr, py, px + pw - pr, py);
    let _ = GdipDrawArc(scope.graphics, scope.pen, px + pw - d, py, d, d, 270.0, 90.0);
    let _ = GdipDrawLine(scope.graphics, scope.pen, px + pw, py + pr, px + pw, py + ph - pr);
    let _ = GdipDrawArc(scope.graphics, scope.pen, px + pw - d, py + ph - d, d, d, 0.0, 90.0);
    let _ = GdipDrawLine(scope.graphics, scope.pen, px + pw - pr, py + ph, px + pr, py + ph);
    let _ = GdipDrawArc(scope.graphics, scope.pen, px, py + ph - d, d, d, 90.0, 90.0);
    let _ = GdipDrawLine(scope.graphics, scope.pen, px, py + ph - pr, px, py + pr);
    let _ = GdipDrawArc(scope.graphics, scope.pen, px, py, d, d, 180.0, 90.0);
}


#[inline]
pub unsafe fn draw_smooth_arc(
    scope: &GraphicsScope,
    ctx: &IconContext,
    cx: f32,
    cy: f32,
    r: f32,
    start_angle: f32,
    sweep_angle: f32,
) {
    let scale = ctx.size as f32 / 24.0;
    let (px, py) = ctx.fpt(cx - r, cy - r);
    let d = r * 2.0 * scale;
    let _ = GdipDrawArc(scope.graphics, scope.pen, px, py, d, d, start_angle, sweep_angle);
}
