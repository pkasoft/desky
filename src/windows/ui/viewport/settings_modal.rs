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

// Active settings sidebar category:
// 0 = Stream & Display, 1 = Codecs, 2 = Recording, 3 = Security, 4 = Connection Info
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

    let panel_w = (client_w - 48).clamp(340, 640);
    let panel_x = (client_w - panel_w) / 2;

    // Top Navigation & Close Bar (5 categories: Display, Codecs, Recording, Security, Info)
    let tab_y = top_y + 14;
    let tab_h = 32;
    let close_w = 76;
    let close_x = panel_x + panel_w - close_w;
    let cat_w = (panel_w - close_w - 30) / 5;
    let cat0_x = panel_x;
    let cat1_x = cat0_x + cat_w + 6;
    let cat2_x = cat1_x + cat_w + 6;
    let cat3_x = cat2_x + cat_w + 6;
    let cat4_x = cat3_x + cat_w + 6;

    let active_cat = SETTINGS_CATEGORY.load(Ordering::Relaxed).min(4);

    let old_font = SelectObject(hdc, HGDIOBJ(font_btn.0));

    // Category 0: Display Tab Button
    let (c0_bg, c0_border, c0_fg) = if active_cat == 0 {
        (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
    } else {
        (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
    };
    draw_rounded_box(hdc, cat0_x, tab_y, cat_w, tab_h, c0_bg, c0_border, 6);
    SetTextColor(hdc, c0_fg);
    let mut c0_txt: Vec<u16> = "Display".encode_utf16().collect();
    let mut c0_rc = RECT { left: cat0_x, top: tab_y, right: cat0_x + cat_w, bottom: tab_y + tab_h };
    let _ = DrawTextW(hdc, &mut c0_txt, &mut c0_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    // Category 1: Codecs Tab Button
    let (c1_bg, c1_border, c1_fg) = if active_cat == 1 {
        (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
    } else {
        (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
    };
    draw_rounded_box(hdc, cat1_x, tab_y, cat_w, tab_h, c1_bg, c1_border, 6);
    SetTextColor(hdc, c1_fg);
    let mut c1_txt: Vec<u16> = "Codecs".encode_utf16().collect();
    let mut c1_rc = RECT { left: cat1_x, top: tab_y, right: cat1_x + cat_w, bottom: tab_y + tab_h };
    let _ = DrawTextW(hdc, &mut c1_txt, &mut c1_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    // Category 2: Recording Tab Button
    let (c2_bg, c2_border, c2_fg) = if active_cat == 2 {
        (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
    } else {
        (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
    };
    draw_rounded_box(hdc, cat2_x, tab_y, cat_w, tab_h, c2_bg, c2_border, 6);
    SetTextColor(hdc, c2_fg);
    let mut c2_txt: Vec<u16> = "Recording".encode_utf16().collect();
    let mut c2_rc = RECT { left: cat2_x, top: tab_y, right: cat2_x + cat_w, bottom: tab_y + tab_h };
    let _ = DrawTextW(hdc, &mut c2_txt, &mut c2_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    // Category 3: Security Tab Button
    let (c3_bg, c3_border, c3_fg) = if active_cat == 3 {
        (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
    } else {
        (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
    };
    draw_rounded_box(hdc, cat3_x, tab_y, cat_w, tab_h, c3_bg, c3_border, 6);
    SetTextColor(hdc, c3_fg);
    let mut c3_txt: Vec<u16> = "Security".encode_utf16().collect();
    let mut c3_rc = RECT { left: cat3_x, top: tab_y, right: cat3_x + cat_w, bottom: tab_y + tab_h };
    let _ = DrawTextW(hdc, &mut c3_txt, &mut c3_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    // Category 4: Info Tab Button
    let (c4_bg, c4_border, c4_fg) = if active_cat == 4 {
        (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
    } else {
        (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
    };
    draw_rounded_box(hdc, cat4_x, tab_y, cat_w, tab_h, c4_bg, c4_border, 6);
    SetTextColor(hdc, c4_fg);
    let mut c4_txt: Vec<u16> = "Info".encode_utf16().collect();
    let mut c4_rc = RECT { left: cat4_x, top: tab_y, right: cat4_x + cat_w, bottom: tab_y + tab_h };
    let _ = DrawTextW(hdc, &mut c4_txt, &mut c4_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    // Close Button [ Close × ]
    draw_rounded_box(hdc, close_x, tab_y, close_w, tab_h, PK_CARD, PK_CARD_BORDER, 6);
    SetTextColor(hdc, PK_MUTED);
    let mut close_txt: Vec<u16> = "Close ×".encode_utf16().collect();
    let mut close_rc = RECT { left: close_x, top: tab_y, right: close_x + close_w, bottom: tab_y + tab_h };
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
        // Category 0: Display Settings
        let sec0_y = top_y + 60;
        let strip_h = 36;
        let step = 74;

        // 1. Display Scaling
        let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
        SetTextColor(hdc, PK_TEXT);
        let mut s0_lbl: Vec<u16> = "Display Scaling".encode_utf16().collect();
        let mut s0_rc = RECT { left: panel_x, top: sec0_y, right: panel_x + panel_w, bottom: sec0_y + 20 };
        let _ = DrawTextW(hdc, &mut s0_lbl, &mut s0_rc, DT_SINGLELINE | DT_LEFT | DT_VCENTER);

        let scale_labels = ["Auto Fit", "Stretch", "Original (1:1)"];
        draw_segmented_strip(panel_x, sec0_y + 22, panel_w, strip_h, &scale_labels, (mode as usize).min(2));

        // 2. Stream Quality
        let sec1_y = sec0_y + step;
        let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
        SetTextColor(hdc, PK_TEXT);
        let mut s1_lbl: Vec<u16> = "Stream Quality".encode_utf16().collect();
        let mut s1_rc = RECT { left: panel_x, top: sec1_y, right: panel_x + panel_w, bottom: sec1_y + 20 };
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
        let mut s2_rc = RECT { left: panel_x, top: sec2_y, right: panel_x + panel_w, bottom: sec2_y + 20 };
        let _ = DrawTextW(hdc, &mut s2_lbl, &mut s2_rc, DT_SINGLELINE | DT_LEFT | DT_VCENTER);

        let fps_labels = ["30 FPS (Low Data)", "60 FPS (Smooth)", "120 FPS (Ultra)"];
        let fps_idx = match fps {
            30 => 0,
            120 => 2,
            _ => 1,
        };
        draw_segmented_strip(panel_x, sec2_y + 22, panel_w, strip_h, &fps_labels, fps_idx);

        // 4. Remote Cursor Rendering
        let sec3_y = sec2_y + step;
        let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
        SetTextColor(hdc, PK_TEXT);
        let mut s3_lbl: Vec<u16> = "Remote Cursor Rendering".encode_utf16().collect();
        let mut s3_rc = RECT { left: panel_x, top: sec3_y, right: panel_x + panel_w, bottom: sec3_y + 20 };
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
        let mut done_rc = RECT { left: done_x, top: done_y, right: done_x + done_w, bottom: done_y + done_h };
        let _ = DrawTextW(hdc, &mut done_txt, &mut done_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);
    } else if active_cat == 1 {
        // Category 1: Codec Settings
        let sec0_y = top_y + 60;
        let strip_h = 36;
        let step = 74;

        // 1. Codec Selection
        let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
        SetTextColor(hdc, PK_TEXT);
        let mut s0_lbl: Vec<u16> = "Video Codec Selection & Hardware Offload".encode_utf16().collect();
        let mut s0_rc = RECT { left: panel_x, top: sec0_y, right: panel_x + panel_w, bottom: sec0_y + 20 };
        let _ = DrawTextW(hdc, &mut s0_lbl, &mut s0_rc, DT_SINGLELINE | DT_LEFT | DT_VCENTER);

        let codec_labels = ["Auto (Best)", "H.265 (HEVC)", "H.264 (AVC)", "Motion-JPEG"];
        let pref_idx = crate::p2p::get_client_codec_pref() as usize;
        draw_segmented_strip(panel_x, sec0_y + 22, panel_w, strip_h, &codec_labels, pref_idx.min(3));

        // 2. Active Session Pipeline Card
        let sec1_y = sec0_y + step;
        let card1_h = 60;
        draw_rounded_box(hdc, panel_x, sec1_y, panel_w, card1_h, PK_INPUT_BG, PK_CARD_BORDER, 6);

        let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
        SetTextColor(hdc, PK_GREEN_DARK);
        let active_name = crate::p2p::get_active_stream_codec_name();
        let active_title = format!("Active Transmission: {}", active_name);
        let mut at_w: Vec<u16> = active_title.encode_utf16().collect();
        let mut at_rc = RECT { left: panel_x + 12, top: sec1_y + 8, right: panel_x + panel_w - 12, bottom: sec1_y + 30 };
        let _ = DrawTextW(hdc, &mut at_w, &mut at_rc, DT_SINGLELINE | DT_VCENTER);

        let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
        SetTextColor(hdc, PK_MUTED);
        let desc = match crate::p2p::get_active_stream_codec() {
            0x16 => "Hardware MFT accelerated (NV12 with macroblock & stride boundary guards)",
            0x17 => "Hardware Transform accelerated (HEVC Main Profile 60 FPS zero-copy)",
            _ => "Universal baseline (Pure-Rust SIMD 4:4:4 Chroma, zero-latency JPEG)",
        };
        let mut d_w: Vec<u16> = desc.encode_utf16().collect();
        let mut d_rc = RECT { left: panel_x + 12, top: sec1_y + 32, right: panel_x + panel_w - 12, bottom: sec1_y + 54 };
        let _ = DrawTextW(hdc, &mut d_w, &mut d_rc, DT_SINGLELINE | DT_VCENTER);

        // 3. Hardware Capabilities Summary
        let sec2_y = sec1_y + card1_h + 14;
        let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
        SetTextColor(hdc, PK_TEXT);
        let mut s2_lbl: Vec<u16> = "Hardware Codec Capabilities".encode_utf16().collect();
        let mut s2_rc = RECT { left: panel_x, top: sec2_y, right: panel_x + panel_w, bottom: sec2_y + 20 };
        let _ = DrawTextW(hdc, &mut s2_lbl, &mut s2_rc, DT_SINGLELINE | DT_LEFT | DT_VCENTER);

        let h264_hw = crate::platform::codec::is_h264_hardware_available();
        let h265_hw = crate::platform::codec::is_h265_hardware_available();

        let card2_h = 76;
        draw_rounded_box(hdc, panel_x, sec2_y + 22, panel_w, card2_h, PK_INPUT_BG, PK_CARD_BORDER, 6);

        let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
        let cap_txt = format!(
            "• H.264 / AVC GPU: {}\n• H.265 / HEVC GPU: {}\n• Motion-JPEG: AVAILABLE (Pure-Rust SIMD 4:4:4 Universal)",
            if h264_hw { "AVAILABLE (Media Foundation GPU MFT)" } else { "UNAVAILABLE" },
            if h265_hw { "AVAILABLE (Synchronous GPU MFT)" } else { "STANDBY (Async Only / Extensions Needed)" },
        );
        SetTextColor(hdc, PK_TEXT);
        let mut c_w: Vec<u16> = cap_txt.encode_utf16().collect();
        let mut c_rc = RECT { left: panel_x + 12, top: sec2_y + 26, right: panel_x + panel_w - 12, bottom: sec2_y + 22 + card2_h - 4 };
        let _ = DrawTextW(hdc, &mut c_w, &mut c_rc, DT_WORDBREAK);

        // 4. Test Codecs Action Button & Result
        let sec3_y = sec2_y + 22 + card2_h + 14;
        let test_btn_w = 180;
        let test_btn_h = 36;
        let test_btn_x = panel_x + (panel_w - test_btn_w) / 2;
        draw_rounded_box(hdc, test_btn_x, sec3_y, test_btn_w, test_btn_h, PK_GREEN_TINT, PK_GREEN, 6);
        let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
        SetTextColor(hdc, PK_GREEN_DARK);
        let mut tb_txt: Vec<u16> = "Test All Codecs".encode_utf16().collect();
        let mut tb_rc = RECT { left: test_btn_x, top: sec3_y, right: test_btn_x + test_btn_w, bottom: sec3_y + test_btn_h };
        let _ = DrawTextW(hdc, &mut tb_txt, &mut tb_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

        let diag_str = crate::p2p::get_codec_diag_result();
        let mut diag_end_y = sec3_y + test_btn_h;
        if !diag_str.is_empty() {
            let diag_y = sec3_y + test_btn_h + 8;
            let diag_h = 34;
            draw_rounded_box(hdc, panel_x, diag_y, panel_w, diag_h, PK_INPUT_BG, PK_CARD_BORDER, 6);
            let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
            SetTextColor(hdc, PK_TEXT);
            let mut dg_w: Vec<u16> = diag_str.encode_utf16().collect();
            let mut dg_rc = RECT { left: panel_x + 8, top: diag_y, right: panel_x + panel_w - 8, bottom: diag_y + diag_h };
            let _ = DrawTextW(hdc, &mut dg_w, &mut dg_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);
            diag_end_y = diag_y + diag_h;
        }

        // Bottom [ Done ] Button
        let done_y = diag_end_y + 14;
        let done_w = 140;
        let done_x = (client_w - done_w) / 2;
        let done_h = 36;
        draw_rounded_box(hdc, done_x, done_y, done_w, done_h, PK_GREEN, PK_GREEN_DARK, 6);
        let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
        SetTextColor(hdc, COLORREF(0x00FFFFFF));
        let mut done_txt: Vec<u16> = "Done".encode_utf16().collect();
        let mut done_rc = RECT { left: done_x, top: done_y, right: done_x + done_w, bottom: done_y + done_h };
        let _ = DrawTextW(hdc, &mut done_txt, &mut done_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);
    } else if active_cat == 2 {
        // Category 2: Recording Settings
        let sec0_y = top_y + 60;
        let strip_h = 36;
        let step = 70;

        // 1. Audio Capture: Remote Media
        let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
        SetTextColor(hdc, PK_TEXT);
        let mut s0_lbl: Vec<u16> = "Target Machine Media Audio (Remote Loopback)".encode_utf16().collect();
        let mut s0_rc = RECT { left: panel_x, top: sec0_y, right: panel_x + panel_w, bottom: sec0_y + 20 };
        let _ = DrawTextW(hdc, &mut s0_lbl, &mut s0_rc, DT_SINGLELINE | DT_LEFT | DT_VCENTER);

        let rem_audio_on = crate::recorder::is_remote_audio_recording_enabled();
        let rem_labels = ["Capture Target Media (ON)", "Mute Target Audio"];
        draw_segmented_strip(panel_x, sec0_y + 22, panel_w, strip_h, &rem_labels, if rem_audio_on { 0 } else { 1 });

        // 2. Audio Capture: This Machine (Local)
        let sec1_y = sec0_y + step;
        let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
        SetTextColor(hdc, PK_TEXT);
        let mut s1_lbl: Vec<u16> = "This Machine Audio (Local Microphone / Host)".encode_utf16().collect();
        let mut s1_rc = RECT { left: panel_x, top: sec1_y, right: panel_x + panel_w, bottom: sec1_y + 20 };
        let _ = DrawTextW(hdc, &mut s1_lbl, &mut s1_rc, DT_SINGLELINE | DT_LEFT | DT_VCENTER);

        let loc_audio_on = crate::recorder::is_local_audio_recording_enabled();
        let loc_labels = ["Capture This Machine (ON)", "Mute Local Audio"];
        draw_segmented_strip(panel_x, sec1_y + 22, panel_w, strip_h, &loc_labels, if loc_audio_on { 0 } else { 1 });

        // 3. Recording Quality Scale
        let sec2_y = sec1_y + step;
        let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
        SetTextColor(hdc, PK_TEXT);
        let mut s2_lbl: Vec<u16> = "Recording Resolution Scale".encode_utf16().collect();
        let mut s2_rc = RECT { left: panel_x, top: sec2_y, right: panel_x + panel_w, bottom: sec2_y + 20 };
        let _ = DrawTextW(hdc, &mut s2_lbl, &mut s2_rc, DT_SINGLELINE | DT_LEFT | DT_VCENTER);

        let scale_val = crate::recorder::get_recording_quality_scale() as usize;
        let scale_labels = ["Native (100%)", "Balanced (75%)", "Compact (50%)"];
        draw_segmented_strip(panel_x, sec2_y + 22, panel_w, strip_h, &scale_labels, scale_val.min(2));

        // 4. Storage Folder card with Open Folder button
        let sec3_y = sec2_y + step;
        let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
        SetTextColor(hdc, PK_TEXT);
        let mut s3_lbl: Vec<u16> = "Recordings Storage Folder".encode_utf16().collect();
        let mut s3_rc = RECT { left: panel_x, top: sec3_y, right: panel_x + panel_w, bottom: sec3_y + 20 };
        let _ = DrawTextW(hdc, &mut s3_lbl, &mut s3_rc, DT_SINGLELINE | DT_LEFT | DT_VCENTER);

        let card_h = 44;
        let folder_card_y = sec3_y + 22;
        draw_rounded_box(hdc, panel_x, folder_card_y, panel_w, card_h, PK_INPUT_BG, PK_CARD_BORDER, 6);

        let save_dir_str = crate::recorder::get_recording_save_dir().to_string_lossy().to_string();
        let btn_open_w = 110;
        let btn_open_x = panel_x + panel_w - btn_open_w - 6;
        let btn_open_y = folder_card_y + 5;
        let btn_open_h = 34;

        let _ = SelectObject(hdc, HGDIOBJ(font_lbl.0));
        SetTextColor(hdc, PK_MUTED);
        let mut dir_w: Vec<u16> = save_dir_str.encode_utf16().collect();
        let mut dir_rc = RECT { left: panel_x + 12, top: folder_card_y, right: btn_open_x - 8, bottom: folder_card_y + card_h };
        let _ = DrawTextW(hdc, &mut dir_w, &mut dir_rc, DT_SINGLELINE | DT_VCENTER);

        draw_rounded_box(hdc, btn_open_x, btn_open_y, btn_open_w, btn_open_h, PK_GREEN_TINT, PK_GREEN, 6);
        let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
        SetTextColor(hdc, PK_GREEN_DARK);
        let mut btn_open_txt: Vec<u16> = "Open Folder".encode_utf16().collect();
        let mut btn_open_rc = RECT { left: btn_open_x, top: btn_open_y, right: btn_open_x + btn_open_w, bottom: btn_open_y + btn_open_h };
        let _ = DrawTextW(hdc, &mut btn_open_txt, &mut btn_open_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

        // Bottom [ Done ] Button
        let done_y = folder_card_y + card_h + 20;
        let done_w = 140;
        let done_x = (client_w - done_w) / 2;
        let done_h = 36;
        draw_rounded_box(hdc, done_x, done_y, done_w, done_h, PK_GREEN, PK_GREEN_DARK, 6);
        let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
        SetTextColor(hdc, COLORREF(0x00FFFFFF));
        let mut done_txt: Vec<u16> = "Done".encode_utf16().collect();
        let mut done_rc = RECT { left: done_x, top: done_y, right: done_x + done_w, bottom: done_y + done_h };
        let _ = DrawTextW(hdc, &mut done_txt, &mut done_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);
    } else if active_cat == 3 {
        // Category 3: Unattended Security
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
        // Category 4: Connection Info
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
            ("Video Codec", crate::p2p::get_active_stream_codec_name().to_string()),
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
        let done_y = tbl_y + (rows.len() as i32) * (row_h + 4) + 24;
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

pub fn close_settings_modal(hwnd: HWND) {
    SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
    crate::windows::ui::launcher::sync_edit_target_visibility();
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

pub fn handle_settings_click(hwnd: HWND, client_w: i32, _client_h: i32, client_x: i32, client_y: i32, top_y: i32) -> bool {
    let panel_w = (client_w - 48).clamp(340, 640);
    let panel_x = (client_w - panel_w) / 2;

    let tab_y = top_y + 14;
    let tab_h = 32;
    let close_w = 76;
    let close_x = panel_x + panel_w - close_w;
    let cat_w = (panel_w - close_w - 30) / 5;
    let cat0_x = panel_x;
    let cat1_x = cat0_x + cat_w + 6;
    let cat2_x = cat1_x + cat_w + 6;
    let cat3_x = cat2_x + cat_w + 6;
    let cat4_x = cat3_x + cat_w + 6;

    // Close Button [ Close × ]
    if client_x >= close_x && client_x <= close_x + close_w && client_y >= tab_y && client_y <= tab_y + tab_h {
        close_settings_modal(hwnd);
        return true;
    }

    // Category 0 Tab: Display
    if client_x >= cat0_x && client_x <= cat0_x + cat_w && client_y >= tab_y && client_y <= tab_y + tab_h {
        SETTINGS_CATEGORY.store(0, Ordering::Relaxed);
        unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
        return true;
    }

    // Category 1 Tab: Codecs
    if client_x >= cat1_x && client_x <= cat1_x + cat_w && client_y >= tab_y && client_y <= tab_y + tab_h {
        SETTINGS_CATEGORY.store(1, Ordering::Relaxed);
        unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
        return true;
    }

    // Category 2 Tab: Recording
    if client_x >= cat2_x && client_x <= cat2_x + cat_w && client_y >= tab_y && client_y <= tab_y + tab_h {
        SETTINGS_CATEGORY.store(2, Ordering::Relaxed);
        unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
        return true;
    }

    // Category 3 Tab: Security
    if client_x >= cat3_x && client_x <= cat3_x + cat_w && client_y >= tab_y && client_y <= tab_y + tab_h {
        SETTINGS_CATEGORY.store(3, Ordering::Relaxed);
        unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
        return true;
    }

    // Category 4 Tab: Info
    if client_x >= cat4_x && client_x <= cat4_x + cat_w && client_y >= tab_y && client_y <= tab_y + tab_h {
        SETTINGS_CATEGORY.store(4, Ordering::Relaxed);
        unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
        return true;
    }

    let active_cat = SETTINGS_CATEGORY.load(Ordering::Relaxed).min(4);

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
                1 => 1,
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
            close_settings_modal(hwnd);
            return true;
        }
    } else if active_cat == 1 {
        // Category 1: Codec Settings
        let sec0_y = top_y + 60;
        let strip_h = 36;

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

        // 1. Codec Selection (4 options: Auto, H265, H264, MJPEG)
        if let Some(idx) = hit_segment(sec0_y + 22, 4) {
            crate::p2p::set_client_codec_pref(idx as u8);
            unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
            return true;
        }

        // 2. Test All Codecs button
        let card1_h = 60;
        let card2_h = 76;
        let sec1_y = sec0_y + 74;
        let sec2_y = sec1_y + card1_h + 14;
        let sec3_y = sec2_y + 22 + card2_h + 14;
        let test_btn_w = 180;
        let test_btn_h = 36;
        let test_btn_x = panel_x + (panel_w - test_btn_w) / 2;

        if client_x >= test_btn_x && client_x <= test_btn_x + test_btn_w
            && client_y >= sec3_y && client_y <= sec3_y + test_btn_h
        {
            crate::p2p::run_codec_diagnostics();
            unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
            return true;
        }

        // Done button
        let diag_str = crate::p2p::get_codec_diag_result();
        let diag_end_y = if !diag_str.is_empty() { sec3_y + test_btn_h + 8 + 34 } else { sec3_y + test_btn_h };
        let done_y = diag_end_y + 14;
        let done_w = 140;
        let done_x = (client_w - done_w) / 2;
        let done_h = 36;
        if client_x >= done_x && client_x <= done_x + done_w && client_y >= done_y && client_y <= done_y + done_h {
            close_settings_modal(hwnd);
            return true;
        }
    } else if active_cat == 2 {
        // Category 2: Recording Settings
        let sec0_y = top_y + 60;
        let strip_h = 36;
        let step = 70;

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

        // 1. Remote Media Audio toggle
        if let Some(idx) = hit_segment(sec0_y + 22, 2) {
            crate::recorder::set_remote_audio_recording_enabled(idx == 0);
            unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
            return true;
        }

        // 2. Local Media Audio toggle
        if let Some(idx) = hit_segment(sec0_y + step + 22, 2) {
            crate::recorder::set_local_audio_recording_enabled(idx == 0);
            unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
            return true;
        }

        // 3. Resolution scale
        if let Some(idx) = hit_segment(sec0_y + step * 2 + 22, 3) {
            crate::recorder::set_recording_quality_scale(idx as u8);
            unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
            return true;
        }

        // 4. Open Folder button
        let card_h = 44;
        let folder_card_y = sec0_y + step * 3 + 22;
        let btn_open_w = 110;
        let btn_open_x = panel_x + panel_w - btn_open_w - 6;
        let btn_open_y = folder_card_y + 5;
        let btn_open_h = 34;
        if client_x >= btn_open_x && client_x <= btn_open_x + btn_open_w && client_y >= btn_open_y && client_y <= btn_open_y + btn_open_h {
            let dir = crate::recorder::get_recording_save_dir();
            let _ = std::process::Command::new("explorer.exe").arg(&dir).spawn();
            return true;
        }

        // Done button
        let done_y = folder_card_y + card_h + 20;
        let done_w = 140;
        let done_x = (client_w - done_w) / 2;
        let done_h = 36;
        if client_x >= done_x && client_x <= done_x + done_w && client_y >= done_y && client_y <= done_y + done_h {
            close_settings_modal(hwnd);
            return true;
        }
    } else if active_cat == 3 {
        // Category 3: Unattended Security
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
            close_settings_modal(hwnd);
            return true;
        }
    } else {
        // Category 4: Info
        let row_h = 40;
        let tbl_y = top_y + 60;
        let done_y = tbl_y + 4 * (row_h + 4) + 24;
        let done_w = 140;
        let done_x = (client_w - done_w) / 2;
        let done_h = 36;
        if client_x >= done_x && client_x <= done_x + done_w && client_y >= done_y && client_y <= done_y + done_h {
            close_settings_modal(hwnd);
            return true;
        }
    }

    false
}
