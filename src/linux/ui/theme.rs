//! pkasoft.com White Theme Styling & X11 Graphics Helpers

#![allow(dead_code)]

use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

// pkasoft.com White Theme color palette (X11 TrueColor RGB: 0x00RRGGBB)
pub const PK_BG: u32 = 0x00F5F7F5;          // #f5f7f5 (--paper soft clean canvas)
pub const PK_CARD: u32 = 0x00FFFFFF;        // #ffffff (pure crisp white card)
pub const PK_CARD_BORDER: u32 = 0x00E2E7E3; // #e2e7e3 (--line border)
pub const PK_GREEN: u32 = 0x002D6A4F;       // #2d6a4f (--green signature emerald)
pub const PK_GREEN_DARK: u32 = 0x001B4332;  // #1b4332 (--green-dark pine hover)
pub const PK_GREEN_TINT: u32 = 0x00EEF5F0;  // #eef5f0 (subtle green pill fill)
pub const PK_TEXT: u32 = 0x001D2420;        // #1d2420 (--ink deep crisp charcoal)
pub const PK_MUTED: u32 = 0x005D6A63;       // #5d6a63 (--muted slate sage)
pub const PK_INPUT_BG: u32 = 0x00FAFCFA;    // #fafcfa (soft clean input background)
pub const PK_BTN_HOVER: u32 = 0x00E7EDE5;   // #e7ede5 (hover outline button)
pub const PK_RED: u32 = 0x00C53030;         // #c53030 (crimson decline button)
pub const PK_RED_TINT: u32 = 0x00FDF2F2;    // #fdf2f2 (subtle red alert fill)
pub const PK_DARK_VIEWPORT: u32 = 0x000E1210; // Dark canvas background for remote stream

pub fn open_best_font<C: Connection>(conn: &C, candidates: &[&str]) -> Option<Font> {
    for &name in candidates {
        if let Ok(fid) = conn.generate_id() {
            if let Ok(cookie) = conn.open_font(fid, name.as_bytes()) {
                if cookie.check().is_ok() {
                    return Some(fid);
                }
            }
        }
    }
    None
}

pub fn create_color_gc<C: Connection>(
    conn: &C,
    drawable: Drawable,
    fg: u32,
    bg: u32,
    font: Option<Font>,
) -> Gcontext {
    let gc = conn.generate_id().unwrap_or(0);
    let mut aux = CreateGCAux::new()
        .foreground(fg)
        .background(bg)
        .graphics_exposures(0);
    if let Some(f) = font {
        aux = aux.font(f);
    }
    let _ = conn.create_gc(gc, drawable, &aux);
    gc
}

pub struct X11Palette {
    pub gc_bg: Gcontext,
    pub gc_card: Gcontext,
    pub gc_input_bg: Gcontext,
    pub gc_border: Gcontext,
    pub gc_green: Gcontext,
    pub gc_green_tint: Gcontext,
    pub gc_red: Gcontext,
    pub gc_red_tint: Gcontext,
    pub gc_dark_vp: Gcontext,

    // Text GCs (foreground + background for image_text8)
    pub txt_dark_on_card: Gcontext,
    pub txt_dark_on_input: Gcontext,
    pub txt_muted_on_card: Gcontext,
    pub txt_muted_on_input: Gcontext,
    pub txt_green_on_card: Gcontext,
    pub txt_green_on_tint: Gcontext,
    pub txt_white_on_green: Gcontext,
    pub txt_white_on_red: Gcontext,
    pub txt_red_on_card: Gcontext,
    pub txt_red_on_tint: Gcontext,

    // Large ID Text GCs
    pub txt_large_green_on_input: Gcontext,
    pub txt_large_dark_on_input: Gcontext,
    pub txt_large_dark_on_card: Gcontext,
}

