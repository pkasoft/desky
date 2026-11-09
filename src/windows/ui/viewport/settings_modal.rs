//! Full-Screen Settings View & Diagnostics Page

#![allow(unsafe_op_in_unsafe_fn)]

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use windows::Win32::Foundation::{COLORREF, HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, DeleteObject, DrawTextW, FillRect, SelectObject, SetBkMode, SetTextColor,
    DT_CENTER, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, FW_BOLD, FW_SEMIBOLD, HDC, HGDIOBJ,
    InvalidateRect, TRANSPARENT,
};

use super::theme::{
    create_segoe_font, draw_rounded_box, format_bitrate, PK_BG, PK_CARD,
    PK_CARD_BORDER, PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_RED, PK_TEXT,
};

// Full-Screen Settings Page state
pub static SHOW_SETTINGS_MODAL: AtomicBool = AtomicBool::new(false);

// Active AnyDesk-style settings sidebar category:
// 0 = Stream & Display, 1 = Connection Info
pub static SETTINGS_CATEGORY: AtomicU8 = AtomicU8::new(0);

// Display mode: 0 = Auto Fit, 1 = Fill Window, 2 = 1:1 Original resolution
pub static DISPLAY_MODE: AtomicU8 = AtomicU8::new(0);

// Target FPS: 30, 60, 120
pub static TARGET_FPS: AtomicU32 = AtomicU32::new(60);

// Remote cursor capture toggle
pub static CAPTURE_CURSOR: AtomicBool = AtomicBool::new(true);

