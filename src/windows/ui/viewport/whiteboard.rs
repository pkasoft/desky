//! Interactive In-Session Whiteboard & Screen Annotation Engine
//! High-performance, anti-aliased GDI+ subpixel overlay rendering directly on the remote canvas.

#![allow(unsafe_op_in_unsafe_fn)]

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Mutex;
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, DeleteObject, FillRect, HDC, HGDIOBJ,
};
use windows::Win32::Graphics::GdiPlus::{
    GdipCreateFromHDC, GdipCreatePen1, GdipCreateSolidFill, GdipDeleteBrush, GdipDeleteGraphics,
    GdipDeletePen, GdipDrawLines, GdipFillEllipse, GdipSetPenEndCap, GdipSetPenLineJoin,
    GdipSetPenStartCap, GdipSetSmoothingMode, LineCapRound, LineJoinRound, PointF,
    SmoothingModeHighQuality, UnitPixel,
};

pub static WHITEBOARD_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static WHITEBOARD_COLOR_IDX: AtomicU8 = AtomicU8::new(0);

// Colors in BGR (Windows COLORREF format)
pub const WB_COLOR_EMERALD: COLORREF = COLORREF(0x0081B910); // #10B981
pub const WB_COLOR_RED: COLORREF = COLORREF(0x004444EF);     // #EF4444
pub const WB_COLOR_YELLOW: COLORREF = COLORREF(0x000B9EF5);  // #F59E0B
pub const WB_COLOR_CYAN: COLORREF = COLORREF(0x00D4B606);    // #06B6D4

pub const WB_COLORS: [COLORREF; 4] = [
    WB_COLOR_EMERALD,
    WB_COLOR_RED,
    WB_COLOR_YELLOW,
    WB_COLOR_CYAN,
];

#[derive(Clone, Debug)]
pub struct WhiteboardStroke {
    pub points: Vec<(f32, f32)>, // Normalized coordinates (0.0 .. 1.0)
    pub color: COLORREF,
    pub width: f32,
}

static STROKES: Mutex<Vec<WhiteboardStroke>> = Mutex::new(Vec::new());
static CURRENT_STROKE: Mutex<Option<WhiteboardStroke>> = Mutex::new(None);

pub fn is_whiteboard_active() -> bool {
    WHITEBOARD_ACTIVE.load(Ordering::Relaxed)
}

pub fn toggle_whiteboard() -> bool {
    let new_val = !WHITEBOARD_ACTIVE.load(Ordering::Relaxed);
    WHITEBOARD_ACTIVE.store(new_val, Ordering::Relaxed);
    new_val
}

pub fn set_whiteboard_active(active: bool) {
    WHITEBOARD_ACTIVE.store(active, Ordering::Relaxed);
}

pub fn clear_strokes() {
    if let Ok(mut guard) = STROKES.lock() {
        guard.clear();
    }
    if let Ok(mut cur) = CURRENT_STROKE.lock() {
        *cur = None;
    }
}

pub fn start_stroke(nx: f32, ny: f32) {
    let col_idx = WHITEBOARD_COLOR_IDX.load(Ordering::Relaxed) as usize;
    let color = WB_COLORS.get(col_idx).copied().unwrap_or(WB_COLOR_EMERALD);
    let stroke = WhiteboardStroke {
        points: vec![(nx, ny)],
        color,
        width: 3.5,
    };
    if let Ok(mut cur) = CURRENT_STROKE.lock() {
        *cur = Some(stroke);
    }
}

pub fn add_stroke_point(nx: f32, ny: f32) {
    if let Ok(mut cur) = CURRENT_STROKE.lock() {
        if let Some(ref mut stroke) = *cur {
            if let Some(&(lx, ly)) = stroke.points.last() {
                let dx = nx - lx;
                let dy = ny - ly;
                if dx * dx + dy * dy >= 0.000004 { // Minimum displacement threshold
                    stroke.points.push((nx, ny));
                }
            } else {
                stroke.points.push((nx, ny));
            }
        }
    }
}

