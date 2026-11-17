//! pkasoft.com White Theme Styling, Anti-Aliased TrueType Font Engine & Software Canvas (Linux X11)
//!
//! Renders crisp anti-aliased TrueType typography (`DejaVu Sans` / `Liberation Sans` / `Ubuntu`),
//! subtle GDI-style rounded boxes, and Lucide vector icons directly into an in-memory BGRA
//! pixel buffer matching the Windows GDI ClearType pkasoft.com White Theme UI 1:1.

#![allow(dead_code)]

use ab_glyph::{point, Font, FontArc, PxScale, ScaleFont};
use std::sync::OnceLock;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

// pkasoft.com White Theme color palette (0x00RRGGBB)
pub const PK_BG: u32 = 0x00F5F7F5;            // #f5f7f5 (--paper soft clean canvas)
pub const PK_CARD: u32 = 0x00FFFFFF;          // #ffffff (pure crisp white card)
pub const PK_CARD_BORDER: u32 = 0x00E2E7E3;   // #e2e7e3 (--line border)
pub const PK_GREEN: u32 = 0x002D6A4F;         // #2d6a4f (--green signature emerald)
pub const PK_GREEN_DARK: u32 = 0x001B4332;    // #1b4332 (--green-dark pine hover)
pub const PK_GREEN_TINT: u32 = 0x00EEF5F0;    // #eef5f0 (subtle green pill fill)
pub const PK_TEXT: u32 = 0x001D2420;          // #1d2420 (--ink deep crisp charcoal)
pub const PK_MUTED: u32 = 0x005D6A63;         // #5d6a63 (--muted slate sage)
pub const PK_INPUT_BG: u32 = 0x00FAFCFA;      // #fafcfa (soft clean input background)
pub const PK_CONSOLE_BG: u32 = 0x00F8FAF8;    // #f8faf8 (console log box background)
pub const PK_BTN_HOVER: u32 = 0x00E7EDE5;     // #e7ede5 (hover outline button)
pub const PK_RED: u32 = 0x00C53030;           // #c53030 (crimson decline button)
pub const PK_RED_TINT: u32 = 0x00FDF2F2;      // #fdf2f2 (subtle red alert fill)
pub const PK_AMBER: u32 = 0x00D9851B;         // #d9851b (waiting approval accent)
pub const PK_DARK_VIEWPORT: u32 = 0x000C0A09; // #0c0a09 (dark canvas background for remote stream)

// Toolbar button state colors (matching Windows toolbar.rs 1:1, converted from BGR to RGB):
pub const TB_ACTIVE_BG: u32 = 0x00E0F5D0;     // Vibrant emerald mint fill
pub const TB_ACTIVE_BORDER: u32 = 0x0008782B; // Bold forest emerald border
pub const TB_ACTIVE_FG: u32 = 0x00044F1B;     // Deep contrast emerald icon

pub const TB_REC_BG: u32 = 0x00FFDCD4;        // Soft red fill
pub const TB_REC_BORDER: u32 = 0x00DF372B;    // Red border
pub const TB_REC_FG: u32 = 0x00DF372B;

pub const TB_PRIV_BG: u32 = 0x00FFEBCE;       // Amber fill
pub const TB_PRIV_BORDER: u32 = 0x00E6820E;   // Amber border
pub const TB_PRIV_FG: u32 = 0x009C560A;       // Bronze lock

pub const TB_MUTED_BG: u32 = 0x00FFEEEC;      // Soft rose fill for muted audio
pub const TB_MUTED_BORDER: u32 = 0x00E88B7A;  // Rose border
pub const TB_MUTED_FG: u32 = 0x00DF372B;      // Red volume_x icon

pub const TB_PASSIVE_BG: u32 = 0x00FBFCFA;    // Clean off-white card fill
pub const TB_PASSIVE_BORDER: u32 = 0x00DEDCE0;// Subtle 1px border
pub const TB_PASSIVE_FG: u32 = 0x00777D6E;    // Slate gray passive icon

pub const TB_HOVER_BG: u32 = 0x00ECF0EB;      // Hover fill
pub const TB_HOVER_BORDER: u32 = 0x00BCB5C2;  // Hover border
pub const TB_HOVER_FG: u32 = 0x0020241B;      // Hover icon

pub struct FontSystem {
    pub regular: Option<FontArc>,
    pub bold: Option<FontArc>,
    pub mono: Option<FontArc>,
}

static FONTS: OnceLock<FontSystem> = OnceLock::new();

