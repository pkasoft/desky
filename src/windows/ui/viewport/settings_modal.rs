//! Full-Screen Settings View & Diagnostics Page
//!
//! Two-pane responsive architecture:
//! - Left Sidebar: Category navigation with native Lucide vector icons
//! - Right Content Area: Modern grouped cards, interactive segment pills,
//!   and native Windows desktop shortcut & installation management.

#![allow(unsafe_op_in_unsafe_fn)]

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU8, Ordering};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, DeleteObject, DrawTextW, FillRect, InvalidateRect, SelectObject, SetBkMode,
    SetTextColor, DT_CENTER, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER,
    DT_WORDBREAK, FW_BOLD, FW_NORMAL, FW_SEMIBOLD, HDC, HGDIOBJ, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    LoadCursorW, MessageBoxW, SetCursor, IDC_ARROW, IDC_HAND, MB_ICONERROR, MB_ICONINFORMATION,
    MB_OK,
};

use super::theme::{
    create_mono_font, create_segoe_font, draw_rounded_box, format_bitrate, PK_BG, PK_BTN_HOVER,
    PK_CARD, PK_CARD_BORDER, PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED,
    PK_RED, PK_TEXT,
};

// Full-Screen Settings Page state
pub static SHOW_SETTINGS_MODAL: AtomicBool = AtomicBool::new(false);

// Active settings sidebar category:
// 0 = General (App info, Desktop Shortcut / Install, Identity)
// 1 = Display & Stream (Scaling, FPS, Quality Presets, Cursor)
// 2 = Hardware Codecs (GPU Acceleration, Codec Prefs, 4:4:4)
// 3 = Audio & Voice (Desktop Audio, Voice Intercom)
// 4 = Security & Access (Unattended Password, E2EE, Screen Privacy)
// 5 = Session Recording (Channels, Quality Scale, Save Folder)
// 6 = About & Diagnostics (Latency Ping, Bitrate, System Specs, AGPL License)
pub static SETTINGS_CATEGORY: AtomicU8 = AtomicU8::new(0);

// Hover element tracker for interactive styling
pub static SETTINGS_HOVER_ID: AtomicI32 = AtomicI32::new(-1);

// Display mode: 0 = Auto Fit, 1 = Fill Window, 2 = 1:1 Original resolution
pub static DISPLAY_MODE: AtomicU8 = AtomicU8::new(0);

// Target FPS: 30, 60, 120
pub static TARGET_FPS: AtomicU32 = AtomicU32::new(60);

// Remote cursor capture toggle
pub static CAPTURE_CURSOR: AtomicBool = AtomicBool::new(true);

pub const CATEGORY_NAMES: [(&str, &str); 7] = [
    ("General", "App mode, desktop shortcuts & machine identity"),
    ("Display & Stream", "Display scaling, framerate & stream quality"),
    ("Hardware Codecs", "GPU hardware acceleration & video codecs"),
    ("Audio & Voice", "Desktop audio streaming & voice intercom"),
    ("Security & Access", "Unattended access, E2EE encryption & privacy"),
    ("Session Recording", "Screen recording quality, audio & save directory"),
    ("About & Network", "Connection diagnostics, system specs & license"),
];

