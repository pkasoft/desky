//! Full-Screen Settings View & Diagnostics Page (Linux X11)
//!
//! Mirrors `src/windows/ui/viewport/settings_modal.rs` with a 2-pane responsive layout:
//! - Left Sidebar: 7 category navigation pills with Lucide vector icons
//! - Right Content Area: Grouped settings cards, interactive segment pills, and diagnostics

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU8, Ordering};

use crate::linux::ui::launcher::request_main_redraw;
use crate::linux::ui::theme::{
    format_bitrate, PixelCanvas, PK_BG, PK_BTN_HOVER, PK_CARD, PK_CARD_BORDER, PK_GREEN,
    PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_RED, PK_TEXT,
};

pub static SHOW_SETTINGS_MODAL: AtomicBool = AtomicBool::new(false);
pub static SETTINGS_CATEGORY: AtomicU8 = AtomicU8::new(0);
pub static SETTINGS_HOVER_ID: AtomicI32 = AtomicI32::new(-1);
pub static SETTINGS_SCROLL_Y: AtomicI32 = AtomicI32::new(0);
pub static SETTINGS_MAX_SCROLL: AtomicI32 = AtomicI32::new(0);

pub fn scroll_settings(delta: i32) {
    let step = 45;
    let cur = SETTINGS_SCROLL_Y.load(Ordering::Relaxed);
    let max_s = SETTINGS_MAX_SCROLL.load(Ordering::Relaxed);
    let new_scroll = if delta > 0 {
        (cur - step).max(0)
    } else {
        (cur + step).min(max_s)
    };
    SETTINGS_SCROLL_Y.store(new_scroll, Ordering::Relaxed);
}

pub fn reset_settings_scroll() {
    SETTINGS_SCROLL_Y.store(0, Ordering::Relaxed);
}

// Display mode: 0 = Auto Fit, 1 = Fill Window, 2 = 1:1 Original resolution
pub static DISPLAY_MODE: AtomicU8 = AtomicU8::new(0);
// Target FPS: 30, 60, 120
pub static TARGET_FPS: AtomicU32 = AtomicU32::new(60);
// Remote cursor capture toggle
pub static CAPTURE_CURSOR: AtomicBool = AtomicBool::new(true);

pub const CATEGORY_NAMES: [(&str, &str); 7] = [
    ("General", "App mode, Linux X11 desktop & machine identity"),
    ("Display & Stream", "Display scaling, framerate & stream quality"),
    ("Hardware Codecs", "GPU hardware acceleration & video codecs"),
    ("Audio & Voice", "Desktop audio streaming & voice intercom"),
    ("Security & Access", "Unattended access, E2EE encryption & privacy"),
    ("Session Recording", "Screen recording quality, audio & save directory"),
    ("About & Network", "Connection diagnostics, system specs & license"),
];

pub fn close_settings_modal() {
    SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
    request_main_redraw();
}