pub fn end_stroke() {
    if let Ok(mut cur) = CURRENT_STROKE.lock() {
        if let Some(stroke) = cur.take() {
            if stroke.points.len() >= 2 {
                if let Ok(mut strokes) = STROKES.lock() {
                    strokes.push(stroke);
                }
            }
        }
    }
}

/// Renders all whiteboard strokes on top of the remote video stream with GDI+ subpixel antialiasing
pub unsafe fn draw_whiteboard_overlay(
    hdc: HDC,
    render_x: i32,
    render_y: i32,
    render_w: i32,
    render_h: i32,
) {
    if render_w <= 0 || render_h <= 0 {
        return;
    }

    crate::ui::icons::ensure_gdiplus();

    let mut graphics = std::ptr::null_mut();
    if GdipCreateFromHDC(hdc, &mut graphics) != windows::Win32::Graphics::GdiPlus::Ok {
        return;
    }
    let _ = GdipSetSmoothingMode(graphics, SmoothingModeHighQuality);

    let rw = render_w as f32;
    let rh = render_h as f32;
    let ox = render_x as f32;
    let oy = render_y as f32;

    let mut all_strokes = Vec::new();
    if let Ok(guard) = STROKES.lock() {
        all_strokes.extend(guard.iter().cloned());
    }
    if let Ok(cur) = CURRENT_STROKE.lock() {
        if let Some(ref stroke) = *cur {
            all_strokes.push(stroke.clone());
        }
    }

    for stroke in all_strokes {
        if stroke.points.len() < 2 {
            continue;
        }

        let r = (stroke.color.0 & 0xFF) as u32;
        let g = ((stroke.color.0 >> 8) & 0xFF) as u32;
        let b = ((stroke.color.0 >> 16) & 0xFF) as u32;
        let argb = 0xFF000000 | (r << 16) | (g << 8) | b;

        let mut pen = std::ptr::null_mut();
        if GdipCreatePen1(argb, stroke.width, UnitPixel, &mut pen)
            == windows::Win32::Graphics::GdiPlus::Ok
        {
            let _ = GdipSetPenStartCap(pen, LineCapRound);
            let _ = GdipSetPenEndCap(pen, LineCapRound);
            let _ = GdipSetPenLineJoin(pen, LineJoinRound);

            let pts: Vec<PointF> = stroke
                .points
                .iter()
                .map(|&(nx, ny)| PointF {
                    X: ox + nx * rw,
                    Y: oy + ny * rh,
                })
                .collect();

            let _ = GdipDrawLines(graphics, pen, pts.as_ptr(), pts.len() as i32);
            let _ = GdipDeletePen(pen);
        }
    }

    let _ = GdipDeleteGraphics(graphics);
}