pub unsafe fn draw_fullscreen_settings_view(
    hdc: HDC,
    client_w: i32,
    client_h: i32,
    mode: u8,
    _fps: u32,
    top_y: i32,
    orig_w: u32,
    orig_h: u32,
) {
    // 1. Full-Screen Canvas Fill
    let full_rect = RECT {
        left: 0,
        top: top_y,
        right: client_w,
        bottom: client_h,
    };
    let bg_brush = CreateSolidBrush(PK_BG);
    FillRect(hdc, &full_rect, bg_brush);
    let _ = DeleteObject(HGDIOBJ(bg_brush.0));

    SetBkMode(hdc, TRANSPARENT);

    let hover_id = SETTINGS_HOVER_ID.load(Ordering::Relaxed);
    let active_cat = SETTINGS_CATEGORY.load(Ordering::Relaxed).min(6);

    // Responsive geometry
    let pad_x = if client_w >= 900 { 24 } else { 14 };
    let pad_y = 14;
    let sidebar_w = 230;

    let sidebar_x = pad_x;
    let sidebar_y = top_y + pad_y;
    let sidebar_h = client_h - sidebar_y - pad_y;

    let content_x = sidebar_x + sidebar_w + 20;
    let content_w = (client_w - content_x - pad_x).max(360);
    let content_y = sidebar_y;

    // Fonts
    let font_header = create_segoe_font(20, FW_BOLD);
    let font_title = create_segoe_font(15, FW_BOLD);
    let font_tab = create_segoe_font(13, FW_SEMIBOLD);
    let font_sub = create_segoe_font(13, FW_NORMAL);
    let font_btn = create_segoe_font(13, FW_SEMIBOLD);
    let font_mono = create_mono_font(12, FW_NORMAL);
    let font_badge = create_segoe_font(11, FW_BOLD);

    // ==========================================
    // 2. LEFT SIDEBAR NAVIGATION
    // ==========================================
    draw_rounded_box(hdc, sidebar_x, sidebar_y, sidebar_w, sidebar_h, PK_CARD, PK_CARD_BORDER, 10);

    // Sidebar Section Header: "DESKY SETTINGS"
    let _ = SelectObject(hdc, HGDIOBJ(font_badge.0));
    SetTextColor(hdc, PK_MUTED);
    let mut sb_head_txt: Vec<u16> = "DESKY SETTINGS".encode_utf16().collect();
    let mut sb_head_rc = RECT {
        left: sidebar_x + 16,
        top: sidebar_y + 16,
        right: sidebar_x + sidebar_w - 16,
        bottom: sidebar_y + 34,
    };
    let _ = DrawTextW(
        hdc,
        &mut sb_head_txt,
        &mut sb_head_rc,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    // Sidebar Category Buttons
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

        draw_rounded_box(hdc, ix, iy, iw, item_h, bg, border, 8);

        // Active left accent indicator bar
        if is_active {
            let acc_brush = CreateSolidBrush(PK_GREEN);
            let acc_rc = RECT {
                left: ix + 2,
                top: iy + 8,
                right: ix + 5,
                bottom: iy + item_h - 8,
            };
            FillRect(hdc, &acc_rc, acc_brush);
            let _ = DeleteObject(HGDIOBJ(acc_brush.0));
        }

        // Vector Icon for category
        let icon_x = ix + 14;
        let icon_y = iy + (item_h - 16) / 2;
        match i {
            0 => crate::ui::draw_settings(hdc, icon_x, icon_y, 16, fg, 2),
            1 => crate::ui::draw_monitor(hdc, icon_x, icon_y, 16, fg, 2),
            2 => crate::ui::draw_cpu(hdc, icon_x, icon_y, 16, fg, 2),
            3 => crate::ui::draw_volume(hdc, icon_x, icon_y, 16, fg, 2),
            4 => crate::ui::draw_lock(hdc, icon_x, icon_y, 16, fg, 2),
            5 => crate::ui::draw_record(hdc, icon_x, icon_y, 16, fg, 2, false),
            _ => crate::ui::draw_info(hdc, icon_x, icon_y, 16, fg, 2),
        }

        // Category Text Label
        let _ = SelectObject(hdc, HGDIOBJ(font_tab.0));
        SetTextColor(hdc, fg);
        let mut name_txt: Vec<u16> = name.encode_utf16().collect();
        let mut name_rc = RECT {
            left: ix + 40,
            top: iy,
            right: ix + iw - 8,
            bottom: iy + item_h,
        };
        let _ = DrawTextW(
            hdc,
            &mut name_txt,
            &mut name_rc,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
    }

    // Sidebar bottom metadata badge
    let _ = SelectObject(hdc, HGDIOBJ(font_sub.0));
    SetTextColor(hdc, PK_MUTED);
    let mut ver_txt: Vec<u16> = format!("Desky v{}\npkasoft.com", env!("CARGO_PKG_VERSION"))
        .encode_utf16()
        .collect();
    let mut ver_rc = RECT {
        left: sidebar_x + 16,
        top: sidebar_y + sidebar_h - 48,
        right: sidebar_x + sidebar_w - 16,
        bottom: sidebar_y + sidebar_h - 10,
    };
    let _ = DrawTextW(hdc, &mut ver_txt, &mut ver_rc, DT_LEFT | DT_NOPREFIX);

    // ==========================================
    // 3. RIGHT CONTENT AREA
    // ==========================================
    let header_y = content_y + 2;
    let (cat_title, cat_subtitle) = CATEGORY_NAMES[active_cat as usize];

    // Category Title
    let _ = SelectObject(hdc, HGDIOBJ(font_header.0));
    SetTextColor(hdc, PK_TEXT);
    let mut title_txt: Vec<u16> = cat_title.encode_utf16().collect();
    let mut title_rc = RECT {
        left: content_x,
        top: header_y,
        right: content_x + content_w - 110,
        bottom: header_y + 28,
    };
    let _ = DrawTextW(
        hdc,
        &mut title_txt,
        &mut title_rc,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    // Category Subtitle
    let _ = SelectObject(hdc, HGDIOBJ(font_sub.0));
    SetTextColor(hdc, PK_MUTED);
    let mut sub_txt: Vec<u16> = cat_subtitle.encode_utf16().collect();
    let mut sub_rc = RECT {
        left: content_x,
        top: header_y + 28,
        right: content_x + content_w - 110,
        bottom: header_y + 48,
    };
    let _ = DrawTextW(
        hdc,
        &mut sub_txt,
        &mut sub_rc,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    // Clean [ × Close ] button in top-right
    let close_w = 96;
    let close_h = 34;
    let close_x = content_x + content_w - close_w;
    let close_y = header_y;
    let is_close_hover = hover_id == 9099;
    let (c_bg, c_border, c_fg) = if is_close_hover {
        (PK_BTN_HOVER, PK_CARD_BORDER, PK_TEXT)
    } else {
        (PK_CARD, PK_CARD_BORDER, PK_MUTED)
    };
    draw_rounded_box(hdc, close_x, close_y, close_w, close_h, c_bg, c_border, 6);
    let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
    SetTextColor(hdc, c_fg);
    let mut close_txt: Vec<u16> = "× Close".encode_utf16().collect();
    let mut close_rc = RECT {
        left: close_x,
        top: close_y,
        right: close_x + close_w,
        bottom: close_y + close_h,
    };
    let _ = DrawTextW(
        hdc,
        &mut close_txt,
        &mut close_rc,
        DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    // Content Cards Container start Y
    let body_y = header_y + 60;

    // Render active category views
    match active_cat {
        0 => draw_category_general(
            hdc, content_x, body_y, content_w, hover_id, font_title, font_sub, font_btn,
            font_mono, font_badge,
        ),
        1 => draw_category_display(
            hdc, content_x, body_y, content_w, mode, hover_id, font_title, font_sub, font_btn,
        ),
        2 => draw_category_codecs(
            hdc, content_x, body_y, content_w, hover_id, font_title, font_sub, font_btn,
            font_badge,
        ),
        3 => draw_category_audio(
            hdc, content_x, body_y, content_w, hover_id, font_title, font_sub, font_btn,
        ),
        4 => draw_category_security(
            hdc, content_x, body_y, content_w, hover_id, font_title, font_sub, font_btn,
        ),
        5 => draw_category_recording(
            hdc, content_x, body_y, content_w, hover_id, font_title, font_sub, font_btn,
            font_mono,
        ),
        _ => draw_category_about(
            hdc, content_x, body_y, content_w, orig_w, orig_h, hover_id, font_title, font_sub,
            font_btn, font_mono,
        ),
    }

    // GDI Cleanup
    let _ = DeleteObject(HGDIOBJ(font_header.0));
    let _ = DeleteObject(HGDIOBJ(font_title.0));
    let _ = DeleteObject(HGDIOBJ(font_tab.0));
    let _ = DeleteObject(HGDIOBJ(font_sub.0));
    let _ = DeleteObject(HGDIOBJ(font_btn.0));
    let _ = DeleteObject(HGDIOBJ(font_mono.0));
    let _ = DeleteObject(HGDIOBJ(font_badge.0));
}

// =========================================================================
// Category 0: General (Desktop Shortcut & Windows Installation Highlight)
// =========================================================================
unsafe fn draw_category_general(
    hdc: HDC,
    cx: i32,
    cy: i32,
    cw: i32,
    hover_id: i32,
    f_title: windows::Win32::Graphics::Gdi::HFONT,
    f_sub: windows::Win32::Graphics::Gdi::HFONT,
    f_btn: windows::Win32::Graphics::Gdi::HFONT,
    f_mono: windows::Win32::Graphics::Gdi::HFONT,
    f_badge: windows::Win32::Graphics::Gdi::HFONT,
) {
    let is_installed = crate::windows::install::is_installed();

    // 1. Hero Card: Windows Integration & Desktop Shortcut
    let hero_h = 148;
    draw_rounded_box(hdc, cx, cy, cw, hero_h, PK_CARD, PK_CARD_BORDER, 10);

    // Title & Status Badge
    let _ = SelectObject(hdc, HGDIOBJ(f_title.0));
    SetTextColor(hdc, PK_TEXT);
    let mut h_txt: Vec<u16> = "Desktop Shortcut & Windows Start Integration"
        .encode_utf16()
        .collect();
    let mut h_rc = RECT {
        left: cx + 18,
        top: cy + 16,
        right: cx + cw - 180,
        bottom: cy + 38,
    };
    let _ = DrawTextW(
        hdc,
        &mut h_txt,
        &mut h_rc,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    // Status Pill on top-right of Hero Card
    let (badge_txt_str, badge_bg, badge_fg) = if is_installed {
        ("✓ INSTALLED", PK_GREEN_TINT, PK_GREEN_DARK)
    } else {
        ("PORTABLE RUNTIME", COLORREF(0x00EBF0FF), COLORREF(0x002B37DF))
    };
    let b_w = 140;
    let b_h = 24;
    let b_x = cx + cw - b_w - 18;
    let b_y = cy + 16;
    draw_rounded_box(hdc, b_x, b_y, b_w, b_h, badge_bg, PK_CARD_BORDER, 6);
    let _ = SelectObject(hdc, HGDIOBJ(f_badge.0));
    SetTextColor(hdc, badge_fg);
    let mut b_txt: Vec<u16> = badge_txt_str.encode_utf16().collect();
    let mut b_rc = RECT {
        left: b_x,
        top: b_y,
        right: b_x + b_w,
        bottom: b_y + b_h,
    };
    let _ = DrawTextW(
        hdc,
        &mut b_txt,
        &mut b_rc,
        DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    // Subtitle description
    let _ = SelectObject(hdc, HGDIOBJ(f_sub.0));
    SetTextColor(hdc, PK_MUTED);
    let mut sub_txt: Vec<u16> =
        "Pin Desky to your Desktop and Windows Start Search so you can launch it instantly without searching Downloads."
            .encode_utf16()
            .collect();
    let mut sub_rc = RECT {
        left: cx + 18,
        top: cy + 42,
        right: cx + cw - 18,
        bottom: cy + 78,
    };
    let _ = DrawTextW(hdc, &mut sub_txt, &mut sub_rc, DT_LEFT | DT_WORDBREAK | DT_NOPREFIX);

    // Action Button: [ 📌 Create Desktop Shortcut / Install ]
    let btn_w = 260;
    let btn_h = 38;
    let btn_x = cx + 18;
    let btn_y = cy + 90;
    let is_btn_hover = hover_id == 201;

    let btn_label = if is_installed {
        "📌 Re-create Desktop Shortcut"
    } else {
        "📌 Install to Start & Desktop"
    };

    let (bg, border, fg) = if is_btn_hover {
        (PK_GREEN_DARK, PK_GREEN_DARK, COLORREF(0x00FFFFFF))
    } else {
        (PK_GREEN, PK_GREEN, COLORREF(0x00FFFFFF))
    };

    draw_rounded_box(hdc, btn_x, btn_y, btn_w, btn_h, bg, border, 6);
    let _ = SelectObject(hdc, HGDIOBJ(f_btn.0));
    SetTextColor(hdc, fg);
    let mut act_txt: Vec<u16> = btn_label.encode_utf16().collect();
    let mut act_rc = RECT {
        left: btn_x,
        top: btn_y,
        right: btn_x + btn_w,
        bottom: btn_y + btn_h,
    };
    let _ = DrawTextW(
        hdc,
        &mut act_txt,
        &mut act_rc,
        DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    // Uninstall / remove button (if installed)
    if is_installed {
        let uninst_w = 160;
        let uninst_x = btn_x + btn_w + 12;
        let is_un_hover = hover_id == 202;
        let (ubg, ubdr, ufg) = if is_un_hover {
            (COLORREF(0x00EBF0FF), PK_RED, PK_RED)
        } else {
            (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
        };
        draw_rounded_box(hdc, uninst_x, btn_y, uninst_w, btn_h, ubg, ubdr, 6);
        SetTextColor(hdc, ufg);
        let mut un_txt: Vec<u16> = "Remove Shortcuts".encode_utf16().collect();
        let mut un_rc = RECT {
            left: uninst_x,
            top: btn_y,
            right: uninst_x + uninst_w,
            bottom: btn_y + btn_h,
        };
        let _ = DrawTextW(
            hdc,
            &mut un_txt,
            &mut un_rc,
            DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
    }

    // 2. Card: Client Identity & Relay Matchmaker
    let card2_y = cy + hero_h + 16;
    let card2_h = 148;
    draw_rounded_box(hdc, cx, card2_y, cw, card2_h, PK_CARD, PK_CARD_BORDER, 10);

    let _ = SelectObject(hdc, HGDIOBJ(f_title.0));
    SetTextColor(hdc, PK_TEXT);
    let mut id_head_txt: Vec<u16> = "Machine Identity & P2P Matchmaker"
        .encode_utf16()
        .collect();
    let mut id_head_rc = RECT {
        left: cx + 18,
        top: card2_y + 16,
        right: cx + cw - 18,
        bottom: card2_y + 38,
    };
    let _ = DrawTextW(
        hdc,
        &mut id_head_txt,
        &mut id_head_rc,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    let ident = crate::identity::load_or_create_identity();
    let local_id = {
        let guard = crate::windows::ui::launcher::LOCAL_DESKY_ID.lock().unwrap();
        if !guard.is_empty() {
            guard.clone()
        } else {
            ident.desky_id.clone().unwrap_or_else(|| "Connecting...".into())
        }
    };
    let vds_server = std::env::var("DESKY_VDS_SERVER")
        .unwrap_or_else(|_| crate::config::DEFAULT_VDS_SERVER.into());

    let rows = [
        ("Assigned Desky ID", local_id),
        ("Hardware Machine GUID", ident.machine_id),
        ("P2P Relay Server", vds_server),
    ];

    let row_h = 30;
    let row_start_y = card2_y + 46;
    for (i, (k, v)) in rows.iter().enumerate() {
        let ry = row_start_y + (i as i32) * row_h;
        let _ = SelectObject(hdc, HGDIOBJ(f_sub.0));
        SetTextColor(hdc, PK_MUTED);
        let mut kw: Vec<u16> = k.encode_utf16().collect();
        let mut k_rc = RECT {
            left: cx + 18,
            top: ry,
            right: cx + 220,
            bottom: ry + row_h,
        };
        let _ = DrawTextW(
            hdc,
            &mut kw,
            &mut k_rc,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );

        let _ = SelectObject(hdc, HGDIOBJ(f_mono.0));
        SetTextColor(hdc, PK_GREEN_DARK);
        let mut vw: Vec<u16> = v.encode_utf16().collect();
        let mut v_rc = RECT {
            left: cx + 220,
            top: ry,
            right: cx + cw - 18,
            bottom: ry + row_h,
        };
        let _ = DrawTextW(
            hdc,
            &mut vw,
            &mut v_rc,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
    }
}

// =========================================================================
// Category 1: Display & Stream
// =========================================================================
unsafe fn draw_category_display(
    hdc: HDC,
    cx: i32,
    cy: i32,
    cw: i32,
    mode: u8,
    hover_id: i32,
    f_title: windows::Win32::Graphics::Gdi::HFONT,
    f_sub: windows::Win32::Graphics::Gdi::HFONT,
    f_btn: windows::Win32::Graphics::Gdi::HFONT,
) {
    let card_h = 108;
    let gap = 10;

    // 1. Display Scaling Mode
    let c0_y = cy;
    draw_card_frame(
        hdc,
        cx,
        c0_y,
        cw,
        card_h,
        "Viewport Scaling Mode",
        "Choose how remote desktop geometry fits your local window",
        f_title,
        f_sub,
    );
    let opts0 = ["Auto Fit (Aspect)", "Fill Window (Stretch)", "1:1 Native Resolution"];
    draw_segment_pills(
        hdc,
        cx + 18,
        c0_y + 64,
        cw - 36,
        34,
        &opts0,
        mode as usize,
        300,
        hover_id,
        f_btn,
    );

    // 2. Stream Quality & Bandwidth
    let c1_y = c0_y + card_h + gap;
    draw_card_frame(
        hdc,
        cx,
        c1_y,
        cw,
        card_h,
        "Stream Quality & Bandwidth Profile",
        "Optimize bandwidth consumption vs image crispness",
        f_title,
        f_sub,
    );
    let q_mode = crate::p2p::get_quality_mode();
    let q_idx = match q_mode {
        4 => 0, // Low (Data Saver)
        3 => 1, // Balanced (Standard)
        2 => 2, // High Quality
        1 => 3, // Lossless 4:4:4
        _ => 1, // Default Balanced
    };
    let opts1 = ["Low (Data Saver)", "Balanced (Standard)", "High Quality", "Lossless 4:4:4"];
    draw_segment_pills(
        hdc,
        cx + 18,
        c1_y + 64,
        cw - 36,
        34,
        &opts1,
        q_idx,
        310,
        hover_id,
        f_btn,
    );

    // 3. Stream Buffer & Memory Cache
    let c2_y = c1_y + card_h + gap;
    draw_card_frame(
        hdc,
        cx,
        c2_y,
        cw,
        card_h,
        "Stream Buffer & Memory Cache",
        "Allocates dedicated RAM buffer for packet jitter smoothing & zero-black-screen decoding",
        f_title,
        f_sub,
    );
    let buf_tier = crate::p2p::get_client_buffer_tier();
    let opts_buf = ["128 MB (Light)", "256 MB (Standard)", "512 MB (High Stability)", "1024 MB (Max Buffer)"];
    draw_segment_pills(
        hdc,
        cx + 18,
        c2_y + 64,
        cw - 36,
        34,
        &opts_buf,
        buf_tier,
        320,
        hover_id,
        f_btn,
    );

    // 4. Target Framerate Limit
    let c3_y = c2_y + card_h + gap;
    draw_card_frame(
        hdc,
        cx,
        c3_y,
        cw,
        card_h,
        "Target Framerate Limit",
        "Higher framerates provide smoother cursor tracking and video",
        f_title,
        f_sub,
    );
    let cur_fps = TARGET_FPS.load(Ordering::Relaxed);
    let fps_idx = match cur_fps {
        30 => 0,
        120 => 2,
        _ => 1,
    };
    let opts2 = ["30 FPS (Power Saver)", "60 FPS (Fluid Desktop)", "120 FPS (Ultra Smooth)"];
    draw_segment_pills(
        hdc,
        cx + 18,
        c3_y + 64,
        cw - 36,
        34,
        &opts2,
        fps_idx,
        330,
        hover_id,
        f_btn,
    );

    // 5. Remote Mouse Cursor
    let c4_y = c3_y + card_h + gap;
    draw_card_frame(
        hdc,
        cx,
        c4_y,
        cw,
        card_h,
        "Remote Cursor Visibility",
        "Render remote host hardware cursor on top of video feed",
        f_title,
        f_sub,
    );
    let cur_on = CAPTURE_CURSOR.load(Ordering::Relaxed);
    let cur_idx = if cur_on { 0 } else { 1 };
    let opts3 = ["Show Remote Cursor", "Hide Remote Cursor"];
    draw_segment_pills(
        hdc,
        cx + 18,
        c4_y + 64,
        cw - 36,
        34,
        &opts3,
        cur_idx,
        340,
        hover_id,
        f_btn,
    );
}

// =========================================================================
// Category 2: Hardware Codecs
// =========================================================================
unsafe fn draw_category_codecs(
    hdc: HDC,
    cx: i32,
    cy: i32,
    cw: i32,
    hover_id: i32,
    f_title: windows::Win32::Graphics::Gdi::HFONT,
    f_sub: windows::Win32::Graphics::Gdi::HFONT,
    f_btn: windows::Win32::Graphics::Gdi::HFONT,
    f_badge: windows::Win32::Graphics::Gdi::HFONT,
) {
    let card_h = 118;
    let gap = 14;

    // 1. Video Codec Selection
    let c0_y = cy;
    draw_card_frame(
        hdc,
        cx,
        c0_y,
        cw,
        card_h,
        "Primary Streaming Codec",
        "Select hardware-accelerated encoder/decoder format",
        f_title,
        f_sub,
    );
    let pref = crate::p2p::get_client_codec_pref();
    let opts = ["Auto (Recommended)", "H.265 / HEVC", "H.264 / AVC", "JPEG Software"];
    draw_segment_pills(
        hdc,
        cx + 18,
        c0_y + 68,
        cw - 36,
        36,
        &opts,
        pref as usize,
        400,
        hover_id,
        f_btn,
    );

    // 2. Hardware Engine Diagnostics Card
    let c1_y = c0_y + card_h + gap;
    let c1_h = 168;
    draw_rounded_box(hdc, cx, c1_y, cw, c1_h, PK_CARD, PK_CARD_BORDER, 10);

    let _ = SelectObject(hdc, HGDIOBJ(f_title.0));
    SetTextColor(hdc, PK_TEXT);
    let mut h_txt: Vec<u16> = "GPU Hardware Acceleration Engine".encode_utf16().collect();
    let mut h_rc = RECT {
        left: cx + 18,
        top: c1_y + 16,
        right: cx + cw - 18,
        bottom: c1_y + 38,
    };
    let _ = DrawTextW(
        hdc,
        &mut h_txt,
        &mut h_rc,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    let _ = SelectObject(hdc, HGDIOBJ(f_sub.0));
    SetTextColor(hdc, PK_MUTED);
    let mut sub_txt: Vec<u16> =
        "Hardware encoders offload rendering from CPU to dedicated GPU silicon."
            .encode_utf16()
            .collect();
    let mut sub_rc = RECT {
        left: cx + 18,
        top: c1_y + 38,
        right: cx + cw - 18,
        bottom: c1_y + 58,
    };
    let _ = DrawTextW(
        hdc,
        &mut sub_txt,
        &mut sub_rc,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    let h264_ok = crate::platform::codec::is_h264_hardware_available();
    let h265_ok = crate::platform::codec::is_h265_hardware_available();

    let hw_rows = [
        (
            "DirectX DXGI H.264 AVC",
            if h264_ok { "Hardware Accelerated (Active)" } else { "Software Fallback" },
            h264_ok,
        ),
        (
            "DirectX DXGI H.265 HEVC",
            if h265_ok { "Hardware Accelerated (Available)" } else { "Not Detected" },
            h265_ok,
        ),
        ("Color Subsampling Pipeline", "YUV 4:4:4 Full Dynamic Range", true),
    ];

    let row_h = 30;
    let r_start_y = c1_y + 68;
    for (i, (name, val, is_good)) in hw_rows.iter().enumerate() {
        let ry = r_start_y + (i as i32) * row_h;
        let _ = SelectObject(hdc, HGDIOBJ(f_sub.0));
        SetTextColor(hdc, PK_MUTED);
        let mut nw: Vec<u16> = name.encode_utf16().collect();
        let mut n_rc = RECT {
            left: cx + 18,
            top: ry,
            right: cx + 240,
            bottom: ry + row_h,
        };
        let _ = DrawTextW(
            hdc,
            &mut nw,
            &mut n_rc,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );

        let fg = if *is_good { PK_GREEN_DARK } else { PK_RED };
        let _ = SelectObject(hdc, HGDIOBJ(f_badge.0));
        SetTextColor(hdc, fg);
        let mut vw: Vec<u16> = val.encode_utf16().collect();
        let mut v_rc = RECT {
            left: cx + 240,
            top: ry,
            right: cx + cw - 18,
            bottom: ry + row_h,
        };
        let _ = DrawTextW(
            hdc,
            &mut vw,
            &mut v_rc,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
    }
}

// =========================================================================
// Category 3: Audio & Voice
// =========================================================================
unsafe fn draw_category_audio(
    hdc: HDC,
    cx: i32,
    cy: i32,
    cw: i32,
    hover_id: i32,
    f_title: windows::Win32::Graphics::Gdi::HFONT,
    f_sub: windows::Win32::Graphics::Gdi::HFONT,
    f_btn: windows::Win32::Graphics::Gdi::HFONT,
) {
    let card_h = 118;
    let gap = 14;

    // 1. Desktop Audio Streaming
    let c0_y = cy;
    draw_card_frame(
        hdc,
        cx,
        c0_y,
        cw,
        card_h,
        "Remote Desktop Audio Streaming",
        "Stream remote system sounds through local speakers",
        f_title,
        f_sub,
    );
    let is_muted = crate::windows::audio::is_audio_muted();
    let audio_idx = if is_muted { 1 } else { 0 };
    let opts0 = ["Audio Enabled (Streaming)", "Audio Muted"];
    draw_segment_pills(
        hdc,
        cx + 18,
        c0_y + 68,
        cw - 36,
        36,
        &opts0,
        audio_idx,
        500,
        hover_id,
        f_btn,
    );

    // 2. Intercom Voice Microphone
    let c1_y = c0_y + card_h + gap;
    draw_card_frame(
        hdc,
        cx,
        c1_y,
        cw,
        card_h,
        "Intercom Voice Talk (Microphone)",
        "Transmit your local microphone audio to remote host",
        f_title,
        f_sub,
    );
    let is_voice = crate::windows::audio::is_intercom_voice_active();
    let voice_idx = if is_voice { 0 } else { 1 };
    let opts1 = ["Microphone Active (Transmitting)", "Microphone Off"];
    draw_segment_pills(
        hdc,
        cx + 18,
        c1_y + 68,
        cw - 36,
        36,
        &opts1,
        voice_idx,
        510,
        hover_id,
        f_btn,
    );
}

// =========================================================================
// Category 4: Security & Access
// =========================================================================
unsafe fn draw_category_security(
    hdc: HDC,
    cx: i32,
    cy: i32,
    cw: i32,
    hover_id: i32,
    f_title: windows::Win32::Graphics::Gdi::HFONT,
    f_sub: windows::Win32::Graphics::Gdi::HFONT,
    f_btn: windows::Win32::Graphics::Gdi::HFONT,
) {
    let card_h = 126;
    let gap = 14;

    // 1. Unattended Access Password
    let c0_y = cy;
    draw_rounded_box(hdc, cx, c0_y, cw, card_h, PK_CARD, PK_CARD_BORDER, 10);
    let _ = SelectObject(hdc, HGDIOBJ(f_title.0));
    SetTextColor(hdc, PK_TEXT);
    let mut h_txt: Vec<u16> = "Unattended Access (Preset Password)".encode_utf16().collect();
    let mut h_rc = RECT {
        left: cx + 18,
        top: c0_y + 16,
        right: cx + cw - 18,
        bottom: c0_y + 38,
    };
    let _ = DrawTextW(
        hdc,
        &mut h_txt,
        &mut h_rc,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    let _ = SelectObject(hdc, HGDIOBJ(f_sub.0));
    SetTextColor(hdc, PK_MUTED);
    let mut sub_txt: Vec<u16> =
        "Allow authorized clients to connect automatically using an unattended password without manual popup approval."
            .encode_utf16()
            .collect();
    let mut sub_rc = RECT {
        left: cx + 18,
        top: c0_y + 38,
        right: cx + cw - 18,
        bottom: c0_y + 62,
    };
    let _ = DrawTextW(hdc, &mut sub_txt, &mut sub_rc, DT_LEFT | DT_WORDBREAK | DT_NOPREFIX);

    let is_unattended = crate::identity::is_unattended_enabled();
    let btn_w = 260;
    let btn_h = 36;
    let btn_x = cx + 18;
    let btn_y = c0_y + 72;
    let is_btn_hover = hover_id == 601;

    let (btn_txt_str, bg, border, fg) = if is_unattended {
        ("Disable Unattended Access", PK_RED, PK_RED, COLORREF(0x00FFFFFF))
    } else if is_btn_hover {
        ("Enable Unattended Password", PK_GREEN_DARK, PK_GREEN_DARK, COLORREF(0x00FFFFFF))
    } else {
        ("Enable Unattended Password", PK_GREEN, PK_GREEN, COLORREF(0x00FFFFFF))
    };

    draw_rounded_box(hdc, btn_x, btn_y, btn_w, btn_h, bg, border, 6);
    let _ = SelectObject(hdc, HGDIOBJ(f_btn.0));
    SetTextColor(hdc, fg);
    let mut bw: Vec<u16> = btn_txt_str.encode_utf16().collect();
    let mut b_rc = RECT {
        left: btn_x,
        top: btn_y,
        right: btn_x + btn_w,
        bottom: btn_y + btn_h,
    };
    let _ = DrawTextW(
        hdc,
        &mut bw,
        &mut b_rc,
        DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    // 2. Encryption Card
    let c1_y = c0_y + card_h + gap;
    let c1_h = 96;
    draw_rounded_box(hdc, cx, c1_y, cw, c1_h, PK_CARD, PK_CARD_BORDER, 10);

    let _ = SelectObject(hdc, HGDIOBJ(f_title.0));
    SetTextColor(hdc, PK_TEXT);
    let mut e_txt: Vec<u16> = "End-to-End Encryption (E2EE)".encode_utf16().collect();
    let mut e_rc = RECT {
        left: cx + 18,
        top: c1_y + 16,
        right: cx + cw - 18,
        bottom: c1_y + 38,
    };
    let _ = DrawTextW(
        hdc,
        &mut e_txt,
        &mut e_rc,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    let _ = SelectObject(hdc, HGDIOBJ(f_sub.0));
    SetTextColor(hdc, PK_MUTED);
    let mut esub_txt: Vec<u16> =
        "All video, keyboard, mouse, and clipboard streams are authenticated and encrypted with ChaCha20-Poly1305."
            .encode_utf16()
            .collect();
    let mut esub_rc = RECT {
        left: cx + 18,
        top: c1_y + 38,
        right: cx + cw - 18,
        bottom: c1_y + 74,
    };
    let _ = DrawTextW(hdc, &mut esub_txt, &mut esub_rc, DT_LEFT | DT_WORDBREAK | DT_NOPREFIX);
}

// =========================================================================
// Category 5: Session Recording
// =========================================================================
unsafe fn draw_category_recording(
    hdc: HDC,
    cx: i32,
    cy: i32,
    cw: i32,
    hover_id: i32,
    f_title: windows::Win32::Graphics::Gdi::HFONT,
    f_sub: windows::Win32::Graphics::Gdi::HFONT,
    f_btn: windows::Win32::Graphics::Gdi::HFONT,
    f_mono: windows::Win32::Graphics::Gdi::HFONT,
) {
    let card_h = 118;
    let gap = 14;

    // 1. Audio Channels
    let c0_y = cy;
    draw_card_frame(
        hdc,
        cx,
        c0_y,
        cw,
        card_h,
        "Audio Ingestion Channels",
        "Include remote sound or local microphone in screen recording",
        f_title,
        f_sub,
    );
    let rem_audio = crate::recorder::is_remote_audio_recording_enabled();
    let opts0 = ["Remote Audio (Recorded)", "Remote Audio (Muted)"];
    draw_segment_pills(
        hdc,
        cx + 18,
        c0_y + 68,
        cw - 36,
        36,
        &opts0,
        if rem_audio { 0 } else { 1 },
        700,
        hover_id,
        f_btn,
    );

    // 2. Recording Quality Scale
    let c1_y = c0_y + card_h + gap;
    draw_card_frame(
        hdc,
        cx,
        c1_y,
        cw,
        card_h,
        "Recording Video Quality & Resolution",
        "Resolution scale for captured session video files",
        f_title,
        f_sub,
    );
    let scale_idx = crate::recorder::get_recording_quality_scale() as usize;
    let opts1 = ["100% Native Resolution", "75% High Quality", "50% Compressed"];
    draw_segment_pills(
        hdc,
        cx + 18,
        c1_y + 68,
        cw - 36,
        36,
        &opts1,
        scale_idx.min(2),
        710,
        hover_id,
        f_btn,
    );

    // 3. Save Directory Card
    let c2_y = c1_y + card_h + gap;
    let c2_h = 92;
    draw_rounded_box(hdc, cx, c2_y, cw, c2_h, PK_CARD, PK_CARD_BORDER, 10);

    let _ = SelectObject(hdc, HGDIOBJ(f_title.0));
    SetTextColor(hdc, PK_TEXT);
    let mut dir_txt: Vec<u16> = "Recording Save Directory".encode_utf16().collect();
    let mut dir_rc = RECT {
        left: cx + 18,
        top: c2_y + 16,
        right: cx + cw - 18,
        bottom: c2_y + 38,
    };
    let _ = DrawTextW(
        hdc,
        &mut dir_txt,
        &mut dir_rc,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    let dir = crate::recorder::get_recording_save_dir();
    let _ = SelectObject(hdc, HGDIOBJ(f_mono.0));
    SetTextColor(hdc, PK_MUTED);
    let mut path_txt: Vec<u16> = dir.to_string_lossy().encode_utf16().collect();
    let mut path_rc = RECT {
        left: cx + 18,
        top: c2_y + 44,
        right: cx + cw - 160,
        bottom: c2_y + 76,
    };
    let _ = DrawTextW(
        hdc,
        &mut path_txt,
        &mut path_rc,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    // [ Open Folder ] Button
    let btn_w = 130;
    let btn_h = 36;
    let btn_x = cx + cw - btn_w - 18;
    let btn_y = c2_y + 42;
    let is_open_hover = hover_id == 720;
    let (bg, border, fg) = if is_open_hover {
        (PK_BTN_HOVER, PK_GREEN, PK_GREEN_DARK)
    } else {
        (PK_INPUT_BG, PK_CARD_BORDER, PK_TEXT)
    };
    draw_rounded_box(hdc, btn_x, btn_y, btn_w, btn_h, bg, border, 6);
    let _ = SelectObject(hdc, HGDIOBJ(f_btn.0));
    SetTextColor(hdc, fg);
    let mut ob_txt: Vec<u16> = "Open Folder".encode_utf16().collect();
    let mut ob_rc = RECT {
        left: btn_x,
        top: btn_y,
        right: btn_x + btn_w,
        bottom: btn_y + btn_h,
    };
    let _ = DrawTextW(
        hdc,
        &mut ob_txt,
        &mut ob_rc,
        DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
}

// =========================================================================
// Category 6: About & Diagnostics
// =========================================================================
unsafe fn draw_category_about(
    hdc: HDC,
    cx: i32,
    cy: i32,
    cw: i32,
    orig_w: u32,
    orig_h: u32,
    hover_id: i32,
    f_title: windows::Win32::Graphics::Gdi::HFONT,
    f_sub: windows::Win32::Graphics::Gdi::HFONT,
    f_btn: windows::Win32::Graphics::Gdi::HFONT,
    f_mono: windows::Win32::Graphics::Gdi::HFONT,
) {
    let c0_y = cy;
    let c0_h = 176;
    draw_rounded_box(hdc, cx, c0_y, cw, c0_h, PK_CARD, PK_CARD_BORDER, 10);

    let _ = SelectObject(hdc, HGDIOBJ(f_title.0));
    SetTextColor(hdc, PK_TEXT);
    let mut h_txt: Vec<u16> = "Live Network Diagnostics".encode_utf16().collect();
    let mut h_rc = RECT {
        left: cx + 18,
        top: c0_y + 16,
        right: cx + cw - 18,
        bottom: c0_y + 38,
    };
    let _ = DrawTextW(
        hdc,
        &mut h_txt,
        &mut h_rc,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    let rtt_ms = {
        let rtt = crate::p2p::get_session_rtt_ms();
        if rtt > 0 {
            rtt
        } else {
            crate::p2p::get_server_ping_ms()
        }
    };
    let kbps = crate::p2p::get_session_bitrate_kbps();
    let bitrate_str = if kbps > 0 {
        format_bitrate(kbps)
    } else {
        "Direct P2P Link (Standby)".to_string()
    };
    let res_str = if orig_w > 0 && orig_h > 0 {
        format!("{}x{} Native", orig_w, orig_h)
    } else {
        "100% Native".to_string()
    };

    let rows = [
        (
            "Network Latency (RTT)",
            if rtt_ms > 0 {
                format!("{} ms", rtt_ms)
            } else {
                "Local LAN (<1 ms)".into()
            },
        ),
        ("Live Stream Bitrate", bitrate_str),
        (
            "Active Stream Codec",
            crate::p2p::get_active_stream_codec_name().to_string(),
        ),
        ("Viewport Resolution", res_str),
    ];

    let row_h = 28;
    let start_y = c0_y + 48;
    for (i, (k, v)) in rows.iter().enumerate() {
        let ry = start_y + (i as i32) * row_h;
        let _ = SelectObject(hdc, HGDIOBJ(f_sub.0));
        SetTextColor(hdc, PK_MUTED);
        let mut kw: Vec<u16> = k.encode_utf16().collect();
        let mut k_rc = RECT {
            left: cx + 18,
            top: ry,
            right: cx + 220,
            bottom: ry + row_h,
        };
        let _ = DrawTextW(
            hdc,
            &mut kw,
            &mut k_rc,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );

        let _ = SelectObject(hdc, HGDIOBJ(f_mono.0));
        SetTextColor(hdc, PK_GREEN_DARK);
        let mut vw: Vec<u16> = v.encode_utf16().collect();
        let mut v_rc = RECT {
            left: cx + 220,
            top: ry,
            right: cx + cw - 18,
            bottom: ry + row_h,
        };
        let _ = DrawTextW(
            hdc,
            &mut vw,
            &mut v_rc,
            DT_RIGHT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
    }

    // 2. Network Tunneling & Routing Mode Card
    let c1_y = c0_y + c0_h + 14;
    let c1_h = 108;
    draw_card_frame(
        hdc,
        cx,
        c1_y,
        cw,
        c1_h,
        "Network Tunneling & Routing Mode",
        "Choose whether traffic routes direct P2P, via encrypted server tunnel, or offline serverless",
        f_title,
        f_sub,
    );
    let r_mode = crate::p2p::get_routing_mode();
    let opts_routing = [
        "Smart P2P (Auto)",
        "Direct Server Tunnel",
        "Serverless (LAN / IP)",
    ];
    draw_segment_pills(
        hdc,
        cx + 18,
        c1_y + 64,
        cw - 36,
        34,
        &opts_routing,
        r_mode as usize,
        820,
        hover_id,
        f_btn,
    );

    // 3. About pkasoft Desky Card
    let c2_y = c1_y + c1_h + 14;
    let c2_h = 138;
    draw_rounded_box(hdc, cx, c2_y, cw, c2_h, PK_CARD, PK_CARD_BORDER, 10);

    let _ = SelectObject(hdc, HGDIOBJ(f_title.0));
    SetTextColor(hdc, PK_TEXT);
    let mut a_txt: Vec<u16> = "Desky Remote Desktop".encode_utf16().collect();
    let mut a_rc = RECT {
        left: cx + 18,
        top: c2_y + 16,
        right: cx + cw - 18,
        bottom: c2_y + 38,
    };
    let _ = DrawTextW(
        hdc,
        &mut a_txt,
        &mut a_rc,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    let _ = SelectObject(hdc, HGDIOBJ(f_sub.0));
    SetTextColor(hdc, PK_MUTED);
    let mut desc_txt: Vec<u16> = format!(
        "Version {} • Built with Rust and native Win32 direct rendering.\nLicensed under GNU AGPLv3 for open personal use. Developed by pkasoft studio.",
        env!("CARGO_PKG_VERSION")
    ).encode_utf16().collect();
    let mut desc_rc = RECT {
        left: cx + 18,
        top: c2_y + 40,
        right: cx + cw - 18,
        bottom: c2_y + 84,
    };
    let _ = DrawTextW(hdc, &mut desc_txt, &mut desc_rc, DT_LEFT | DT_WORDBREAK | DT_NOPREFIX);

    // [ Visit pkasoft.com ] Button
    let btn_w = 160;
    let btn_h = 34;
    let btn_x = cx + 18;
    let btn_y = c2_y + 92;
    let is_web_hover = hover_id == 801;
    let (bg, border, fg) = if is_web_hover {
        (PK_GREEN_DARK, PK_GREEN_DARK, COLORREF(0x00FFFFFF))
    } else {
        (PK_GREEN, PK_GREEN, COLORREF(0x00FFFFFF))
    };
    draw_rounded_box(hdc, btn_x, btn_y, btn_w, btn_h, bg, border, 6);
    let _ = SelectObject(hdc, HGDIOBJ(f_btn.0));
    SetTextColor(hdc, fg);
    let mut wb_txt: Vec<u16> = "pkasoft.com ↗".encode_utf16().collect();
    let mut wb_rc = RECT {
        left: btn_x,
        top: btn_y,
        right: btn_x + btn_w,
        bottom: btn_y + btn_h,
    };
    let _ = DrawTextW(
        hdc,
        &mut wb_txt,
        &mut wb_rc,
        DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
}

// =========================================================================
// UI Component Helpers
// =========================================================================
unsafe fn draw_card_frame(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    title: &str,
    subtitle: &str,
    f_title: windows::Win32::Graphics::Gdi::HFONT,
    f_sub: windows::Win32::Graphics::Gdi::HFONT,
) {
    draw_rounded_box(hdc, x, y, w, h, PK_CARD, PK_CARD_BORDER, 10);

    let _ = SelectObject(hdc, HGDIOBJ(f_title.0));
    SetTextColor(hdc, PK_TEXT);
    let mut t_txt: Vec<u16> = title.encode_utf16().collect();
    let mut t_rc = RECT {
        left: x + 18,
        top: y + 16,
        right: x + w - 18,
        bottom: y + 36,
    };
    let _ = DrawTextW(
        hdc,
        &mut t_txt,
        &mut t_rc,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    let _ = SelectObject(hdc, HGDIOBJ(f_sub.0));
    SetTextColor(hdc, PK_MUTED);
    let mut s_txt: Vec<u16> = subtitle.encode_utf16().collect();
    let mut s_rc = RECT {
        left: x + 18,
        top: y + 38,
        right: x + w - 18,
        bottom: y + 58,
    };
    let _ = DrawTextW(
        hdc,
        &mut s_txt,
        &mut s_rc,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
}

unsafe fn draw_segment_pills(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    items: &[&str],
    selected_idx: usize,
    hover_base_id: i32,
    current_hover_id: i32,
    font: windows::Win32::Graphics::Gdi::HFONT,
) {
    let count = items.len().max(1);
    let gap = 6;
    let seg_w = (w - (count as i32 - 1) * gap) / (count as i32);

    let _ = SelectObject(hdc, HGDIOBJ(font.0));

    for (i, label) in items.iter().enumerate() {
        let sx = x + (i as i32) * (seg_w + gap);
        let is_selected = i == selected_idx;
        let is_hover = current_hover_id == hover_base_id + (i as i32);

        let (bg, border, fg) = if is_selected {
            (PK_GREEN, PK_GREEN, COLORREF(0x00FFFFFF))
        } else if is_hover {
            (PK_BTN_HOVER, PK_CARD_BORDER, PK_TEXT)
        } else {
            (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
        };

        draw_rounded_box(hdc, sx, y, seg_w, h, bg, border, 6);
        SetTextColor(hdc, fg);
        let mut txt: Vec<u16> = label.encode_utf16().collect();
        let mut rc = RECT {
            left: sx,
            top: y,
            right: sx + seg_w,
            bottom: y + h,
        };
        let _ = DrawTextW(
            hdc,
            &mut txt,
            &mut rc,
            DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
    }
}

// =========================================================================
// Mouse Move & Hover Tracking
// =========================================================================
pub fn handle_settings_mousemove(
    hwnd: HWND,
    client_w: i32,
    _client_h: i32,
    client_x: i32,
    client_y: i32,
    top_y: i32,
) {
    let pad_x = if client_w >= 900 { 24 } else { 14 };
    let pad_y = 14;
    let sidebar_w = 230;
    let sidebar_x = pad_x;
    let sidebar_y = top_y + pad_y;

    let content_x = sidebar_x + sidebar_w + 20;
    let content_w = (client_w - content_x - pad_x).max(360);
    let header_y = sidebar_y + 2;
    let body_y = header_y + 60;

    let mut new_hover = -1;

    // 1. Close button [ × Close ]
    let close_w = 96;
    let close_h = 34;
    let close_x = content_x + content_w - close_w;
    if client_x >= close_x
        && client_x <= close_x + close_w
        && client_y >= header_y
        && client_y <= header_y + close_h
    {
        new_hover = 9099;
    }

    // 2. Sidebar category buttons
    let item_h = 42;
    let item_start_y = sidebar_y + 44;
    for i in 0..7 {
        let iy = item_start_y + i * (item_h + 4);
        let ix = sidebar_x + 8;
        let iw = sidebar_w - 16;
        if client_x >= ix && client_x <= ix + iw && client_y >= iy && client_y <= iy + item_h {
            new_hover = 100 + i;
            break;
        }
    }

    let active_cat = SETTINGS_CATEGORY.load(Ordering::Relaxed).min(6);

    // 3. Content controls based on active category
    if new_hover == -1 {
        match active_cat {
            0 => {
                let btn_w = 260;
                let btn_h = 38;
                let btn_x = content_x + 18;
                let btn_y = body_y + 90;
                if client_x >= btn_x
                    && client_x <= btn_x + btn_w
                    && client_y >= btn_y
                    && client_y <= btn_y + btn_h
                {
                    new_hover = 201;
                }
                let un_x = btn_x + btn_w + 12;
                let un_w = 160;
                if client_x >= un_x
                    && client_x <= un_x + un_w
                    && client_y >= btn_y
                    && client_y <= btn_y + btn_h
                {
                    new_hover = 202;
                }
            }
            1 => {
                let card_h = 108;
                let gap = 10;
                let check_segs = |cy: i32, count: usize, base: i32| -> Option<i32> {
                    let y = cy + 64;
                    let h = 34;
                    let w = content_w - 36;
                    let x = content_x + 18;
                    let seg_w = (w - (count as i32 - 1) * 6) / count as i32;
                    if client_y >= y && client_y <= y + h {
                        for s in 0..count {
                            let sx = x + (s as i32) * (seg_w + 6);
                            if client_x >= sx && client_x <= sx + seg_w {
                                return Some(base + s as i32);
                            }
                        }
                    }
                    None
                };
                if let Some(h) = check_segs(body_y, 3, 300) {
                    new_hover = h;
                } else if let Some(h) = check_segs(body_y + card_h + gap, 4, 310) {
                    new_hover = h;
                } else if let Some(h) = check_segs(body_y + (card_h + gap) * 2, 4, 320) {
                    new_hover = h;
                } else if let Some(h) = check_segs(body_y + (card_h + gap) * 3, 3, 330) {
                    new_hover = h;
                } else if let Some(h) = check_segs(body_y + (card_h + gap) * 4, 2, 340) {
                    new_hover = h;
                }
            }
            2 => {
                let y = body_y + 68;
                let h = 36;
                let w = content_w - 36;
                let x = content_x + 18;
                let seg_w = (w - 3 * 6) / 4;
                if client_y >= y && client_y <= y + h {
                    for s in 0..4 {
                        let sx = x + (s as i32) * (seg_w + 6);
                        if client_x >= sx && client_x <= sx + seg_w {
                            new_hover = 400 + s as i32;
                            break;
                        }
                    }
                }
            }
            3 => {
                let card_h = 118;
                let gap = 14;
                let check_segs = |cy: i32, count: usize, base: i32| -> Option<i32> {
                    let y = cy + 68;
                    let h = 36;
                    let w = content_w - 36;
                    let x = content_x + 18;
                    let seg_w = (w - (count as i32 - 1) * 6) / count as i32;
                    if client_y >= y && client_y <= y + h {
                        for s in 0..count {
                            let sx = x + (s as i32) * (seg_w + 6);
                            if client_x >= sx && client_x <= sx + seg_w {
                                return Some(base + s as i32);
                            }
                        }
                    }
                    None
                };
                if let Some(h) = check_segs(body_y, 2, 500) {
                    new_hover = h;
                } else if let Some(h) = check_segs(body_y + card_h + gap, 2, 510) {
                    new_hover = h;
                }
            }
            4 => {
                let btn_w = 260;
                let btn_h = 36;
                let btn_x = content_x + 18;
                let btn_y = body_y + 72;
                if client_x >= btn_x
                    && client_x <= btn_x + btn_w
                    && client_y >= btn_y
                    && client_y <= btn_y + btn_h
                {
                    new_hover = 601;
                }
            }
            5 => {
                let card_h = 118;
                let gap = 14;
                let c2_y = body_y + (card_h + gap) * 2;
                let btn_w = 130;
                let btn_h = 36;
                let btn_x = content_x + content_w - btn_w - 18;
                let btn_y = c2_y + 42;
                if client_x >= btn_x
                    && client_x <= btn_x + btn_w
                    && client_y >= btn_y
                    && client_y <= btn_y + btn_h
                {
                    new_hover = 720;
                }
            }
            6 => {
                let c0_h = 176;
                let c1_y = body_y + c0_h + 14;
                let c1_h = 108;
                let c2_y = c1_y + c1_h + 14;

                let y = c1_y + 64;
                let h = 34;
                let w = content_w - 36;
                let x = content_x + 18;
                let seg_w = (w - 2 * 6) / 3;
                if client_y >= y && client_y <= y + h {
                    for s in 0..3 {
                        let sx = x + (s as i32) * (seg_w + 6);
                        if client_x >= sx && client_x <= sx + seg_w {
                            new_hover = 820 + s as i32;
                            break;
                        }
                    }
                }

                let btn_w = 160;
                let btn_h = 34;
                let btn_x = content_x + 18;
                let btn_y = c2_y + 92;
                if client_x >= btn_x
                    && client_x <= btn_x + btn_w
                    && client_y >= btn_y
                    && client_y <= btn_y + btn_h
                {
                    new_hover = 801;
                }
            }
            _ => {}
        }
    }

    let prev = SETTINGS_HOVER_ID.swap(new_hover, Ordering::Relaxed);
    if prev != new_hover {
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }

    let cursor_id = if new_hover != -1 { IDC_HAND } else { IDC_ARROW };
    unsafe {
        let _ = SetCursor(Some(LoadCursorW(None, cursor_id).unwrap_or_default()));
    }
}

// =========================================================================
// Click & Action Handling
// =========================================================================
pub fn close_settings_modal(hwnd: HWND) {
    SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
    crate::windows::ui::launcher::sync_edit_target_visibility();
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

pub fn handle_settings_click(
    hwnd: HWND,
    client_w: i32,
    _client_h: i32,
    client_x: i32,
    client_y: i32,
    top_y: i32,
) -> bool {
    let pad_x = if client_w >= 900 { 24 } else { 14 };
    let pad_y = 14;
    let sidebar_w = 230;
    let sidebar_x = pad_x;
    let sidebar_y = top_y + pad_y;

    let content_x = sidebar_x + sidebar_w + 20;
    let content_w = (client_w - content_x - pad_x).max(360);
    let header_y = sidebar_y + 2;
    let body_y = header_y + 60;

    // 1. Close Button [ × Close ]
    let close_w = 96;
    let close_h = 34;
    let close_x = content_x + content_w - close_w;
    if client_x >= close_x
        && client_x <= close_x + close_w
        && client_y >= header_y
        && client_y <= header_y + close_h
    {
        close_settings_modal(hwnd);
        return true;
    }

    // 2. Sidebar category clicks
    let item_h = 42;
    let item_start_y = sidebar_y + 44;
    for i in 0..7 {
        let iy = item_start_y + i * (item_h + 4);
        let ix = sidebar_x + 8;
        let iw = sidebar_w - 16;
        if client_x >= ix && client_x <= ix + iw && client_y >= iy && client_y <= iy + item_h {
            SETTINGS_CATEGORY.store(i as u8, Ordering::Relaxed);
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            return true;
        }
    }

    let active_cat = SETTINGS_CATEGORY.load(Ordering::Relaxed).min(6);

    // 3. Category-Specific Actions
    match active_cat {
        0 => {
            // Hero Action Button [ 📌 Install to Start & Desktop / Re-create ]
            let btn_w = 260;
            let btn_h = 38;
            let btn_x = content_x + 18;
            let btn_y = body_y + 90;
            if client_x >= btn_x
                && client_x <= btn_x + btn_w
                && client_y >= btn_y
                && client_y <= btn_y + btn_h
            {
                match crate::windows::install::install_to_system() {
                    Ok(dest) => {
                        crate::windows::ui::launcher::append_status_log(&format!(
                            "[✓] Installed to Start Menu & Desktop: {}",
                            dest.display()
                        ));
                        unsafe {
                            let msg = format!(
                                "Desky has been successfully installed!\n\n\
                                • Desktop shortcut created\n\
                                • Windows Start Menu integrated (search 'Desky' in Windows Search)\n\
                                • Installed to: {}\n\n\
                                You can now launch Desky anytime from your Desktop or Start Menu.",
                                dest.display()
                            );
                            let wide_msg: Vec<u16> =
                                msg.encode_utf16().chain(std::iter::once(0)).collect();
                            let wide_title: Vec<u16> = "Desky - Installed Successfully"
                                .encode_utf16()
                                .chain(std::iter::once(0))
                                .collect();
                            let _ = MessageBoxW(
                                Some(hwnd),
                                PCWSTR(wide_msg.as_ptr()),
                                PCWSTR(wide_title.as_ptr()),
                                MB_OK | MB_ICONINFORMATION,
                            );
                        }
                    }
                    Err(e) => {
                        crate::windows::ui::launcher::append_status_log(&format!(
                            "[!] Installation error: {}",
                            e
                        ));
                        unsafe {
                            let msg = format!("Failed to install Desky:\n\n{}", e);
                            let wide_msg: Vec<u16> =
                                msg.encode_utf16().chain(std::iter::once(0)).collect();
                            let wide_title: Vec<u16> = "Desky - Installation Error"
                                .encode_utf16()
                                .chain(std::iter::once(0))
                                .collect();
                            let _ = MessageBoxW(
                                Some(hwnd),
                                PCWSTR(wide_msg.as_ptr()),
                                PCWSTR(wide_title.as_ptr()),
                                MB_OK | MB_ICONERROR,
                            );
                        }
                    }
                }
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return true;
            }

            // [ Remove Shortcuts ]
            let un_x = btn_x + btn_w + 12;
            let un_w = 160;
            if client_x >= un_x
                && client_x <= un_x + un_w
                && client_y >= btn_y
                && client_y <= btn_y + btn_h
            {
                crate::windows::install::uninstall_from_system();
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return true;
            }
        }
        1 => {
            let card_h = 108;
            let gap = 10;
            let hit_segs = |cy: i32, count: usize| -> Option<usize> {
                let y = cy + 64;
                let h = 34;
                let w = content_w - 36;
                let x = content_x + 18;
                let seg_w = (w - (count as i32 - 1) * 6) / count as i32;
                if client_y >= y && client_y <= y + h {
                    for s in 0..count {
                        let sx = x + (s as i32) * (seg_w + 6);
                        if client_x >= sx && client_x <= sx + seg_w {
                            return Some(s);
                        }
                    }
                }
                None
            };

            // Scaling
            if let Some(idx) = hit_segs(body_y, 3) {
                DISPLAY_MODE.store(idx as u8, Ordering::Relaxed);
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return true;
            }
            // Quality
            if let Some(idx) = hit_segs(body_y + card_h + gap, 4) {
                let mode_val = match idx {
                    0 => 4, // Low (Data Saver)
                    1 => 3, // Balanced (Standard)
                    2 => 2, // High Quality
                    _ => 1, // Lossless 4:4:4
                };
                crate::p2p::set_quality_mode(mode_val);
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return true;
            }
            // Stream Buffer & Memory Cache
            if let Some(idx) = hit_segs(body_y + (card_h + gap) * 2, 4) {
                crate::p2p::set_client_buffer_tier(idx);
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return true;
            }
            // FPS
            if let Some(idx) = hit_segs(body_y + (card_h + gap) * 3, 3) {
                let fps_val = match idx {
                    0 => 30,
                    2 => 120,
                    _ => 60,
                };
                TARGET_FPS.store(fps_val, Ordering::Relaxed);
                let cur = CAPTURE_CURSOR.load(Ordering::Relaxed);
                crate::p2p::update_client_preferences(fps_val, cur);
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return true;
            }
            // Cursor
            if let Some(idx) = hit_segs(body_y + (card_h + gap) * 4, 2) {
                let cur_val = idx == 0;
                CAPTURE_CURSOR.store(cur_val, Ordering::Relaxed);
                let fps = TARGET_FPS.load(Ordering::Relaxed);
                crate::p2p::update_client_preferences(fps, cur_val);
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return true;
            }
        }
        2 => {
            let y = body_y + 68;
            let h = 36;
            let w = content_w - 36;
            let x = content_x + 18;
            let seg_w = (w - 3 * 6) / 4;
            if client_y >= y && client_y <= y + h {
                for s in 0..4 {
                    let sx = x + (s as i32) * (seg_w + 6);
                    if client_x >= sx && client_x <= sx + seg_w {
                        let h265_hw = crate::platform::codec::is_h265_hardware_available();
                        let h264_hw = crate::platform::codec::is_h264_hardware_available();
                        let is_enabled = match s {
                            1 => h265_hw,
                            2 => h264_hw,
                            _ => true,
                        };
                        if is_enabled {
                            crate::p2p::set_client_codec_pref(s as u8);
                            unsafe {
                                let _ = InvalidateRect(Some(hwnd), None, false);
                            }
                        }
                        return true;
                    }
                }
            }
        }
        3 => {
            let card_h = 118;
            let gap = 14;
            let hit_segs = |cy: i32, count: usize| -> Option<usize> {
                let y = cy + 68;
                let h = 36;
                let w = content_w - 36;
                let x = content_x + 18;
                let seg_w = (w - (count as i32 - 1) * 6) / count as i32;
                if client_y >= y && client_y <= y + h {
                    for s in 0..count {
                        let sx = x + (s as i32) * (seg_w + 6);
                        if client_x >= sx && client_x <= sx + seg_w {
                            return Some(s);
                        }
                    }
                }
                None
            };
            if let Some(idx) = hit_segs(body_y, 2) {
                let current_muted = crate::windows::audio::is_audio_muted();
                let want_muted = idx == 1;
                if current_muted != want_muted {
                    let _ = crate::windows::audio::toggle_audio_mute();
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                return true;
            }
            if let Some(idx) = hit_segs(body_y + card_h + gap, 2) {
                let current_voice = crate::windows::audio::is_intercom_voice_active();
                let want_voice = idx == 0;
                if current_voice != want_voice {
                    let _ = crate::windows::audio::toggle_intercom_voice();
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                return true;
            }
        }
        4 => {
            let btn_w = 260;
            let btn_h = 36;
            let btn_x = content_x + 18;
            let btn_y = body_y + 72;
            if client_x >= btn_x
                && client_x <= btn_x + btn_w
                && client_y >= btn_y
                && client_y <= btn_y + btn_h
            {
                if crate::identity::is_unattended_enabled() {
                    crate::identity::disable_unattended();
                } else {
                    crate::identity::set_unattended_password("Desky@2026");
                }
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return true;
            }
        }
        5 => {
            let card_h = 118;
            let gap = 14;
            let hit_segs = |cy: i32, count: usize| -> Option<usize> {
                let y = cy + 68;
                let h = 36;
                let w = content_w - 36;
                let x = content_x + 18;
                let seg_w = (w - (count as i32 - 1) * 6) / count as i32;
                if client_y >= y && client_y <= y + h {
                    for s in 0..count {
                        let sx = x + (s as i32) * (seg_w + 6);
                        if client_x >= sx && client_x <= sx + seg_w {
                            return Some(s);
                        }
                    }
                }
                None
            };
            if let Some(idx) = hit_segs(body_y, 2) {
                crate::recorder::set_remote_audio_recording_enabled(idx == 0);
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return true;
            }
            if let Some(idx) = hit_segs(body_y + card_h + gap, 3) {
                crate::recorder::set_recording_quality_scale(idx as u8);
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return true;
            }
            let c2_y = body_y + (card_h + gap) * 2;
            let btn_w = 130;
            let btn_h = 36;
            let btn_x = content_x + content_w - btn_w - 18;
            let btn_y = c2_y + 42;
            if client_x >= btn_x
                && client_x <= btn_x + btn_w
                && client_y >= btn_y
                && client_y <= btn_y + btn_h
            {
                let dir = crate::recorder::get_recording_save_dir();
                let _ = std::process::Command::new("explorer.exe").arg(&dir).spawn();
                return true;
            }
        }
        6 => {
            let c0_h = 176;
            let c1_y = body_y + c0_h + 14;
            let c1_h = 108;
            let c2_y = c1_y + c1_h + 14;

            // Hit test routing mode segments
            let y = c1_y + 64;
            let h = 34;
            let w = content_w - 36;
            let x = content_x + 18;
            let seg_w = (w - 2 * 6) / 3;
            if client_y >= y && client_y <= y + h {
                for s in 0..3 {
                    let sx = x + (s as i32) * (seg_w + 6);
                    if client_x >= sx && client_x <= sx + seg_w {
                        crate::p2p::set_routing_mode(s as u8);
                        unsafe {
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        return true;
                    }
                }
            }

            let btn_w = 160;
            let btn_h = 34;
            let btn_x = content_x + 18;
            let btn_y = c2_y + 92;
            if client_x >= btn_x
                && client_x <= btn_x + btn_w
                && client_y >= btn_y
                && client_y <= btn_y + btn_h
            {
                let _ = std::process::Command::new("explorer.exe")
                    .arg("https://pkasoft.com")
                    .spawn();
                return true;
            }
        }
        _ => {}
    }

    false
}