fn load_first_font(paths: &[&str]) -> Option<FontArc> {
    for &p in paths {
        if let Ok(bytes) = std::fs::read(p) {
            if let Ok(font) = FontArc::try_from_vec(bytes) {
                return Some(font);
            }
        }
    }
    None
}

pub fn fonts() -> &'static FontSystem {
    FONTS.get_or_init(|| {
        let regular = load_first_font(&[
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
            "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
        ]);
        let bold = load_first_font(&[
            "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/truetype/ubuntu/Ubuntu-B.ttf",
            "/usr/share/fonts/truetype/freefont/FreeSansBold.ttf",
            "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
        ])
        .or_else(|| regular.clone());
        let mono = load_first_font(&[
            "/usr/share/fonts/truetype/liberation/LiberationMono-Bold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/truetype/ubuntu/UbuntuMono-B.ttf",
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
        ])
        .or_else(|| bold.clone());

        FontSystem {
            regular,
            bold,
            mono,
        }
    })
}

/// High-speed in-memory BGRA software canvas with anti-aliased TrueType font & vector icon rendering
pub struct PixelCanvas {
    pub width: i32,
    pub height: i32,
    pub data: Vec<u8>,
}

impl PixelCanvas {
    pub fn new(width: u16, height: u16) -> Self {
        let w = width.max(1) as i32;
        let h = height.max(1) as i32;
        Self {
            width: w,
            height: h,
            data: vec![0u8; (w as usize) * (h as usize) * 4],
        }
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        let w = width.max(1) as i32;
        let h = height.max(1) as i32;
        if self.width != w || self.height != h {
            self.width = w;
            self.height = h;
            self.data.resize((w as usize) * (h as usize) * 4, 0);
        }
    }

    pub fn clear(&mut self, rgb: u32) {
        self.fill_rect(0, 0, self.width, self.height, rgb);
    }