/// Floating Whiteboard Toolbar Palette (Color Dots + Clear Button)
pub unsafe fn draw_whiteboard_palette(
    hdc: HDC,
    client_w: i32,
    toolbar_h: i32,
    hover_state: i32,
) {
    if !is_whiteboard_active() {
        return;
    }

    let p_w = 176;
    let p_h = 36;
    let p_x = (client_w - p_w) / 2;
    let p_y = toolbar_h + 8;

    // Palette pill background
    super::theme::draw_rounded_box(
        hdc,
        p_x,
        p_y,
        p_w,
        p_h,
        super::theme::PK_CARD,
        super::theme::PK_CARD_BORDER,
        8,
    );

    let active_color_idx = WHITEBOARD_COLOR_IDX.load(Ordering::Relaxed) as usize;

    crate::ui::icons::ensure_gdiplus();
    let mut graphics = std::ptr::null_mut();
    let has_gfx = GdipCreateFromHDC(hdc, &mut graphics) == windows::Win32::Graphics::GdiPlus::Ok;
    if has_gfx {
        let _ = GdipSetSmoothingMode(graphics, SmoothingModeHighQuality);
    }

    // 4 Color circles
    let start_x = p_x + 12;
    let dot_spacing = 28;
    for (i, &col) in WB_COLORS.iter().enumerate() {
        let cx = (start_x + (i as i32) * dot_spacing) as f32;
        let cy = (p_y + p_h / 2) as f32;
        let is_selected = i == active_color_idx;
        let is_hover = hover_state == 9301 + (i as i32);

        let r = (col.0 & 0xFF) as u32;
        let g = ((col.0 >> 8) & 0xFF) as u32;
        let b = ((col.0 >> 16) & 0xFF) as u32;
        let argb = 0xFF000000 | (r << 16) | (g << 8) | b;

        if has_gfx {
            // Selected outline ring
            if is_selected || is_hover {
                let mut ring_pen = std::ptr::null_mut();
                let ring_argb = if is_selected { 0xFF10B981 } else { 0x6610B981 };
                if GdipCreatePen1(ring_argb, 2.0, UnitPixel, &mut ring_pen) == windows::Win32::Graphics::GdiPlus::Ok {
                    let _ = windows::Win32::Graphics::GdiPlus::GdipDrawEllipse(
                        graphics,
                        ring_pen,
                        cx - 10.0,
                        cy - 10.0,
                        20.0,
                        20.0,
                    );
                    let _ = GdipDeletePen(ring_pen);
                }
            }

            let mut brush = std::ptr::null_mut();
            if GdipCreateSolidFill(argb, &mut brush) == windows::Win32::Graphics::GdiPlus::Ok {
                let dot_radius = if is_selected { 7.0 } else { 6.0 };
                let _ = GdipFillEllipse(
                    graphics,
                    brush as *mut _,
                    cx - dot_radius,
                    cy - dot_radius,
                    dot_radius * 2.0,
                    dot_radius * 2.0,
                );
                let _ = GdipDeleteBrush(brush as *mut _);
            }
        }
    }

    if has_gfx {
        let _ = GdipDeleteGraphics(graphics);
    }

    // Divider line
    let div_x = start_x + 4 * dot_spacing;
    let div_rc = RECT {
        left: div_x,
        top: p_y + 6,
        right: div_x + 1,
        bottom: p_y + p_h - 6,
    };
    let div_brush = CreateSolidBrush(super::theme::PK_CARD_BORDER);
    FillRect(hdc, &div_rc, div_brush);
    let _ = DeleteObject(HGDIOBJ(div_brush.0));

    // Trash / Clear Button
    let trash_x = div_x + 8;
    let trash_y = p_y + 4;
    let trash_sz = 28;
    let is_trash_hover = hover_state == 9399;
    let (t_bg, t_border, t_fg) = if is_trash_hover {
        (COLORREF(0x00EBF0FF), super::theme::PK_RED, super::theme::PK_RED)
    } else {
        (super::theme::PK_CARD, super::theme::PK_CARD_BORDER, super::theme::PK_MUTED)
    };
    super::theme::draw_rounded_box(hdc, trash_x, trash_y, trash_sz, trash_sz, t_bg, t_border, 6);
    crate::ui::draw_trash(hdc, trash_x + 6, trash_y + 6, 16, t_fg, 2);
}

pub fn hit_test_whiteboard_palette(
    client_w: i32,
    toolbar_h: i32,
    client_x: i32,
    client_y: i32,
) -> i32 {
    if !is_whiteboard_active() {
        return -1;
    }

    let p_w = 176;
    let p_h = 36;
    let p_x = (client_w - p_w) / 2;
    let p_y = toolbar_h + 8;

    if client_x < p_x || client_x >= p_x + p_w || client_y < p_y || client_y >= p_y + p_h {
        return -1;
    }

    let start_x = p_x + 12;
    let dot_spacing = 28;
    for i in 0..4 {
        let cx = start_x + (i as i32) * dot_spacing;
        let dx = client_x - cx;
        let dy = client_y - (p_y + p_h / 2);
        if dx * dx + dy * dy <= 12 * 12 {
            return 9301 + i as i32;
        }
    }

    let div_x = start_x + 4 * dot_spacing;
    let trash_x = div_x + 8;
    if client_x >= trash_x && client_x < trash_x + 28 && client_y >= p_y + 4 && client_y < p_y + 32 {
        return 9399; // Clear strokes
    }

    9300 // Inside palette container
}
