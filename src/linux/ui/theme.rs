//! pkasoft.com White Theme Styling, Anti-Aliased TrueType Font Engine & Software Canvas (Linux X11)
//!
//! Renders crisp anti-aliased TrueType typography (`DejaVu Sans` / `Liberation Sans` / `Ubuntu`)
//! and smooth rounded rectangles with 1px borders directly into an in-memory BGRA pixel buffer,
//! matching the Windows GDI ClearType pkasoft.com White Theme UI 1:1.

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

/// High-speed in-memory BGRA software canvas with anti-aliased TrueType font & rounded shape rendering
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

    /// Renders a smooth anti-aliased rounded rectangle with a 1px border
    #[allow(clippy::too_many_arguments)]
    pub fn draw_rounded_box(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        fill_rgb: u32,
        border_rgb: u32,
        radius: i32,
    ) {
        if w <= 0 || h <= 0 {
            return;
        }
        let r = radius.clamp(0, (w.min(h) / 2).max(0)) as f32;
        if r <= 1.0 {
            self.fill_rect(x, y, w, h, border_rgb);
            if w > 2 && h > 2 {
                self.fill_rect(x + 1, y + 1, w - 2, h - 2, fill_rgb);
            }
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

                // Fast path for interior non-corner pixels
                if lx >= r && lx <= wf - r && ly >= 1.5 && ly <= hf - 1.5 {
                    self.set_pixel(px, py, fill_rgb);
                    continue;
                }
                if ly >= r && ly <= hf - r && lx >= 1.5 && lx <= wf - 1.5 {
                    self.set_pixel(px, py, fill_rgb);
                    continue;
                }

                // Signed distance to rounded box
                let qx = (lx - wf * 0.5).abs() - (wf * 0.5 - r);
                let qy = (ly - hf * 0.5).abs() - (hf * 0.5 - r);
                let dist = (qx.max(0.0).hypot(qy.max(0.0))) + qx.max(qy).min(0.0) - r;

                if dist <= -1.2 {
                    self.set_pixel(px, py, fill_rgb);
                } else if dist <= 0.5 {
                    let outer_alpha = (0.5 - dist).clamp(0.0, 1.0);
                    let border_mix = (dist + 1.2).clamp(0.0, 1.0);
                    let color = lerp_rgb(fill_rgb, border_rgb, border_mix);
                    self.blend_pixel(px, py, color, outer_alpha);
                }
            }
        }
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
