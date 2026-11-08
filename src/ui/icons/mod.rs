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
pub mod info;
pub mod lock;
pub mod monitor;
pub mod plus;
pub mod radio;
pub mod refresh;
pub mod search;
pub mod server;
pub mod settings;
pub mod terminal;
pub mod trash;
pub mod wifi;
pub mod x;
pub mod zap;

pub use arrow_right::draw_arrow_right;
pub use bell::draw_bell;
pub use check::draw_check;
pub use copy::draw_copy;
pub use cpu::draw_cpu;
pub use file::draw_file;
pub use folder::draw_folder;
pub use info::draw_info;
pub use lock::draw_lock;
pub use monitor::draw_monitor;
pub use plus::draw_plus;
pub use radio::draw_radio;
pub use refresh::draw_refresh;
pub use search::draw_search;
pub use server::draw_server;
pub use settings::draw_settings;
pub use terminal::draw_terminal;
pub use trash::draw_trash;
pub use wifi::draw_wifi;
pub use x::draw_x;
pub use zap::draw_zap;

use windows::Win32::Foundation::{COLORREF, POINT};
use windows::Win32::Graphics::Gdi::{
    Arc, CreatePen, DeleteObject, Ellipse, GetStockObject, LineTo, MoveToEx, Polyline, RoundRect,
    SelectObject, HDC, HGDIOBJ, NULL_BRUSH, PS_SOLID,
};

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