pub unsafe fn draw_fullscreen_settings_view(
    hdc: HDC,
    client_w: i32,
    client_h: i32,
    mode: u8,
    fps: u32,
    top_y: i32,
    orig_w: u32,
    orig_h: u32,
) {
    // 1. Full-Screen Canvas (PK_BG)
    let full_rect = RECT {
        left: 0,
        top: top_y,
        right: client_w,
        bottom: client_h,
    };
    let bg_brush = CreateSolidBrush(PK_BG);
    FillRect(hdc, &full_rect, bg_brush);
    let _ = DeleteObject(HGDIOBJ(bg_brush.0));

    let line_brush = CreateSolidBrush(PK_CARD_BORDER);
    let green_brush = CreateSolidBrush(PK_GREEN);
    let green_dark_brush = CreateSolidBrush(PK_GREEN_DARK);
    let input_brush = CreateSolidBrush(PK_INPUT_BG);

    let font_title = create_segoe_font(16, FW_BOLD);
    let font_lbl = create_segoe_font(13, FW_SEMIBOLD);
    let font_btn = create_segoe_font(12, FW_SEMIBOLD);

    SetBkMode(hdc, TRANSPARENT);

    let panel_w = (client_w - 48).clamp(320, 620);
    let panel_x = (client_w - panel_w) / 2;

    // Top Navigation & Close Bar
    let tab_y = top_y + 14;
    let tab_h = 32;
    let close_w = 84;
    let close_x = panel_x + panel_w - close_w;
    let cat_w = (panel_w - close_w - 24) / 3;
    let cat0_x = panel_x;
    let cat1_x = cat0_x + cat_w + 6;
    let cat2_x = cat1_x + cat_w + 6;

    let active_cat = SETTINGS_CATEGORY.load(Ordering::Relaxed).min(2);

    // Category 0: Stream & Display Tab Button
    let (c0_bg, c0_border, c0_fg) = if active_cat == 0 {
        (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
    } else {
        (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
    };
    draw_rounded_box(hdc, cat0_x, tab_y, cat_w, tab_h, c0_bg, c0_border, 6);
    let old_font = SelectObject(hdc, HGDIOBJ(font_btn.0));
    SetTextColor(hdc, c0_fg);
    let mut c0_txt: Vec<u16> = "Stream & Display".encode_utf16().collect();
    let mut c0_rc = RECT {
        left: cat0_x,
        top: tab_y,
        right: cat0_x + cat_w,
        bottom: tab_y + tab_h,
    };
    let _ = DrawTextW(hdc, &mut c0_txt, &mut c0_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    // Category 1: Security & Unattended Tab Button
    let (c1_bg, c1_border, c1_fg) = if active_cat == 1 {
        (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
    } else {
        (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
    };
    draw_rounded_box(hdc, cat1_x, tab_y, cat_w, tab_h, c1_bg, c1_border, 6);
    SetTextColor(hdc, c1_fg);
    let mut c1_txt: Vec<u16> = "Security".encode_utf16().collect();
    let mut c1_rc = RECT {
        left: cat1_x,
        top: tab_y,
        right: cat1_x + cat_w,
        bottom: tab_y + tab_h,
    };
    let _ = DrawTextW(hdc, &mut c1_txt, &mut c1_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    // Category 2: Connection Info Tab Button
    let (c2_bg, c2_border, c2_fg) = if active_cat == 2 {
        (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
    } else {
        (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
    };
    draw_rounded_box(hdc, cat2_x, tab_y, cat_w, tab_h, c2_bg, c2_border, 6);
    SetTextColor(hdc, c2_fg);
    let mut c2_txt: Vec<u16> = "Connection Info".encode_utf16().collect();
    let mut c2_rc = RECT {
        left: cat2_x,
        top: tab_y,
        right: cat2_x + cat_w,
        bottom: tab_y + tab_h,
    };
    let _ = DrawTextW(hdc, &mut c2_txt, &mut c2_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    // Close Button [ Close × ]
    draw_rounded_box(hdc, close_x, tab_y, close_w, tab_h, PK_CARD, PK_CARD_BORDER, 6);
    SetTextColor(hdc, PK_MUTED);
    let mut close_txt: Vec<u16> = "Close ×".encode_utf16().collect();
    let mut close_rc = RECT {
        left: close_x,
        top: tab_y,
        right: close_x + close_w,
        bottom: tab_y + tab_h,
    };
    let _ = DrawTextW(hdc, &mut close_txt, &mut close_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    let draw_segmented_strip = |x: i32, y: i32, w: i32, h: i32, labels: &[&str], active_idx: usize| {
        let count = labels.len().max(1) as i32;
        let seg_w = w / count;

        draw_rounded_box(hdc, x, y, w, h, PK_INPUT_BG, PK_CARD_BORDER, 6);

        let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
        for (i, label) in labels.iter().enumerate() {
            let sx = x + (i as i32) * seg_w;
            let sr = if i as i32 == count - 1 { x + w } else { sx + seg_w };
            let is_active = i == active_idx;

            let cell_rc = RECT {
                left: sx + 1,
                top: y + 1,
                right: sr - 1,
                bottom: y + h - 1,
            };
            if is_active {
                FillRect(hdc, &cell_rc, green_brush);
                SetTextColor(hdc, COLORREF(0x00FFFFFF));
            } else {
                FillRect(hdc, &cell_rc, input_brush);
                SetTextColor(hdc, PK_MUTED);
            }

            if (i as i32) < count - 1 {
                let div_rc = RECT {
                    left: sr - 1,
                    top: y + 4,
                    right: sr,
                    bottom: y + h - 4,
                };
                FillRect(hdc, &div_rc, line_brush);
            }

            let mut lw: Vec<u16> = label.encode_utf16().collect();
            let mut lrc = cell_rc;
            let _ = DrawTextW(hdc, &mut lw, &mut lrc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);
        }
    };

    if active_cat == 0 {
        let sec0_y = top_y + 60;
        let strip_h = 36;
        let step = 74;

        // 1. Display Scaling
        let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
        SetTextColor(hdc, PK_TEXT);
        let mut s0_lbl: Vec<u16> = "Display Scaling".encode_utf16().collect();
        let mut s0_rc = RECT {
            left: panel_x,
            top: sec0_y,
            right: panel_x + panel_w,
            bottom: sec0_y + 20,
        };
        let _ = DrawTextW(hdc, &mut s0_lbl, &mut s0_rc, DT_SINGLELINE | DT_LEFT | DT_VCENTER);

        let scale_labels = ["Auto Fit", "Stretch", "Original (1:1)"];
        draw_segmented_strip(panel_x, sec0_y + 22, panel_w, strip_h, &scale_labels, (mode as usize).min(2));

        // 2. Stream Quality
        let sec1_y = sec0_y + step;
        let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
        SetTextColor(hdc, PK_TEXT);
        let mut s1_lbl: Vec<u16> = "Stream Quality".encode_utf16().collect();
        let mut s1_rc = RECT {
            left: panel_x,
            top: sec1_y,
            right: panel_x + panel_w,
            bottom: sec1_y + 20,
        };
        let _ = DrawTextW(hdc, &mut s1_lbl, &mut s1_rc, DT_SINGLELINE | DT_LEFT | DT_VCENTER);

        let q_mode = crate::p2p::get_quality_mode() as usize;
        let q_labels = ["Auto (ABR)", "High Quality", "Balanced", "Low Data"];
        let q_idx = match q_mode {
            0 => 0,
            1 | 2 => 1,
            3 => 2,
            _ => 3,
        };
        draw_segmented_strip(panel_x, sec1_y + 22, panel_w, strip_h, &q_labels, q_idx);

        // 3. Target Framerate
        let sec2_y = sec1_y + step;
        let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
        SetTextColor(hdc, PK_TEXT);
        let mut s2_lbl: Vec<u16> = "Target Framerate".encode_utf16().collect();
        let mut s2_rc = RECT {
            left: panel_x,
            top: sec2_y,
            right: panel_x + panel_w,
            bottom: sec2_y + 20,
        };
        let _ = DrawTextW(hdc, &mut s2_lbl, &mut s2_rc, DT_SINGLELINE | DT_LEFT | DT_VCENTER);

        let fps_labels = ["30 FPS", "60 FPS", "120 FPS"];
        let fps_idx = match fps {
            30 => 0,
            120 => 2,
            _ => 1,
        };
        draw_segmented_strip(panel_x, sec2_y + 22, panel_w, strip_h, &fps_labels, fps_idx);

        // 4. Remote Cursor
        let sec3_y = sec2_y + step;
        let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
        SetTextColor(hdc, PK_TEXT);
        let mut s3_lbl: Vec<u16> = "Remote Cursor".encode_utf16().collect();
        let mut s3_rc = RECT {
            left: panel_x,
            top: sec3_y,
            right: panel_x + panel_w,
            bottom: sec3_y + 20,
        };
        let _ = DrawTextW(hdc, &mut s3_lbl, &mut s3_rc, DT_SINGLELINE | DT_LEFT | DT_VCENTER);

        let cur_labels = ["Show Cursor", "Hide Cursor"];
        let cursor_on = CAPTURE_CURSOR.load(Ordering::Relaxed);
        draw_segmented_strip(panel_x, sec3_y + 22, panel_w, strip_h, &cur_labels, if cursor_on { 0 } else { 1 });

        // Bottom [ Done ] Button
        let done_y = sec3_y + step + 6;
        let done_w = 140;
        let done_x = (client_w - done_w) / 2;
        let done_h = 36;
        draw_rounded_box(hdc, done_x, done_y, done_w, done_h, PK_GREEN, PK_GREEN_DARK, 6);
        let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
        SetTextColor(hdc, COLORREF(0x00FFFFFF));
        let mut done_txt: Vec<u16> = "Done".encode_utf16().collect();
        let mut done_rc = RECT {
            left: done_x,
            top: done_y,
            right: done_x + done_w,
            bottom: done_y + done_h,
        };
        let _ = DrawTextW(hdc, &mut done_txt, &mut done_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);
    } else if active_cat == 1 {
        // Category 1: Unattended Security
        let is_unattended = crate::identity::is_unattended_enabled();
        let sec_y = top_y + 60;

        // Card 1: Status box
        draw_rounded_box(hdc, panel_x, sec_y, panel_w, 76, PK_INPUT_BG, PK_CARD_BORDER, 8);
        crate::ui::draw_lock(hdc, panel_x + 16, sec_y + 16, 24, if is_unattended { PK_GREEN } else { PK_MUTED }, 2);

        let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
        SetTextColor(hdc, if is_unattended { PK_GREEN_DARK } else { PK_TEXT });
        let status_title = if is_unattended { "Unattended Access: ACTIVE" } else { "Unattended Access: DISABLED" };
        let mut st_w: Vec<u16> = status_title.encode_utf16().collect();
        let mut st_rc = RECT { left: panel_x + 50, top: sec_y + 10, right: panel_x + panel_w - 16, bottom: sec_y + 34 };
        let _ = DrawTextW(hdc, &mut st_w, &mut st_rc, DT_SINGLELINE | DT_VCENTER);

        let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
        SetTextColor(hdc, PK_MUTED);
        let status_desc = if is_unattended {
            "Permanent master password is set with salted SHA-256 hash. Authorized remote desks authenticate automatically."
        } else {
            "Unattended access is off. Incoming connection requests must be manually accepted at this desk."
        };
        let mut sd_w: Vec<u16> = status_desc.encode_utf16().collect();
        let mut sd_rc = RECT { left: panel_x + 50, top: sec_y + 36, right: panel_x + panel_w - 16, bottom: sec_y + 70 };
        let _ = DrawTextW(hdc, &mut sd_w, &mut sd_rc, DT_WORDBREAK);

        // Card 2: Action Toggle Button
        let act_y = sec_y + 92;
        let btn_w = 230;
        let btn_h = 40;
        let btn_x = panel_x + (panel_w - btn_w) / 2;

        if is_unattended {
            draw_rounded_box(hdc, btn_x, act_y, btn_w, btn_h, COLORREF(0x00EBF0FF), PK_RED, 6);
            let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
            SetTextColor(hdc, PK_RED);
            let mut btn_txt: Vec<u16> = "Disable Unattended Access".encode_utf16().collect();
            let mut btn_rc = RECT { left: btn_x, top: act_y, right: btn_x + btn_w, bottom: act_y + btn_h };
            let _ = DrawTextW(hdc, &mut btn_txt, &mut btn_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
        } else {
            draw_rounded_box(hdc, btn_x, act_y, btn_w, btn_h, PK_GREEN_TINT, PK_GREEN, 6);
            let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
            SetTextColor(hdc, PK_GREEN_DARK);
            let mut btn_txt: Vec<u16> = "Enable Master Password".encode_utf16().collect();
            let mut btn_rc = RECT { left: btn_x, top: act_y, right: btn_x + btn_w, bottom: act_y + btn_h };
            let _ = DrawTextW(hdc, &mut btn_txt, &mut btn_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
        }

        // Bottom [ Done ] Button
        let done_y = act_y + 64;
        let done_w = 140;
        let done_x = (client_w - done_w) / 2;
        let done_h = 36;
        draw_rounded_box(hdc, done_x, done_y, done_w, done_h, PK_GREEN, PK_GREEN_DARK, 6);
        let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
        SetTextColor(hdc, COLORREF(0x00FFFFFF));
        let mut done_txt: Vec<u16> = "Done".encode_utf16().collect();
        let mut done_rc = RECT { left: done_x, top: done_y, right: done_x + done_w, bottom: done_y + done_h };
        let _ = DrawTextW(hdc, &mut done_txt, &mut done_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);
    } else {
        let (_active_q, active_scale, _active_c444) = crate::p2p::get_active_quality_params();
        let bitrate_str = format_bitrate(crate::p2p::get_session_bitrate_kbps());
        let rtt_ms = crate::p2p::get_session_rtt_ms();
        let res_str = if orig_w > 0 && orig_h > 0 {
            format!("{}x{} ({}%)", orig_w, orig_h, active_scale)
        } else {
            format!("{}% Native", active_scale)
        };

        let rows = [
            (
                "Latency (Ping)",
                if rtt_ms > 0 {
                    format!("{} ms", rtt_ms)
                } else {
                    "Local LAN (<1 ms)".to_string()
                },
            ),
            ("Stream Bitrate", bitrate_str),
            ("Resolution", res_str),
            ("Encryption", "ChaCha20-Poly1305 E2EE".to_string()),
        ];

        let row_h = 40;
        let tbl_y = top_y + 60;
        for (i, (k, v)) in rows.iter().enumerate() {
            let ry = tbl_y + (i as i32) * (row_h + 4);
            let row_bg = if i % 2 == 0 { PK_CARD } else { PK_INPUT_BG };
            draw_rounded_box(hdc, panel_x, ry, panel_w, row_h, row_bg, PK_CARD_BORDER, 6);

            let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
            SetTextColor(hdc, PK_MUTED);
            let mut kw: Vec<u16> = k.encode_utf16().collect();
            let mut k_rc = RECT {
                left: panel_x + 16,
                top: ry,
                right: panel_x + 200,
                bottom: ry + row_h,
            };
            let _ = DrawTextW(hdc, &mut kw, &mut k_rc, DT_SINGLELINE | DT_VCENTER);

            let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
            SetTextColor(hdc, PK_GREEN_DARK);
            let mut vw: Vec<u16> = v.encode_utf16().collect();
            let mut v_rc = RECT {
                left: panel_x + 200,
                top: ry,
                right: panel_x + panel_w - 16,
                bottom: ry + row_h,
            };
            let _ = DrawTextW(hdc, &mut vw, &mut v_rc, DT_SINGLELINE | DT_RIGHT | DT_VCENTER);
        }

        // Bottom [ Done ] Button
        let done_y = tbl_y + 4 * (row_h + 4) + 24;
        let done_w = 140;
        let done_x = (client_w - done_w) / 2;
        let done_h = 36;
        draw_rounded_box(hdc, done_x, done_y, done_w, done_h, PK_GREEN, PK_GREEN_DARK, 6);
        let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
        SetTextColor(hdc, COLORREF(0x00FFFFFF));
        let mut done_txt: Vec<u16> = "Done".encode_utf16().collect();
        let mut done_rc = RECT {
            left: done_x,
            top: done_y,
            right: done_x + done_w,
            bottom: done_y + done_h,
        };
        let _ = DrawTextW(hdc, &mut done_txt, &mut done_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);
    }

    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(HGDIOBJ(line_brush.0));
    let _ = DeleteObject(HGDIOBJ(green_brush.0));
    let _ = DeleteObject(HGDIOBJ(green_dark_brush.0));
    let _ = DeleteObject(HGDIOBJ(input_brush.0));
    let _ = DeleteObject(HGDIOBJ(font_title.0));
    let _ = DeleteObject(HGDIOBJ(font_lbl.0));
    let _ = DeleteObject(HGDIOBJ(font_btn.0));
}

pub fn handle_settings_click(hwnd: HWND, client_w: i32, _client_h: i32, client_x: i32, client_y: i32, top_y: i32) -> bool {
    let panel_w = (client_w - 48).clamp(320, 620);
    let panel_x = (client_w - panel_w) / 2;

    let tab_y = top_y + 14;
    let tab_h = 32;
    let close_w = 84;
    let close_x = panel_x + panel_w - close_w;
    let cat_w = (panel_w - close_w - 24) / 3;
    let cat0_x = panel_x;
    let cat1_x = cat0_x + cat_w + 6;
    let cat2_x = cat1_x + cat_w + 6;

    // Close Button [ Close × ]
    if client_x >= close_x && client_x <= close_x + close_w && client_y >= tab_y && client_y <= tab_y + tab_h {
        SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
        unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
        return true;
    }

    // Category 0 Tab
    if client_x >= cat0_x && client_x <= cat0_x + cat_w && client_y >= tab_y && client_y <= tab_y + tab_h {
        SETTINGS_CATEGORY.store(0, Ordering::Relaxed);
        unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
        return true;
    }

    // Category 1 Tab
    if client_x >= cat1_x && client_x <= cat1_x + cat_w && client_y >= tab_y && client_y <= tab_y + tab_h {
        SETTINGS_CATEGORY.store(1, Ordering::Relaxed);
        unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
        return true;
    }

    // Category 2 Tab
    if client_x >= cat2_x && client_x <= cat2_x + cat_w && client_y >= tab_y && client_y <= tab_y + tab_h {
        SETTINGS_CATEGORY.store(2, Ordering::Relaxed);
        unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
        return true;
    }

    let active_cat = SETTINGS_CATEGORY.load(Ordering::Relaxed).min(2);

    if active_cat == 0 {
        let sec0_y = top_y + 60;
        let strip_h = 36;
        let step = 74;

        let hit_segment = |sy: i32, count: i32| -> Option<usize> {
            if client_x >= panel_x
                && client_x <= panel_x + panel_w
                && client_y >= sy
                && client_y <= sy + strip_h
            {
                let seg_w = (panel_w / count.max(1)).max(1);
                let idx = ((client_x - panel_x) / seg_w).clamp(0, count - 1) as usize;
                Some(idx)
            } else {
                None
            }
        };

        // 1. Display Scaling (3 options)
        if let Some(idx) = hit_segment(sec0_y + 22, 3) {
            DISPLAY_MODE.store(idx as u8, Ordering::Relaxed);
            unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
            return true;
        }

        // 2. Stream Quality (4 options)
        if let Some(idx) = hit_segment(sec0_y + step + 22, 4) {
            let mode_val = match idx {
                0 => 0,
                1 => 2,
                2 => 3,
                _ => 4,
            };
            crate::p2p::set_quality_mode(mode_val);
            unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
            return true;
        }

        // 3. Target Framerate (3 options)
        if let Some(idx) = hit_segment(sec0_y + step * 2 + 22, 3) {
            let fps_val = match idx {
                0 => 30,
                2 => 120,
                _ => 60,
            };
            TARGET_FPS.store(fps_val, Ordering::Relaxed);
            let cur = CAPTURE_CURSOR.load(Ordering::Relaxed);
            crate::p2p::update_client_preferences(fps_val, cur);
            unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
            return true;
        }

        // 4. Remote Cursor (2 options)
        if let Some(idx) = hit_segment(sec0_y + step * 3 + 22, 2) {
            let cur_val = idx == 0;
            CAPTURE_CURSOR.store(cur_val, Ordering::Relaxed);
            let fps = TARGET_FPS.load(Ordering::Relaxed);
            crate::p2p::update_client_preferences(fps, cur_val);
            unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
            return true;
        }

        // Done Button
        let done_y = sec0_y + step * 3 + step + 6;
        let done_w = 140;
        let done_x = (client_w - done_w) / 2;
        let done_h = 36;
        if client_x >= done_x && client_x <= done_x + done_w && client_y >= done_y && client_y <= done_y + done_h {
            SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
            unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
            return true;
        }
    } else if active_cat == 1 {
        let sec_y = top_y + 60;
        let act_y = sec_y + 92;
        let btn_w = 230;
        let btn_h = 40;
        let btn_x = panel_x + (panel_w - btn_w) / 2;

        if client_x >= btn_x && client_x <= btn_x + btn_w && client_y >= act_y && client_y <= act_y + btn_h {
            if crate::identity::is_unattended_enabled() {
                crate::identity::disable_unattended();
            } else {
                crate::identity::set_unattended_password("Desky@2026");
            }
            unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
            return true;
        }

        let done_y = act_y + 64;
        let done_w = 140;
        let done_x = (client_w - done_w) / 2;
        let done_h = 36;
        if client_x >= done_x && client_x <= done_x + done_w && client_y >= done_y && client_y <= done_y + done_h {
            SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
            unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
            return true;
        }
    } else {
        let row_h = 40;
        let tbl_y = top_y + 60;
        let done_y = tbl_y + 4 * (row_h + 4) + 24;
        let done_w = 140;
        let done_x = (client_w - done_w) / 2;
        let done_h = 36;
        if client_x >= done_x && client_x <= done_x + done_w && client_y >= done_y && client_y <= done_y + done_h {
            SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
            unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
            return true;
        }
    }

    false
}