impl X11Palette {
    pub fn new<C: Connection>(conn: &C, win_id: Window) -> Self {
        let font_norm = open_best_font(
            conn,
            &[
                "-adobe-helvetica-medium-r-normal--14-*",
                "9x15",
                "8x13",
                "fixed",
            ],
        );
        let font_bold = open_best_font(
            conn,
            &[
                "-adobe-helvetica-bold-r-normal--14-*",
                "9x15bold",
                "9x15",
                "fixed",
            ],
        )
        .or(font_norm);
        let font_large = open_best_font(
            conn,
            &[
                "-adobe-helvetica-bold-r-normal--24-*",
                "-adobe-helvetica-bold-r-normal--18-*",
                "10x20",
                "9x15bold",
                "fixed",
            ],
        )
        .or(font_bold);

        Self {
            gc_bg: create_color_gc(conn, win_id, PK_BG, PK_BG, font_norm),
            gc_card: create_color_gc(conn, win_id, PK_CARD, PK_CARD, font_norm),
            gc_input_bg: create_color_gc(conn, win_id, PK_INPUT_BG, PK_INPUT_BG, font_norm),
            gc_border: create_color_gc(conn, win_id, PK_CARD_BORDER, PK_CARD, font_norm),
            gc_green: create_color_gc(conn, win_id, PK_GREEN, PK_GREEN, font_bold),
            gc_green_tint: create_color_gc(conn, win_id, PK_GREEN_TINT, PK_GREEN_TINT, font_bold),
            gc_red: create_color_gc(conn, win_id, PK_RED, PK_RED, font_bold),
            gc_red_tint: create_color_gc(conn, win_id, PK_RED_TINT, PK_RED_TINT, font_bold),
            gc_dark_vp: create_color_gc(conn, win_id, PK_DARK_VIEWPORT, PK_DARK_VIEWPORT, font_norm),

            txt_dark_on_card: create_color_gc(conn, win_id, PK_TEXT, PK_CARD, font_bold),
            txt_dark_on_input: create_color_gc(conn, win_id, PK_TEXT, PK_INPUT_BG, font_norm),
            txt_muted_on_card: create_color_gc(conn, win_id, PK_MUTED, PK_CARD, font_norm),
            txt_muted_on_input: create_color_gc(conn, win_id, PK_MUTED, PK_INPUT_BG, font_norm),
            txt_green_on_card: create_color_gc(conn, win_id, PK_GREEN, PK_CARD, font_bold),
            txt_green_on_tint: create_color_gc(conn, win_id, PK_GREEN_DARK, PK_GREEN_TINT, font_bold),
            txt_white_on_green: create_color_gc(conn, win_id, PK_CARD, PK_GREEN, font_bold),
            txt_white_on_red: create_color_gc(conn, win_id, PK_CARD, PK_RED, font_bold),
            txt_red_on_card: create_color_gc(conn, win_id, PK_RED, PK_CARD, font_bold),
            txt_red_on_tint: create_color_gc(conn, win_id, PK_RED, PK_RED_TINT, font_bold),

            txt_large_green_on_input: create_color_gc(
                conn,
                win_id,
                PK_GREEN,
                PK_INPUT_BG,
                font_large,
            ),
            txt_large_dark_on_input: create_color_gc(
                conn,
                win_id,
                PK_TEXT,
                PK_INPUT_BG,
                font_large,
            ),
            txt_large_dark_on_card: create_color_gc(
                conn,
                win_id,
                PK_TEXT,
                PK_CARD,
                font_large,
            ),
        }
    }
}

pub fn fill_bordered_box<C: Connection>(
    conn: &C,
    drawable: Drawable,
    x: i16,
    y: i16,
    w: u16,
    h: u16,
    fill_gc: Gcontext,
    border_gc: Gcontext,
) {
    if w == 0 || h == 0 {
        return;
    }
    let r = Rectangle {
        x,
        y,
        width: w,
        height: h,
    };
    let _ = conn.poly_fill_rectangle(drawable, fill_gc, &[r]);
    let b = Rectangle {
        x,
        y,
        width: w.saturating_sub(1),
        height: h.saturating_sub(1),
    };
    let _ = conn.poly_rectangle(drawable, border_gc, &[b]);
}

pub fn draw_str<C: Connection>(
    conn: &C,
    drawable: Drawable,
    gc: Gcontext,
    x: i16,
    y: i16,
    text: &str,
) {
    let bytes: Vec<u8> = text
        .chars()
        .map(|c| if (c as u32) < 0x100 { c as u8 } else { b'?' })
        .take(200)
        .collect();
    if !bytes.is_empty() {
        let _ = conn.image_text8(drawable, gc, x, y, &bytes);
    }
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
