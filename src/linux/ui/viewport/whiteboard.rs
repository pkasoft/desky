//! Interactive In-Session Whiteboard & Screen Annotation Engine (Linux X11)
//!
//! Mirrors `src/windows/ui/viewport/whiteboard.rs` by providing anti-aliased
//! polyline screen annotations and a floating 4-color + clear palette.

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Mutex;

use crate::linux::ui::theme::{
    PixelCanvas, PK_CARD, PK_CARD_BORDER, PK_MUTED, PK_RED, PK_RED_TINT,
};

pub static WHITEBOARD_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static WHITEBOARD_COLOR_IDX: AtomicU8 = AtomicU8::new(0);

// Colors in RGB (0x00RRGGBB)
pub const WB_COLOR_EMERALD: u32 = 0x0010B981;
pub const WB_COLOR_RED: u32 = 0x00EF4444;
pub const WB_COLOR_YELLOW: u32 = 0x00F59E0B;
pub const WB_COLOR_CYAN: u32 = 0x0006B6D4;

pub const WB_COLORS: [u32; 4] = [
    WB_COLOR_EMERALD,
    WB_COLOR_RED,
    WB_COLOR_YELLOW,
    WB_COLOR_CYAN,
];

#[derive(Clone, Debug)]
pub struct WhiteboardStroke {
    pub points: Vec<(f32, f32)>, // Normalized coordinates (0.0 .. 1.0)
    pub color: u32,
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
                if dx * dx + dy * dy >= 0.000004 {
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

pub fn draw_whiteboard_overlay(
    canvas: &mut PixelCanvas,
    render_x: i32,
    render_y: i32,
    render_w: i32,
    render_h: i32,
) {
    if render_w <= 0 || render_h <= 0 {
        return;
    }

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
        for pair in stroke.points.windows(2) {
            let x0 = ox + pair[0].0 * rw;
            let y0 = oy + pair[0].1 * rh;
            let x1 = ox + pair[1].0 * rw;
            let y1 = oy + pair[1].1 * rh;
            canvas.draw_line(x0, y0, x1, y1, stroke.color, stroke.width);
        }
    }
}

pub fn draw_whiteboard_palette(
    canvas: &mut PixelCanvas,
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

    canvas.draw_rounded_box(p_x, p_y, p_w, p_h, PK_CARD, PK_CARD_BORDER, 8);
    let active_color_idx = WHITEBOARD_COLOR_IDX.load(Ordering::Relaxed) as usize;

    let start_x = p_x + 22;
    let dot_spacing = 28;
    for (i, &col) in WB_COLORS.iter().enumerate() {
        let cx = (start_x + (i as i32) * dot_spacing) as f32;
        let cy = (p_y + p_h / 2) as f32;
        let is_selected = i == active_color_idx;
        let is_hover = hover_state == 9301 + (i as i32);

        if is_selected || is_hover {
            canvas.draw_circle(cx, cy, 9.5, WB_COLOR_EMERALD, 1.8, false);
        }
        canvas.draw_circle(cx, cy, if is_selected { 6.5 } else { 5.5 }, col, 1.0, true);
    }

    let div_x = start_x + 4 * dot_spacing - 8;
    canvas.fill_rect(div_x, p_y + 6, 1, p_h - 12, PK_CARD_BORDER);

    let trash_x = div_x + 8;
    let trash_y = p_y + 4;
    let trash_sz = 28;
    let is_trash_hover = hover_state == 9399;
    let (t_bg, t_border, t_fg) = if is_trash_hover {
        (PK_RED_TINT, PK_RED, PK_RED)
    } else {
        (PK_CARD, PK_CARD_BORDER, PK_MUTED)
    };
    canvas.draw_rounded_box(trash_x, trash_y, trash_sz, trash_sz, t_bg, t_border, 6);
    canvas.draw_icon_trash(trash_x + 6, trash_y + 6, 16, t_fg);
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

    if client_x < p_x
        || client_x >= p_x + p_w
        || client_y < p_y
        || client_y >= p_y + p_h
    {
        return -1;
    }

    let start_x = p_x + 22;
    let dot_spacing = 28;
    for i in 0..4 {
        let cx = start_x + (i as i32) * dot_spacing;
        let dx = client_x - cx;
        let dy = client_y - (p_y + p_h / 2);
        if dx * dx + dy * dy <= 12 * 12 {
            return 9301 + i as i32;
        }
    }

    let div_x = start_x + 4 * dot_spacing - 8;
    let trash_x = div_x + 8;
    if client_x >= trash_x
        && client_x < trash_x + 28
        && client_y >= p_y + 4
        && client_y < p_y + 32
    {
        return 9399;
    }

    9300
}