    #[inline]
    pub fn set_pixel(&mut self, x: i32, y: i32, rgb: u32) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let idx = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        self.data[idx] = (rgb & 0xFF) as u8;
        self.data[idx + 1] = ((rgb >> 8) & 0xFF) as u8;
        self.data[idx + 2] = ((rgb >> 16) & 0xFF) as u8;
        self.data[idx + 3] = 0xFF;
    }

    #[inline]
    pub fn blend_pixel(&mut self, x: i32, y: i32, rgb: u32, alpha: f32) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height || alpha <= 0.0 {
            return;
        }
        let idx = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        let a = alpha.clamp(0.0, 1.0);
        let inv = 1.0 - a;

        let sr = ((rgb >> 16) & 0xFF) as f32;
        let sg = ((rgb >> 8) & 0xFF) as f32;
        let sb = (rgb & 0xFF) as f32;

        let db = self.data[idx] as f32;
        let dg = self.data[idx + 1] as f32;
        let dr = self.data[idx + 2] as f32;

        self.data[idx] = (sb * a + db * inv) as u8;
        self.data[idx + 1] = (sg * a + dg * inv) as u8;
        self.data[idx + 2] = (sr * a + dr * inv) as u8;
        self.data[idx + 3] = 0xFF;
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: u32) {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(self.width);
        let y1 = (y + h).min(self.height);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let b = (rgb & 0xFF) as u8;
        let g = ((rgb >> 8) & 0xFF) as u8;
        let r = ((rgb >> 16) & 0xFF) as u8;
        let pixel = [b, g, r, 0xFF];

        let stride = (self.width as usize) * 4;
        for py in y0..y1 {
            let row_start = (py as usize) * stride + (x0 as usize) * 4;
            let row_end = (py as usize) * stride + (x1 as usize) * 4;
            for chunk in self.data[row_start..row_end].chunks_exact_mut(4) {
                chunk.copy_from_slice(&pixel);
            }
        }
    }

    /// Draws a crisp rectangular toolbar button box (matching Windows `draw_tb_button_box` 1:1)
    #[allow(clippy::too_many_arguments)]
    pub fn draw_tb_button_box(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        fill_rgb: u32,
        border_rgb: u32,
        border_width: i32,
    ) {
        let bw = border_width.max(1);
        self.fill_rect(x, y, w, h, border_rgb);
        if w > bw * 2 && h > bw * 2 {
            self.fill_rect(x + bw, y + bw, w - bw * 2, h - bw * 2, fill_rgb);
        }
    }

    /// Renders a subtle GDI-style rounded rectangle (`RoundRect` diameter = radius, so actual corner radius is half!)
    #[allow(clippy::too_many_arguments)]
    pub fn draw_rounded_box(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        fill_rgb: u32,
        border_rgb: u32,
        radius_diameter: i32,
    ) {
        if w <= 0 || h <= 0 {
            return;
        }
        // In Windows GDI RoundRect(hdc, ..., d, d), `d` is the ellipse diameter, so actual corner radius is d / 2!
        let r = ((radius_diameter as f32) * 0.5).clamp(0.0, 5.0);
        if r <= 1.0 {
            self.draw_tb_button_box(x, y, w, h, fill_rgb, border_rgb, 1);
            return;
        }

        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(self.width);
        let y1 = (y + h).min(self.height);

        let wf = w as f32;
        let hf = h as f32;

        for py in y0..y1 {
            let ly = (py - y) as f32 + 0.5;
            for px in x0..x1 {
                let lx = (px - x) as f32 + 0.5;

                if lx >= r && lx <= wf - r && ly >= 1.2 && ly <= hf - 1.2 {
                    self.set_pixel(px, py, fill_rgb);
                    continue;
                }
                if ly >= r && ly <= hf - r && lx >= 1.2 && lx <= wf - 1.2 {
                    self.set_pixel(px, py, fill_rgb);
                    continue;
                }

                let qx = (lx - wf * 0.5).abs() - (wf * 0.5 - r);
                let qy = (ly - hf * 0.5).abs() - (hf * 0.5 - r);
                let dist = (qx.max(0.0).hypot(qy.max(0.0))) + qx.max(qy).min(0.0) - r;

                if dist <= -1.0 {
                    self.set_pixel(px, py, fill_rgb);
                } else if dist <= 0.5 {
                    let outer_alpha = (0.5 - dist).clamp(0.0, 1.0);
                    let border_mix = (dist + 1.0).clamp(0.0, 1.0);
                    let color = lerp_rgb(fill_rgb, border_rgb, border_mix);
                    self.blend_pixel(px, py, color, outer_alpha);
                }
            }
        }
    }

    /// Anti-aliased line segment with configurable thickness
    pub fn draw_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, rgb: u32, thickness: f32) {
        let half_t = (thickness * 0.5).max(0.5);
        let pad = (half_t + 1.5).ceil() as i32;
        let min_x = (x0.min(x1).floor() as i32 - pad).max(0);
        let max_x = (x0.max(x1).ceil() as i32 + pad).min(self.width - 1);
        let min_y = (y0.min(y1).floor() as i32 - pad).max(0);
        let max_y = (y0.max(y1).ceil() as i32 + pad).min(self.height - 1);

        let dx = x1 - x0;
        let dy = y1 - y0;
        let len_sq = dx * dx + dy * dy;

        for py in min_y..=max_y {
            let fy = py as f32 + 0.5;
            for px in min_x..=max_x {
                let fx = px as f32 + 0.5;
                let t = if len_sq > 0.0001 {
                    ((fx - x0) * dx + (fy - y0) * dy) / len_sq
                } else {
                    0.0
                }
                .clamp(0.0, 1.0);
                let proj_x = x0 + t * dx;
                let proj_y = y0 + t * dy;
                let dist = (fx - proj_x).hypot(fy - proj_y);
                let alpha = (half_t + 0.5 - dist).clamp(0.0, 1.0);
                if alpha > 0.0 {
                    self.blend_pixel(px, py, rgb, alpha);
                }
            }
        }
    }

    /// Anti-aliased circle outline or filled disc
    pub fn draw_circle(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        rgb: u32,
        thickness: f32,
        filled: bool,
    ) {
        let half_t = (thickness * 0.5).max(0.5);
        let pad = (radius + half_t + 2.0).ceil() as i32;
        let min_x = (cx.floor() as i32 - pad).max(0);
        let max_x = (cx.ceil() as i32 + pad).min(self.width - 1);
        let min_y = (cy.floor() as i32 - pad).max(0);
        let max_y = (cy.ceil() as i32 + pad).min(self.height - 1);

        for py in min_y..=max_y {
            let fy = py as f32 + 0.5;
            for px in min_x..=max_x {
                let fx = px as f32 + 0.5;
                let dist = (fx - cx).hypot(fy - cy);
                let alpha = if filled {
                    (radius + 0.5 - dist).clamp(0.0, 1.0)
                } else {
                    (half_t + 0.5 - (dist - radius).abs()).clamp(0.0, 1.0)
                };
                if alpha > 0.0 {
                    self.blend_pixel(px, py, rgb, alpha);
                }
            }
        }
    }

    // =========================================================================
    // Lucide Vector Icon Suite (Matching Windows `src/ui/icons/` 1:1)
    // =========================================================================

    pub fn draw_icon_home(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let ox = x as f32;
        let oy = y as f32;
        self.draw_line(ox + s * 0.12, oy + s * 0.45, ox + s * 0.50, oy + s * 0.12, rgb, 1.8);
        self.draw_line(ox + s * 0.50, oy + s * 0.12, ox + s * 0.88, oy + s * 0.45, rgb, 1.8);
        self.draw_line(ox + s * 0.22, oy + s * 0.40, ox + s * 0.22, oy + s * 0.88, rgb, 1.8);
        self.draw_line(ox + s * 0.78, oy + s * 0.40, ox + s * 0.78, oy + s * 0.88, rgb, 1.8);
        self.draw_line(ox + s * 0.22, oy + s * 0.88, ox + s * 0.78, oy + s * 0.88, rgb, 1.8);
    }

    pub fn draw_icon_plus(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let ox = x as f32;
        let oy = y as f32;
        self.draw_line(ox + s * 0.5, oy + s * 0.2, ox + s * 0.5, oy + s * 0.8, rgb, 1.8);
        self.draw_line(ox + s * 0.2, oy + s * 0.5, ox + s * 0.8, oy + s * 0.5, rgb, 1.8);
    }

    pub fn draw_icon_x(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let ox = x as f32;
        let oy = y as f32;
        self.draw_line(ox + s * 0.22, oy + s * 0.22, ox + s * 0.78, oy + s * 0.78, rgb, 1.8);
        self.draw_line(ox + s * 0.78, oy + s * 0.22, ox + s * 0.22, oy + s * 0.78, rgb, 1.8);
    }

    pub fn draw_icon_check(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let ox = x as f32;
        let oy = y as f32;
        self.draw_line(ox + s * 0.18, oy + s * 0.52, ox + s * 0.42, oy + s * 0.76, rgb, 2.0);
        self.draw_line(ox + s * 0.42, oy + s * 0.76, ox + s * 0.84, oy + s * 0.26, rgb, 2.0);
    }

    pub fn draw_icon_copy(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let ox = x as f32;
        let oy = y as f32;
        // Back rect
        self.draw_line(ox + s * 0.15, oy + s * 0.15, ox + s * 0.65, oy + s * 0.15, rgb, 1.6);
        self.draw_line(ox + s * 0.15, oy + s * 0.15, ox + s * 0.15, oy + s * 0.65, rgb, 1.6);
        // Front rect
        let rx = (ox + s * 0.32) as i32;
        let ry = (oy + s * 0.32) as i32;
        let rw = (s * 0.55) as i32;
        self.draw_line(rx as f32, ry as f32, (rx + rw) as f32, ry as f32, rgb, 1.6);
        self.draw_line((rx + rw) as f32, ry as f32, (rx + rw) as f32, (ry + rw) as f32, rgb, 1.6);
        self.draw_line((rx + rw) as f32, (ry + rw) as f32, rx as f32, (ry + rw) as f32, rgb, 1.6);
        self.draw_line(rx as f32, (ry + rw) as f32, rx as f32, ry as f32, rgb, 1.6);
    }

    pub fn draw_icon_arrow_right(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let ox = x as f32;
        let oy = y as f32;
        self.draw_line(ox + s * 0.18, oy + s * 0.50, ox + s * 0.82, oy + s * 0.50, rgb, 1.8);
        self.draw_line(ox + s * 0.54, oy + s * 0.22, ox + s * 0.82, oy + s * 0.50, rgb, 1.8);
        self.draw_line(ox + s * 0.54, oy + s * 0.78, ox + s * 0.82, oy + s * 0.50, rgb, 1.8);
    }

    pub fn draw_icon_settings(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let cx = x as f32 + s * 0.5;
        let cy = y as f32 + s * 0.5;
        self.draw_circle(cx, cy, s * 0.22, rgb, 1.8, false);
        for i in 0..8 {
            let angle = (i as f32) * std::f32::consts::FRAC_PI_4;
            let r1 = s * 0.28;
            let r2 = s * 0.44;
            self.draw_line(
                cx + angle.cos() * r1,
                cy + angle.sin() * r1,
                cx + angle.cos() * r2,
                cy + angle.sin() * r2,
                rgb,
                1.8,
            );
        }
    }

    pub fn draw_icon_chat(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let ox = x as f32;
        let oy = y as f32;
        self.draw_line(ox + s * 0.15, oy + s * 0.18, ox + s * 0.85, oy + s * 0.18, rgb, 1.7);
        self.draw_line(ox + s * 0.85, oy + s * 0.18, ox + s * 0.85, oy + s * 0.68, rgb, 1.7);
        self.draw_line(ox + s * 0.85, oy + s * 0.68, ox + s * 0.38, oy + s * 0.68, rgb, 1.7);
        self.draw_line(ox + s * 0.38, oy + s * 0.68, ox + s * 0.15, oy + s * 0.86, rgb, 1.7);
        self.draw_line(ox + s * 0.15, oy + s * 0.86, ox + s * 0.15, oy + s * 0.18, rgb, 1.7);
    }

    pub fn draw_icon_terminal(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let ox = x as f32;
        let oy = y as f32;
        self.draw_line(ox + s * 0.18, oy + s * 0.28, ox + s * 0.45, oy + s * 0.50, rgb, 1.8);
        self.draw_line(ox + s * 0.45, oy + s * 0.50, ox + s * 0.18, oy + s * 0.72, rgb, 1.8);
        self.draw_line(ox + s * 0.52, oy + s * 0.72, ox + s * 0.82, oy + s * 0.72, rgb, 1.8);
    }

    pub fn draw_icon_monitor(&mut self, x: i32, y: i32, sz: i32, rgb: u32, num: Option<u32>) {
        let s = sz as f32;
        let ox = x as f32;
        let oy = y as f32;
        self.draw_line(ox + s * 0.12, oy + s * 0.16, ox + s * 0.88, oy + s * 0.16, rgb, 1.7);
        self.draw_line(ox + s * 0.88, oy + s * 0.16, ox + s * 0.88, oy + s * 0.68, rgb, 1.7);
        self.draw_line(ox + s * 0.88, oy + s * 0.68, ox + s * 0.12, oy + s * 0.68, rgb, 1.7);
        self.draw_line(ox + s * 0.12, oy + s * 0.68, ox + s * 0.12, oy + s * 0.16, rgb, 1.7);
        self.draw_line(ox + s * 0.50, oy + s * 0.68, ox + s * 0.50, oy + s * 0.86, rgb, 1.7);
        self.draw_line(ox + s * 0.30, oy + s * 0.86, ox + s * 0.70, oy + s * 0.86, rgb, 1.7);
        if let Some(n) = num {
            self.draw_text_centered(
                x + 2,
                y + 1,
                sz - 4,
                (sz as f32 * 0.68) as i32,
                &n.to_string(),
                9.5,
                true,
                rgb,
            );
        }
    }

    pub fn draw_icon_lock(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let ox = x as f32;
        let oy = y as f32;
        // Body
        self.draw_line(ox + s * 0.20, oy + s * 0.46, ox + s * 0.80, oy + s * 0.46, rgb, 1.7);
        self.draw_line(ox + s * 0.80, oy + s * 0.46, ox + s * 0.80, oy + s * 0.86, rgb, 1.7);
        self.draw_line(ox + s * 0.80, oy + s * 0.86, ox + s * 0.20, oy + s * 0.86, rgb, 1.7);
        self.draw_line(ox + s * 0.20, oy + s * 0.86, ox + s * 0.20, oy + s * 0.46, rgb, 1.7);
        // Shackle
        self.draw_line(ox + s * 0.32, oy + s * 0.46, ox + s * 0.32, oy + s * 0.24, rgb, 1.7);
        self.draw_line(ox + s * 0.68, oy + s * 0.46, ox + s * 0.68, oy + s * 0.24, rgb, 1.7);
        self.draw_line(ox + s * 0.32, oy + s * 0.24, ox + s * 0.68, oy + s * 0.24, rgb, 1.7);
    }

    pub fn draw_icon_mic(&mut self, x: i32, y: i32, sz: i32, rgb: u32, muted: bool) {
        let s = sz as f32;
        let ox = x as f32;
        let oy = y as f32;
        self.draw_line(ox + s * 0.38, oy + s * 0.16, ox + s * 0.62, oy + s * 0.16, rgb, 1.7);
        self.draw_line(ox + s * 0.38, oy + s * 0.16, ox + s * 0.38, oy + s * 0.52, rgb, 1.7);
        self.draw_line(ox + s * 0.62, oy + s * 0.16, ox + s * 0.62, oy + s * 0.52, rgb, 1.7);
        self.draw_line(ox + s * 0.24, oy + s * 0.48, ox + s * 0.50, oy + s * 0.68, rgb, 1.7);
        self.draw_line(ox + s * 0.76, oy + s * 0.48, ox + s * 0.50, oy + s * 0.68, rgb, 1.7);
        self.draw_line(ox + s * 0.50, oy + s * 0.68, ox + s * 0.50, oy + s * 0.86, rgb, 1.7);
        if muted {
            self.draw_line(ox + s * 0.15, oy + s * 0.15, ox + s * 0.85, oy + s * 0.85, rgb, 1.8);
        }
    }

    pub fn draw_icon_volume(&mut self, x: i32, y: i32, sz: i32, rgb: u32, muted: bool) {
        let s = sz as f32;
        let ox = x as f32;
        let oy = y as f32;
        self.draw_line(ox + s * 0.15, oy + s * 0.38, ox + s * 0.34, oy + s * 0.38, rgb, 1.7);
        self.draw_line(ox + s * 0.34, oy + s * 0.38, ox + s * 0.56, oy + s * 0.18, rgb, 1.7);
        self.draw_line(ox + s * 0.56, oy + s * 0.18, ox + s * 0.56, oy + s * 0.82, rgb, 1.7);
        self.draw_line(ox + s * 0.56, oy + s * 0.82, ox + s * 0.34, oy + s * 0.62, rgb, 1.7);
        self.draw_line(ox + s * 0.34, oy + s * 0.62, ox + s * 0.15, oy + s * 0.62, rgb, 1.7);
        self.draw_line(ox + s * 0.15, oy + s * 0.38, ox + s * 0.15, oy + s * 0.62, rgb, 1.7);
        if muted {
            self.draw_line(ox + s * 0.66, oy + s * 0.36, ox + s * 0.88, oy + s * 0.64, rgb, 1.8);
            self.draw_line(ox + s * 0.88, oy + s * 0.36, ox + s * 0.66, oy + s * 0.64, rgb, 1.8);
        } else {
            self.draw_line(ox + s * 0.68, oy + s * 0.34, ox + s * 0.78, oy + s * 0.50, rgb, 1.7);
            self.draw_line(ox + s * 0.78, oy + s * 0.50, ox + s * 0.68, oy + s * 0.66, rgb, 1.7);
        }
    }

    pub fn draw_icon_record(&mut self, x: i32, y: i32, sz: i32, rgb: u32, active: bool) {
        let s = sz as f32;
        let cx = x as f32 + s * 0.5;
        let cy = y as f32 + s * 0.5;
        self.draw_circle(cx, cy, s * 0.36, rgb, 1.8, false);
        self.draw_circle(cx, cy, if active { s * 0.22 } else { s * 0.14 }, rgb, 1.0, true);
    }

    pub fn draw_icon_pen(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let ox = x as f32;
        let oy = y as f32;
        self.draw_line(ox + s * 0.20, oy + s * 0.80, ox + s * 0.74, oy + s * 0.26, rgb, 2.0);
        self.draw_line(ox + s * 0.16, oy + s * 0.84, ox + s * 0.32, oy + s * 0.78, rgb, 1.6);
        self.draw_line(ox + s * 0.16, oy + s * 0.84, ox + s * 0.22, oy + s * 0.68, rgb, 1.6);
    }

    pub fn draw_icon_zap(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let ox = x as f32;
        let oy = y as f32;
        self.draw_line(ox + s * 0.56, oy + s * 0.12, ox + s * 0.22, oy + s * 0.54, rgb, 1.8);
        self.draw_line(ox + s * 0.22, oy + s * 0.54, ox + s * 0.50, oy + s * 0.54, rgb, 1.8);
        self.draw_line(ox + s * 0.50, oy + s * 0.54, ox + s * 0.44, oy + s * 0.88, rgb, 1.8);
        self.draw_line(ox + s * 0.44, oy + s * 0.88, ox + s * 0.78, oy + s * 0.46, rgb, 1.8);
        self.draw_line(ox + s * 0.78, oy + s * 0.46, ox + s * 0.50, oy + s * 0.46, rgb, 1.8);
    }

    pub fn draw_icon_more(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let cy = y as f32 + s * 0.5;
        let ox = x as f32;
        self.draw_circle(ox + s * 0.24, cy, 1.6, rgb, 1.0, true);
        self.draw_circle(ox + s * 0.50, cy, 1.6, rgb, 1.0, true);
        self.draw_circle(ox + s * 0.76, cy, 1.6, rgb, 1.0, true);
    }

    pub fn draw_icon_radio(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let cx = x as f32 + s * 0.5;
        let cy = y as f32 + s * 0.5;
        self.draw_circle(cx, cy, s * 0.16, rgb, 1.0, true);
        self.draw_circle(cx, cy, s * 0.36, rgb, 1.5, false);
    }

    pub fn draw_icon_cpu(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let ox = x as f32;
        let oy = y as f32;
        self.draw_line(ox + s * 0.24, oy + s * 0.24, ox + s * 0.76, oy + s * 0.24, rgb, 1.7);
        self.draw_line(ox + s * 0.76, oy + s * 0.24, ox + s * 0.76, oy + s * 0.76, rgb, 1.7);
        self.draw_line(ox + s * 0.76, oy + s * 0.76, ox + s * 0.24, oy + s * 0.76, rgb, 1.7);
        self.draw_line(ox + s * 0.24, oy + s * 0.76, ox + s * 0.24, oy + s * 0.24, rgb, 1.7);
    }

    pub fn draw_icon_info(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let cx = x as f32 + s * 0.5;
        let cy = y as f32 + s * 0.5;
        self.draw_circle(cx, cy, s * 0.38, rgb, 1.7, false);
        self.draw_line(cx, cy - s * 0.04, cx, cy + s * 0.22, rgb, 1.8);
        self.draw_circle(cx, cy - s * 0.18, 1.3, rgb, 1.0, true);
    }

    pub fn draw_icon_trash(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let ox = x as f32;
        let oy = y as f32;
        self.draw_line(ox + s * 0.18, oy + s * 0.26, ox + s * 0.82, oy + s * 0.26, rgb, 1.7);
        self.draw_line(ox + s * 0.28, oy + s * 0.26, ox + s * 0.28, oy + s * 0.84, rgb, 1.7);
        self.draw_line(ox + s * 0.72, oy + s * 0.26, ox + s * 0.72, oy + s * 0.84, rgb, 1.7);
        self.draw_line(ox + s * 0.28, oy + s * 0.84, ox + s * 0.72, oy + s * 0.84, rgb, 1.7);
    }

    pub fn draw_icon_refresh(&mut self, x: i32, y: i32, sz: i32, rgb: u32) {
        let s = sz as f32;
        let cx = x as f32 + s * 0.5;
        let cy = y as f32 + s * 0.5;
        self.draw_circle(cx, cy, s * 0.34, rgb, 1.8, false);
        self.draw_line(cx + s * 0.12, cy - s * 0.34, cx + s * 0.36, cy - s * 0.34, rgb, 1.8);
        self.draw_line(cx + s * 0.36, cy - s * 0.34, cx + s * 0.36, cy - s * 0.10, rgb, 1.8);
    }

    /// Measures the pixel width and height of a single line of text
    pub fn measure_text(&self, text: &str, size_px: f32, bold: bool) -> (i32, i32) {
        let f_sys = fonts();
        let font_opt = if bold {
            f_sys.bold.as_ref()
        } else {
            f_sys.regular.as_ref()
        };
        let Some(font) = font_opt else {
            return ((text.len() as i32) * 8, size_px as i32);
        };

        let scale = PxScale::from(size_px);
        let scaled = font.as_scaled(scale);
        let mut width = 0.0f32;
        let mut prev: Option<ab_glyph::GlyphId> = None;
        for ch in text.chars() {
            let gid = scaled.glyph_id(ch);
            if let Some(p) = prev {
                width += scaled.kern(p, gid);
            }
            width += scaled.h_advance(gid);
            prev = Some(gid);
        }
        let height = (scaled.ascent() - scaled.descent()).ceil() as i32;
        (width.ceil() as i32, height)
    }

    /// Draws anti-aliased text with optional clipping width
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &mut self,
        x: i32,
        y: i32,
        text: &str,
        size_px: f32,
        bold: bool,
        rgb: u32,
        max_width: Option<i32>,
    ) {
        let f_sys = fonts();
        let font_opt = if bold {
            f_sys.bold.as_ref()
        } else {
            f_sys.regular.as_ref()
        };
        let Some(font) = font_opt else {
            return;
        };

        let scale = PxScale::from(size_px);
        let scaled = font.as_scaled(scale);
        let baseline_y = y as f32 + scaled.ascent();
        let limit_x = max_width.map(|mw| (x + mw) as f32);

        let mut caret_x = x as f32;
        let mut prev: Option<ab_glyph::GlyphId> = None;

        for ch in text.chars() {
            let gid = scaled.glyph_id(ch);
            if let Some(p) = prev {
                caret_x += scaled.kern(p, gid);
            }
            let advance = scaled.h_advance(gid);
            if let Some(lx) = limit_x {
                if caret_x + advance > lx {
                    break;
                }
            }

            let glyph = gid.with_scale_and_position(scale, point(caret_x, baseline_y));
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, cov| {
                    if cov > 0.02 {
                        let px = bounds.min.x as i32 + gx as i32;
                        let py = bounds.min.y as i32 + gy as i32;
                        self.blend_pixel(px, py, rgb, cov);
                    }
                });
            }

            caret_x += advance;
            prev = Some(gid);
        }
    }

    /// Draws text centered both horizontally and vertically inside `(rx, ry, rw, rh)`
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_centered(
        &mut self,
        rx: i32,
        ry: i32,
        rw: i32,
        rh: i32,
        text: &str,
        size_px: f32,
        bold: bool,
        rgb: u32,
    ) {
        let (tw, th) = self.measure_text(text, size_px, bold);
        let tx = rx + ((rw - tw) / 2).max(2);
        let ty = ry + (rh - th) / 2;
        self.draw_text(tx, ty, text, size_px, bold, rgb, Some(rw - 4));
    }

    /// Draws text left-aligned and vertically centered inside `(rx, ry, rw, rh)`
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_vcenter(
        &mut self,
        rx: i32,
        ry: i32,
        rw: i32,
        rh: i32,
        text: &str,
        size_px: f32,
        bold: bool,
        rgb: u32,
    ) {
        let (_, th) = self.measure_text(text, size_px, bold);
        let ty = ry + (rh - th) / 2;
        self.draw_text(rx, ty, text, size_px, bold, rgb, Some(rw.max(10)));
    }

    /// Draws multi-line word-wrapped text inside `(rx, ry, rw, rh)`
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_wrapped(
        &mut self,
        rx: i32,
        ry: i32,
        rw: i32,
        rh: i32,
        text: &str,
        size_px: f32,
        bold: bool,
        rgb: u32,
    ) {
        let line_h = (size_px * 1.38) as i32;
        let mut cur_y = ry;
        let mut current_line = String::new();

        for word in text.split_whitespace() {
            let candidate = if current_line.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", current_line, word)
            };
            let (cw, _) = self.measure_text(&candidate, size_px, bold);
            if cw > rw && !current_line.is_empty() {
                self.draw_text(rx, cur_y, &current_line, size_px, bold, rgb, Some(rw));
                cur_y += line_h;
                if cur_y + line_h > ry + rh {
                    return;
                }
                current_line = word.to_string();
            } else {
                current_line = candidate;
            }
        }

        if !current_line.is_empty() && cur_y <= ry + rh {
            self.draw_text(rx, cur_y, &current_line, size_px, bold, rgb, Some(rw));
        }
    }

    /// Blits the entire software canvas to an X11 Drawable (Window or Pixmap) in safe chunks
    pub fn present_to_x11<C: Connection>(
        &self,
        conn: &C,
        drawable: Drawable,
        gc: Gcontext,
        depth: u8,
    ) {
        let w = self.width as usize;
        let h = self.height as usize;
        if w == 0 || h == 0 {
            return;
        }
        let max_rows_per_chunk = (65536 / (w * 4)).max(1);
        let mut row_start = 0;
        while row_start < h {
            let chunk_h = (h - row_start).min(max_rows_per_chunk);
            let byte_start = row_start * w * 4;
            let byte_end = (row_start + chunk_h) * w * 4;
            let _ = conn.put_image(
                ImageFormat::Z_PIXMAP,
                drawable,
                gc,
                w as u16,
                chunk_h as u16,
                0,
                row_start as i16,
                0,
                depth,
                &self.data[byte_start..byte_end],
            );
            row_start += chunk_h;
        }
    }
}

#[inline]
fn lerp_rgb(c1: u32, c2: u32, t: f32) -> u32 {
    let r1 = ((c1 >> 16) & 0xFF) as f32;
    let g1 = ((c1 >> 8) & 0xFF) as f32;
    let b1 = (c1 & 0xFF) as f32;

    let r2 = ((c2 >> 16) & 0xFF) as f32;
    let g2 = ((c2 >> 8) & 0xFF) as f32;
    let b2 = (c2 & 0xFF) as f32;

    let r = (r1 + (r2 - r1) * t) as u32;
    let g = (g1 + (g2 - g1) * t) as u32;
    let b = (b1 + (b2 - b1) * t) as u32;
    (r << 16) | (g << 8) | b
}

pub fn copy_to_clipboard(text: &str) {
    crate::linux::write_clipboard_text(text);
}

pub fn format_bitrate(kbps: u32) -> String {
    if kbps >= 1000 {
        format!("{:.1} Mbps", kbps as f32 / 1000.0)
    } else if kbps > 0 {
        format!("{} kbps", kbps)
    } else {
        "Measuring...".to_string()
    }
}