pub fn draw_fullscreen_settings_view(
    canvas: &mut PixelCanvas,
    client_w: i32,
    client_h: i32,
    top_y: i32,
    orig_w: u32,
    orig_h: u32,
) {
    canvas.fill_rect(0, top_y, client_w, client_h - top_y, PK_BG);

    let hover_id = SETTINGS_HOVER_ID.load(Ordering::Relaxed);
    let active_cat = SETTINGS_CATEGORY.load(Ordering::Relaxed).min(6);
    let mode = DISPLAY_MODE.load(Ordering::Relaxed);
    let target_fps = TARGET_FPS.load(Ordering::Relaxed);
    let cursor_on = CAPTURE_CURSOR.load(Ordering::Relaxed);
    let q_mode = crate::p2p::get_quality_mode();

    let pad_x = if client_w >= 900 { 24 } else { 14 };
    let pad_y = 14;
    let sidebar_w = 230;

    let sidebar_x = pad_x;
    let sidebar_y = top_y + pad_y;
    let sidebar_h = client_h - sidebar_y - pad_y;

    let content_x = sidebar_x + sidebar_w + 20;
    let content_w = (client_w - content_x - pad_x).max(360);
    let content_y = sidebar_y;

    // 1. Left Sidebar Navigation
    canvas.draw_rounded_box(sidebar_x, sidebar_y, sidebar_w, sidebar_h, PK_CARD, PK_CARD_BORDER, 8);
    canvas.draw_text(
        sidebar_x + 16,
        sidebar_y + 16,
        "DESKY SETTINGS",
        11.5,
        true,
        PK_MUTED,
        Some(sidebar_w - 32),
    );

    let item_h = 42;
    let item_start_y = sidebar_y + 44;
    for (i, (name, _)) in CATEGORY_NAMES.iter().enumerate() {
        let iy = item_start_y + (i as i32) * (item_h + 4);
        let ix = sidebar_x + 8;
        let iw = sidebar_w - 16;
        let is_active = (i as u8) == active_cat;
        let is_hover = hover_id == 100 + (i as i32);

        let (bg, border, fg) = if is_active {
            (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
        } else if is_hover {
            (PK_BTN_HOVER, PK_CARD_BORDER, PK_TEXT)
        } else {
            (PK_CARD, PK_CARD, PK_MUTED)
        };

        canvas.draw_rounded_box(ix, iy, iw, item_h, bg, border, 6);
        if is_active {
            canvas.fill_rect(ix + 2, iy + 8, 3, item_h - 16, PK_GREEN);
        }

        let icon_x = ix + 14;
        let icon_y = iy + (item_h - 16) / 2;
        match i {
            0 => canvas.draw_icon_settings(icon_x, icon_y, 16, fg),
            1 => canvas.draw_icon_monitor(icon_x, icon_y, 16, fg, None),
            2 => canvas.draw_icon_cpu(icon_x, icon_y, 16, fg),
            3 => canvas.draw_icon_volume(icon_x, icon_y, 16, fg, false),
            4 => canvas.draw_icon_lock(icon_x, icon_y, 16, fg),
            5 => canvas.draw_icon_record(icon_x, icon_y, 16, fg, false),
            _ => canvas.draw_icon_info(icon_x, icon_y, 16, fg),
        }

        canvas.draw_text_vcenter(ix + 38, iy, iw - 44, item_h, name, 13.5, is_active, fg);
    }

    canvas.draw_text(
        sidebar_x + 16,
        sidebar_y + sidebar_h - 44,
        &format!("Desky v{} (Linux X11)", env!("CARGO_PKG_VERSION")),
        12.0,
        false,
        PK_MUTED,
        Some(sidebar_w - 32),
    );
    canvas.draw_text(
        sidebar_x + 16,
        sidebar_y + sidebar_h - 26,
        "pkasoft.com",
        12.0,
        true,
        PK_GREEN_DARK,
        Some(sidebar_w - 32),
    );

    // 2. Right Content Area Header
    let header_y = content_y + 2;
    let (cat_title, cat_subtitle) = CATEGORY_NAMES[active_cat as usize];

    canvas.draw_text(
        content_x,
        header_y,
        cat_title,
        20.0,
        true,
        PK_TEXT,
        Some(content_w - 110),
    );
    canvas.draw_text(
        content_x,
        header_y + 26,
        cat_subtitle,
        13.0,
        false,
        PK_MUTED,
        Some(content_w - 110),
    );

    // [ × Close ] button in top-right
    let close_w = 96;
    let close_h = 34;
    let close_x = content_x + content_w - close_w;
    let close_y = header_y;
    let is_close_hover = hover_id == 9099;
    let (c_bg, c_bd, c_fg) = if is_close_hover {
        (PK_BTN_HOVER, PK_GREEN, PK_GREEN_DARK)
    } else {
        (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
    };
    canvas.draw_rounded_box(close_x, close_y, close_w, close_h, c_bg, c_bd, 6);
    canvas.draw_text_centered(close_x, close_y, close_w, close_h, "× Close", 13.5, true, c_fg);

    let content_top = header_y + 56;
    let content_bot = client_h - pad_y;
    let avail_h = (content_bot - content_top).max(50);

    let cat_content_h = match active_cat {
        0 => 430,
        1 => 650,
        2 => 360,
        3 => 320,
        4 => 320,
        5 => 460,
        _ => 540,
    };

    let max_scroll = (cat_content_h - avail_h).max(0);
    SETTINGS_MAX_SCROLL.store(max_scroll, Ordering::Relaxed);
    let scroll_y = SETTINGS_SCROLL_Y.load(Ordering::Relaxed).min(max_scroll);
    SETTINGS_SCROLL_Y.store(scroll_y, Ordering::Relaxed);

    let body_y = content_top - scroll_y;

    canvas.set_clip_rect(Some((content_x - 4, content_top, content_x + content_w + 10, content_bot)));

    match active_cat {
        0 => {
            // General Category
            canvas.draw_rounded_box(content_x, body_y, content_w, 130, PK_CARD, PK_CARD_BORDER, 8);
            canvas.draw_text(
                content_x + 18,
                body_y + 16,
                "Linux X11 Native Runtime",
                15.0,
                true,
                PK_TEXT,
                Some(content_w - 160),
            );
            canvas.draw_rounded_box(
                content_x + content_w - 148,
                body_y + 14,
                130,
                24,
                PK_GREEN_TINT,
                PK_GREEN,
                6,
            );
            canvas.draw_text_centered(
                content_x + content_w - 148,
                body_y + 14,
                130,
                24,
                "PURE RUST X11",
                11.0,
                true,
                PK_GREEN_DARK,
            );
            canvas.draw_text_wrapped(
                content_x + 18,
                body_y + 46,
                content_w - 36,
                64,
                "Desky runs as a single self-contained native Linux binary using X11 protocol and XTest input injection with zero external daemon dependencies.",
                13.0,
                false,
                PK_MUTED,
            );

            let id_y = body_y + 144;
            canvas.draw_rounded_box(content_x, id_y, content_w, 120, PK_CARD, PK_CARD_BORDER, 8);
            canvas.draw_text(
                content_x + 18,
                id_y + 16,
                "Machine Identity & Rendezvous Server",
                15.0,
                true,
                PK_TEXT,
                Some(content_w - 36),
            );
            let ident = crate::identity::load_or_create_identity();
            let my_id = ident.desky_id.unwrap_or_else(|| "Connecting...".into());
            canvas.draw_text(
                content_x + 18,
                id_y + 46,
                &format!("Permanent Desky ID:  {}", my_id),
                13.5,
                true,
                PK_GREEN_DARK,
                Some(content_w - 36),
            );
            canvas.draw_text(
                content_x + 18,
                id_y + 72,
                &format!("Rendezvous Server:   {}", crate::config::DEFAULT_VDS_SERVER),
                13.0,
                false,
                PK_MUTED,
                Some(content_w - 36),
            );

            // 3. System Tray & Startup Behavior Card
            let tray_y = id_y + 134;
            canvas.draw_rounded_box(content_x, tray_y, content_w, 136, PK_CARD, PK_CARD_BORDER, 8);
            canvas.draw_text(
                content_x + 18,
                tray_y + 16,
                "System Tray & Startup Behavior",
                15.0,
                true,
                PK_TEXT,
                Some(content_w - 36),
            );

            let autostart_on = crate::linux::system::is_autostart_enabled();
            let prefs = crate::identity::load_preferences();
            let t_btn_w = 112;
            let t_btn_h = 30;
            let t_btn_x = content_x + content_w - t_btn_w - 18;
            let tray_rows = [
                ("Start automatically on boot (Background Tray)", autostart_on),
                ("Close main window to System Tray (Keep P2P online)", prefs.close_to_tray),
            ];
            for (idx, (label, enabled)) in tray_rows.iter().enumerate() {
                let ry = tray_y + 52 + (idx as i32) * 40;
                canvas.draw_text(
                    content_x + 18,
                    ry + 7,
                    label,
                    13.0,
                    false,
                    PK_TEXT,
                    Some(t_btn_x - content_x - 28),
                );
                let (bg, bd, fg) = if *enabled {
                    (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
                } else {
                    (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
                };
                canvas.draw_rounded_box(t_btn_x, ry, t_btn_w, t_btn_h, bg, bd, 6);
                canvas.draw_text_centered(
                    t_btn_x,
                    ry,
                    t_btn_w,
                    t_btn_h,
                    if *enabled { "✓ Enabled" } else { "Disabled" },
                    12.5,
                    *enabled,
                    fg,
                );
            }
        }
        1 => {
            // Display & Stream Category
            // 1. Scaling Mode (Auto Fit / Stretch / 1:1)
            canvas.draw_rounded_box(content_x, body_y, content_w, 108, PK_CARD, PK_CARD_BORDER, 8);
            canvas.draw_text(
                content_x + 18,
                body_y + 14,
                "Viewport Display Scaling",
                15.0,
                true,
                PK_TEXT,
                Some(content_w - 36),
            );
            let modes = [(0u8, "Auto Fit (Aspect)"), (1, "Stretch to Fill"), (2, "1:1 Original")];
            let btn_w = (content_w - 36 - 16) / 3;
            for (idx, (m_val, label)) in modes.iter().enumerate() {
                let bx = content_x + 18 + (idx as i32) * (btn_w + 8);
                let by = body_y + 48;
                let active = mode == *m_val;
                let (bg, bd, fg) = if active {
                    (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
                } else {
                    (PK_INPUT_BG, PK_CARD_BORDER, PK_TEXT)
                };
                canvas.draw_rounded_box(bx, by, btn_w, 40, bg, bd, 6);
                canvas.draw_text_centered(bx, by, btn_w, 40, label, 13.0, active, fg);
            }

            // 2. Target Framerate (30 / 60 FPS) & Remote Cursor Toggle
            let row2_y = body_y + 122;
            canvas.draw_rounded_box(content_x, row2_y, content_w, 108, PK_CARD, PK_CARD_BORDER, 8);
            canvas.draw_text(
                content_x + 18,
                row2_y + 14,
                "Target Framerate & Cursor Capture",
                15.0,
                true,
                PK_TEXT,
                Some(content_w - 36),
            );
            let fps_opts = [(30u32, "30 FPS (Standard)"), (60, "60 FPS (Smooth)")];
            let f_w = (content_w - 36 - 16) / 3;
            for (idx, (f_val, label)) in fps_opts.iter().enumerate() {
                let bx = content_x + 18 + (idx as i32) * (f_w + 8);
                let by = row2_y + 48;
                let active = target_fps == *f_val;
                let (bg, bd, fg) = if active {
                    (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
                } else {
                    (PK_INPUT_BG, PK_CARD_BORDER, PK_TEXT)
                };
                canvas.draw_rounded_box(bx, by, f_w, 40, bg, bd, 6);
                canvas.draw_text_centered(bx, by, f_w, 40, label, 13.0, active, fg);
            }
            // Cursor toggle button in 3rd slot
            let cur_x = content_x + 18 + 2 * (f_w + 8);
            let cur_y = row2_y + 48;
            let (c_bg, c_bd, c_fg) = if cursor_on {
                (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
            } else {
                (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
            };
            canvas.draw_rounded_box(cur_x, cur_y, f_w, 40, c_bg, c_bd, 6);
            canvas.draw_text_centered(
                cur_x,
                cur_y,
                f_w,
                40,
                if cursor_on { "Cursor: ON" } else { "Cursor: OFF" },
                13.0,
                true,
                c_fg,
            );

            // 3. Stream Quality Preset (Auto / Ultra / High / Balanced / Low)
            let row3_y = row2_y + 122;
            canvas.draw_rounded_box(content_x, row3_y, content_w, 108, PK_CARD, PK_CARD_BORDER, 8);
            canvas.draw_text(
                content_x + 18,
                row3_y + 14,
                "Image Quality & Bandwidth Mode",
                15.0,
                true,
                PK_TEXT,
                Some(content_w - 36),
            );
            let q_opts = [
                (0u8, "Auto ABR"),
                (1, "Ultra 4:4:4"),
                (2, "High"),
                (3, "Balanced"),
                (4, "Low Data"),
            ];
            let qw = (content_w - 36 - 32) / 5;
            for (idx, (qv, label)) in q_opts.iter().enumerate() {
                let bx = content_x + 18 + (idx as i32) * (qw + 8);
                let by = row3_y + 48;
                let active = q_mode == *qv;
                let (bg, bd, fg) = if active {
                    (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
                } else {
                    (PK_INPUT_BG, PK_CARD_BORDER, PK_TEXT)
                };
                canvas.draw_rounded_box(bx, by, qw, 40, bg, bd, 6);
                canvas.draw_text_centered(bx, by, qw, 40, label, 12.5, active, fg);
            }

            // 4. Stream Buffer & Memory Cache
            let row4_y = row3_y + 122;
            canvas.draw_rounded_box(content_x, row4_y, content_w, 108, PK_CARD, PK_CARD_BORDER, 8);
            canvas.draw_text(
                content_x + 18,
                row4_y + 14,
                "Stream Buffer & Memory Cache",
                15.0,
                true,
                PK_TEXT,
                Some(content_w - 36),
            );
            let buf_tier = crate::p2p::get_client_buffer_tier();
            let buf_opts = ["128 MB (Light)", "256 MB (Standard)", "512 MB (High)", "1024 MB (Max)"];
            let bw = (content_w - 36 - 24) / 4;
            for (idx, label) in buf_opts.iter().enumerate() {
                let bx = content_x + 18 + (idx as i32) * (bw + 8);
                let by = row4_y + 48;
                let active = buf_tier == idx;
                let (bg, bd, fg) = if active {
                    (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
                } else {
                    (PK_INPUT_BG, PK_CARD_BORDER, PK_TEXT)
                };
                canvas.draw_rounded_box(bx, by, bw, 40, bg, bd, 6);
                canvas.draw_text_centered(bx, by, bw, 40, label, 12.5, active, fg);
            }
        }
        2 => {
            // Hardware Codecs Category
            canvas.draw_rounded_box(content_x, body_y, content_w, 162, PK_CARD, PK_CARD_BORDER, 8);
            canvas.draw_text(
                content_x + 18,
                body_y + 16,
                "Video Codec & GPU Hardware Acceleration",
                15.0,
                true,
                PK_TEXT,
                Some(content_w - 36),
            );
            let h264_avail = crate::linux::codec::is_h264_hardware_available();
            let h265_avail = crate::linux::codec::is_h265_hardware_available();
            let vaapi_status = if h264_avail {
                "VA-API H.264 Hardware Acceleration: READY (/dev/dri/renderD128)"
            } else {
                "VA-API H.264 Hardware Acceleration: Unavailable (Fallback: SIMD 4:4:4)"
            };
            canvas.draw_text(
                content_x + 18,
                body_y + 40,
                vaapi_status,
                13.0,
                true,
                if h264_avail { PK_GREEN_DARK } else { PK_MUTED },
                Some(content_w - 36),
            );
            canvas.draw_text(
                content_x + 18,
                body_y + 60,
                &format!(
                    "Active Stream Codec: {}",
                    crate::p2p::get_active_stream_codec_name()
                ),
                13.0,
                false,
                PK_TEXT,
                Some(content_w - 36),
            );

            let pref = crate::p2p::get_client_codec_pref();

            let c_opts = [
                (0u8, "Auto (DeskyTile)", true),
                (1, "H.265 GPU (Unsupported)", h265_avail),
                (2, "H.264 VA-API", h264_avail),
                (3, "Motion-JPEG 4:4:4", true),
            ];
            let cw = (content_w - 36 - 24) / 4;
            for (idx, (cv, label, supported)) in c_opts.iter().enumerate() {
                let bx = content_x + 18 + (idx as i32) * (cw + 8);
                let by = body_y + 94;
                let active = pref == *cv;
                let (bg, bd, fg) = if !*supported {
                    (PK_BG, PK_CARD_BORDER, PK_MUTED)
                } else if active {
                    (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
                } else {
                    (PK_INPUT_BG, PK_CARD_BORDER, PK_TEXT)
                };
                canvas.draw_rounded_box(bx, by, cw, 42, bg, bd, 6);
                canvas.draw_text_centered(bx, by, cw, 42, label, 12.0, active, fg);
            }
        }
        3 => {
            // Audio & Voice Category
            canvas.draw_rounded_box(content_x, body_y, content_w, 120, PK_CARD, PK_CARD_BORDER, 8);
            canvas.draw_text(
                content_x + 18,
                body_y + 16,
                "Audio Loopback & Voice Intercom",
                15.0,
                true,
                PK_TEXT,
                Some(content_w - 36),
            );
            canvas.draw_text_wrapped(
                content_x + 18,
                body_y + 48,
                content_w - 36,
                54,
                "Opus-compressed 48 kHz desktop audio and E2EE 2-way voice intercom are active via PulseAudio / PipeWire (parec, pacat, pw-record, pw-play).",
                13.0,
                false,
                PK_MUTED,
            );
        }
        4 => {
            // Security & Access Category
            canvas.draw_rounded_box(content_x, body_y, content_w, 140, PK_CARD, PK_CARD_BORDER, 8);
            canvas.draw_text(
                content_x + 18,
                body_y + 16,
                "Unattended Access & Session Security",
                15.0,
                true,
                PK_TEXT,
                Some(content_w - 36),
            );
            let enabled = crate::identity::is_unattended_enabled();
            canvas.draw_text(
                content_x + 18,
                body_y + 46,
                if enabled {
                    "Status: Unattended Password Protection ENABLED"
                } else {
                    "Status: Interactive Manual Approval Required"
                },
                13.5,
                true,
                if enabled { PK_GREEN_DARK } else { PK_TEXT },
                Some(content_w - 36),
            );
            canvas.draw_text_wrapped(
                content_x + 18,
                body_y + 76,
                content_w - 36,
                48,
                "All connections use salted SHA-256 challenge-response authentication and direct peer-to-peer UDP transport.",
                13.0,
                false,
                PK_MUTED,
            );
        }
        5 => {
            // Session Recording Category
            canvas.draw_rounded_box(content_x, body_y, content_w, 130, PK_CARD, PK_CARD_BORDER, 8);
            canvas.draw_text(
                content_x + 18,
                body_y + 16,
                "Session Screen Recording",
                15.0,
                true,
                PK_TEXT,
                Some(content_w - 36),
            );
            let is_rec = crate::recorder::is_recording();
            canvas.draw_text(
                content_x + 18,
                body_y + 46,
                if is_rec {
                    "Status: 🔴 Recording in progress..."
                } else {
                    "Status: Idle (Click Record button in top toolbar during a session)"
                },
                13.5,
                true,
                if is_rec { PK_RED } else { PK_MUTED },
                Some(content_w - 36),
            );
            let dir = crate::linux::get_desktop_dir();
            canvas.draw_text(
                content_x + 18,
                body_y + 78,
                &format!("Save Directory: {}", dir.display()),
                13.0,
                false,
                PK_TEXT,
                Some(content_w - 36),
            );
        }
        _ => {
            // About & Network Diagnostics Category
            canvas.draw_rounded_box(content_x, body_y, content_w, 175, PK_CARD, PK_CARD_BORDER, 8);
            canvas.draw_text(
                content_x + 18,
                body_y + 16,
                "Live Network Telemetry & Session Diagnostics",
                15.0,
                true,
                PK_TEXT,
                Some(content_w - 36),
            );
            let rtt = crate::p2p::get_session_rtt_ms();
            let kbps = crate::p2p::get_session_bitrate_kbps();
            let srv_ping = crate::p2p::get_server_ping_ms();

            canvas.draw_text(
                content_x + 18,
                body_y + 48,
                &format!("Rendezvous Server Ping:   {} ms", srv_ping),
                13.5,
                false,
                PK_TEXT,
                Some(content_w - 36),
            );
            canvas.draw_text(
                content_x + 18,
                body_y + 74,
                &format!("Peer-to-Peer Round-Trip:  {} ms", rtt),
                13.5,
                false,
                PK_TEXT,
                Some(content_w - 36),
            );
            canvas.draw_text(
                content_x + 18,
                body_y + 100,
                &format!("Stream Bitrate:           {}", format_bitrate(kbps)),
                13.5,
                false,
                PK_TEXT,
                Some(content_w - 36),
            );
            canvas.draw_text(
                content_x + 18,
                body_y + 126,
                &format!("Remote Desktop Resolution: {}x{}", orig_w, orig_h),
                13.5,
                false,
                PK_TEXT,
                Some(content_w - 36),
            );

            // 2. Network Tunneling & Routing Mode Card
            let r_y = body_y + 190;
            canvas.draw_rounded_box(content_x, r_y, content_w, 108, PK_CARD, PK_CARD_BORDER, 8);
            canvas.draw_text(
                content_x + 18,
                r_y + 14,
                "Network Tunneling & Routing Mode",
                15.0,
                true,
                PK_TEXT,
                Some(content_w - 36),
            );
            let r_mode = crate::p2p::get_routing_mode();
            let r_opts = ["Smart P2P (Auto)", "Direct Server Tunnel", "Serverless (LAN / IP)"];
            let rw = (content_w - 36 - 16) / 3;
            for (idx, label) in r_opts.iter().enumerate() {
                let bx = content_x + 18 + (idx as i32) * (rw + 8);
                let by = r_y + 48;
                let active = (r_mode as usize) == idx;
                let (bg, bd, fg) = if active {
                    (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
                } else {
                    (PK_INPUT_BG, PK_CARD_BORDER, PK_TEXT)
                };
                canvas.draw_rounded_box(bx, by, rw, 40, bg, bd, 6);
                canvas.draw_text_centered(bx, by, rw, 40, label, 12.5, active, fg);
            }
        }
    }

    canvas.set_clip_rect(None);

    if max_scroll > 0 {
        let sb_w = 4;
        let sb_x = content_x + content_w - sb_w;
        let sb_y = content_top;
        let sb_h = avail_h;
        canvas.draw_rounded_box(sb_x, sb_y, sb_w, sb_h, PK_INPUT_BG, PK_CARD_BORDER, 2);
        let thumb_h = ((avail_h as f32 / cat_content_h as f32) * sb_h as f32).clamp(24.0, sb_h as f32) as i32;
        let thumb_travel = sb_h - thumb_h;
        let thumb_y = sb_y + ((scroll_y as f32 / max_scroll as f32) * thumb_travel as f32) as i32;
        canvas.draw_rounded_box(sb_x, thumb_y, sb_w, thumb_h, PK_MUTED, PK_MUTED, 2);
    }
}

pub fn handle_settings_click(client_x: i32, client_y: i32, client_w: i32, client_h: i32, top_y: i32) -> bool {
    if !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
        return false;
    }

    let pad_x = if client_w >= 900 { 24 } else { 14 };
    let pad_y = 14;
    let sidebar_w = 230;
    let sidebar_x = pad_x;
    let sidebar_y = top_y + pad_y;
    let content_x = sidebar_x + sidebar_w + 20;
    let content_w = (client_w - content_x - pad_x).max(360);
    let header_y = sidebar_y + 2;

    // 1. Check Close Button
    let close_w = 96;
    let close_h = 34;
    let close_x = content_x + content_w - close_w;
    let close_y = header_y;
    if client_x >= close_x
        && client_x <= close_x + close_w
        && client_y >= close_y
        && client_y <= close_y + close_h
    {
        close_settings_modal();
        return true;
    }

    // 2. Check Left Sidebar Categories
    let item_h = 42;
    let item_start_y = sidebar_y + 44;
    for i in 0..CATEGORY_NAMES.len() {
        let iy = item_start_y + (i as i32) * (item_h + 4);
        let ix = sidebar_x + 8;
        let iw = sidebar_w - 16;
        if client_x >= ix && client_x <= ix + iw && client_y >= iy && client_y <= iy + item_h {
            reset_settings_scroll();
            SETTINGS_CATEGORY.store(i as u8, Ordering::Relaxed);
            request_main_redraw();
            return true;
        }
    }

    // 3. Content card geometry & vertical scroll offset
    let content_top = header_y + 56;
    let content_bot = client_h - pad_y;
    let avail_h = (content_bot - content_top).max(50);
    let active_cat = SETTINGS_CATEGORY.load(Ordering::Relaxed).min(6);
    let cat_content_h = match active_cat {
        0 => 430,
        1 => 650,
        2 => 360,
        3 => 320,
        4 => 320,
        5 => 460,
        _ => 540,
    };
    let max_scroll = (cat_content_h - avail_h).max(0);
    let scroll_y = SETTINGS_SCROLL_Y.load(Ordering::Relaxed).min(max_scroll);
    let body_y = content_top - scroll_y;

    if client_y < content_top || client_y > content_bot {
        return false;
    }
    if active_cat == 0 {
        let tray_y = body_y + 144 + 134;
        let t_btn_w = 112;
        let t_btn_h = 30;
        let t_btn_x = content_x + content_w - t_btn_w - 18;
        if client_x >= t_btn_x && client_x <= t_btn_x + t_btn_w {
            let ry0 = tray_y + 52;
            let ry1 = tray_y + 92;
            if client_y >= ry0 && client_y <= ry0 + t_btn_h {
                let next = !crate::linux::system::is_autostart_enabled();
                if crate::linux::system::set_autostart_enabled(next).is_ok() {
                    let mut prefs = crate::identity::load_preferences();
                    prefs.start_on_boot = next;
                    crate::identity::save_preferences(&prefs);
                }
                request_main_redraw();
                return true;
            }
            if client_y >= ry1 && client_y <= ry1 + t_btn_h {
                let mut prefs = crate::identity::load_preferences();
                prefs.close_to_tray = !prefs.close_to_tray;
                crate::identity::save_preferences(&prefs);
                request_main_redraw();
                return true;
            }
        }
    } else if active_cat == 1 {
        // Scaling modes
        let btn_w = (content_w - 36 - 16) / 3;
        let by1 = body_y + 48;
        if client_y >= by1 && client_y <= by1 + 40 {
            for idx in 0..3 {
                let bx = content_x + 18 + (idx as i32) * (btn_w + 8);
                if client_x >= bx && client_x <= bx + btn_w {
                    DISPLAY_MODE.store(idx as u8, Ordering::Relaxed);
                    request_main_redraw();
                    return true;
                }
            }
        }

        // FPS & Cursor
        let row2_y = body_y + 122;
        let by2 = row2_y + 48;
        if client_y >= by2 && client_y <= by2 + 40 {
            let bx0 = content_x + 18;
            let bx1 = content_x + 18 + (btn_w + 8);
            let bx2 = content_x + 18 + 2 * (btn_w + 8);
            if client_x >= bx0 && client_x <= bx0 + btn_w {
                TARGET_FPS.store(30, Ordering::Relaxed);
                crate::p2p::update_client_preferences(30, CAPTURE_CURSOR.load(Ordering::Relaxed));
                request_main_redraw();
                return true;
            } else if client_x >= bx1 && client_x <= bx1 + btn_w {
                TARGET_FPS.store(60, Ordering::Relaxed);
                crate::p2p::update_client_preferences(60, CAPTURE_CURSOR.load(Ordering::Relaxed));
                request_main_redraw();
                return true;
            } else if client_x >= bx2 && client_x <= bx2 + btn_w {
                let next = !CAPTURE_CURSOR.load(Ordering::Relaxed);
                CAPTURE_CURSOR.store(next, Ordering::Relaxed);
                crate::p2p::update_client_preferences(TARGET_FPS.load(Ordering::Relaxed), next);
                request_main_redraw();
                return true;
            }
        }

        // Quality Mode
        let row3_y = row2_y + 122;
        let by3 = row3_y + 48;
        let qw = (content_w - 36 - 32) / 5;
        if client_y >= by3 && client_y <= by3 + 40 {
            for idx in 0..5 {
                let bx = content_x + 18 + (idx as i32) * (qw + 8);
                if client_x >= bx && client_x <= bx + qw {
                    crate::p2p::set_quality_mode(idx as u8);
                    request_main_redraw();
                    return true;
                }
            }
        }

        // Stream Buffer & Memory Cache
        let row4_y = row3_y + 122;
        let by4 = row4_y + 48;
        let bw = (content_w - 36 - 24) / 4;
        if client_y >= by4 && client_y <= by4 + 40 {
            for idx in 0..4 {
                let bx = content_x + 18 + (idx as i32) * (bw + 8);
                if client_x >= bx && client_x <= bx + bw {
                    crate::p2p::set_client_buffer_tier(idx);
                    request_main_redraw();
                    return true;
                }
            }
        }
    } else if active_cat == 2 {
        // Codec selection
        let cw = (content_w - 36 - 24) / 4;
        let by = body_y + 94;
        if client_y >= by - 12 && client_y <= by + 48 {
            for idx in 0..4 {
                let bx = content_x + 18 + (idx as i32) * (cw + 8);
                if client_x >= bx && client_x <= bx + cw {
                    crate::p2p::set_client_codec_pref(idx as u8);
                    request_main_redraw();
                    return true;
                }
            }
        }
    } else if active_cat == 6 {
        let r_y = body_y + 190;
        let by = r_y + 48;
        let rw = (content_w - 36 - 16) / 3;
        if client_y >= by && client_y <= by + 40 {
            for idx in 0..3 {
                let bx = content_x + 18 + (idx as i32) * (rw + 8);
                if client_x >= bx && client_x <= bx + rw {
                    crate::p2p::set_routing_mode(idx as u8);
                    request_main_redraw();
                    return true;
                }
            }
        }
    }

    true
}
