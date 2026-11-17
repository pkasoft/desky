//! Viewport Top Tab Bar & Interactive Toolbar
//! Renders session tabs, Lucide vector buttons, and floating tooltips.

#![allow(unsafe_op_in_unsafe_fn)]

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use windows::Win32::Foundation::{COLORREF, HWND, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetTextExtentPoint32W,
    Rectangle, SelectObject, SetBkMode, SetTextColor, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT,
    DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_SEMIBOLD, HBRUSH, HDC, HGDIOBJ, PS_SOLID, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

use super::quick_actions::is_quick_actions_open;
use super::settings_modal::SHOW_SETTINGS_MODAL;
use super::theme::{
    create_segoe_font, draw_rounded_box, PK_BG, PK_BTN_HOVER, PK_CARD, PK_CARD_BORDER, PK_GREEN,
    PK_GREEN_DARK, PK_INPUT_BG, PK_MUTED, PK_RED, PK_TEXT,
};
use super::whiteboard::is_whiteboard_active;
use super::{SessionTab, ACTIVE_REMOTE_MONITOR, ACTIVE_TAB_INDEX, REMOTE_PRIVACY_MODE, VIEWPORT_IS_ADMIN_MODE};

pub const TOOLBAR_HEIGHT: i32 = 40;
pub static TAB_HOVER_STATE: AtomicI32 = AtomicI32::new(-1);
pub static JUST_INSTALLED: AtomicBool = AtomicBool::new(false);
pub static INSTALL_BTN_X: AtomicI32 = AtomicI32::new(0);
pub static INSTALL_BTN_W: AtomicI32 = AtomicI32::new(0);

// Modern high-visibility active vs passive button colors:
pub const TB_ACTIVE_BG: COLORREF = COLORREF(0x00D0F5E0);     // Vibrant emerald mint fill (RGB 224, 245, 208)
pub const TB_ACTIVE_BORDER: COLORREF = COLORREF(0x002B7808); // Bold forest emerald border (RGB 8, 120, 43)
pub const TB_ACTIVE_FG: COLORREF = COLORREF(0x001B4F04);     // Deep contrast emerald icon (RGB 4, 79, 27)

pub const TB_REC_BG: COLORREF = COLORREF(0x00D4DCFF);        // Vibrant soft red fill (RGB 255, 220, 212)
pub const TB_REC_BORDER: COLORREF = COLORREF(0x002B37DF);    // PK_RED
pub const TB_REC_FG: COLORREF = COLORREF(0x002B37DF);

pub const TB_PRIV_BG: COLORREF = COLORREF(0x00CEEBFF);       // Vibrant amber fill (RGB 255, 235, 206)
pub const TB_PRIV_BORDER: COLORREF = COLORREF(0x000E82E6);   // Amber border (RGB 230, 130, 14)
pub const TB_PRIV_FG: COLORREF = COLORREF(0x000A569C);       // Bronze lock (RGB 156, 86, 10)

pub const TB_MUTED_BG: COLORREF = COLORREF(0x00E8EEFF);      // Soft red/pink fill for muted audio
pub const TB_MUTED_BORDER: COLORREF = COLORREF(0x007A8BE8);  // Rose border for muted audio
pub const TB_MUTED_FG: COLORREF = COLORREF(0x002B37DF);      // Red volume_x icon

pub const TB_PASSIVE_BG: COLORREF = COLORREF(0x00FAFCFB);    // Clean off-white card fill
pub const TB_PASSIVE_BORDER: COLORREF = COLORREF(0x00DCE0DE);// Subtle 1px border
pub const TB_PASSIVE_FG: COLORREF = COLORREF(0x006E7D77);    // Slate gray passive icon

pub const TB_HOVER_BG: COLORREF = COLORREF(0x00EBF0EC);      // Hover fill
pub const TB_HOVER_BORDER: COLORREF = COLORREF(0x00B5C2BC);  // Hover border
pub const TB_HOVER_FG: COLORREF = COLORREF(0x001B2420);      // Hover icon

pub unsafe fn draw_tb_button_box(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    fill_color: COLORREF,
    border_color: COLORREF,
    border_width: i32,
) {
    let brush: HBRUSH = CreateSolidBrush(fill_color);
    let pen = CreatePen(PS_SOLID, border_width, border_color);
    let old_brush = SelectObject(hdc, HGDIOBJ(brush.0));
    let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));

    let _ = Rectangle(hdc, x, y, x + w, y + h);

    let _ = SelectObject(hdc, old_brush);
    let _ = SelectObject(hdc, old_pen);
    let _ = DeleteObject(HGDIOBJ(brush.0));
    let _ = DeleteObject(HGDIOBJ(pen.0));
}

pub unsafe fn draw_toolbar_tooltip(
    hdc: HDC,
    client_w: i32,
    tab_bottom_y: i32,
    text: &str,
    target_center_x: i32,
) {
    let font_tip = create_segoe_font(11, FW_SEMIBOLD);
    let old_f = SelectObject(hdc, HGDIOBJ(font_tip.0));
    SetBkMode(hdc, TRANSPARENT);

    let mut tip_txt: Vec<u16> = text.encode_utf16().collect();
    let mut tip_sz = SIZE::default();
    let _ = GetTextExtentPoint32W(hdc, &mut tip_txt, &mut tip_sz);

    let pad_x = 10;
    let pad_y = 4;
    let tip_w = tip_sz.cx + pad_x * 2;
    let tip_h = tip_sz.cy + pad_y * 2;

    let tip_x = (target_center_x - tip_w / 2).clamp(8, client_w - tip_w - 8);
    let tip_y = tab_bottom_y + 4;

    draw_rounded_box(
        hdc,
        tip_x,
        tip_y,
        tip_w,
        tip_h,
        super::theme::PK_CARD,
        super::theme::PK_CARD_BORDER,
        6,
    );

    SetTextColor(hdc, PK_TEXT);
    let mut tip_rc = RECT {
        left: tip_x,
        top: tip_y,
        right: tip_x + tip_w,
        bottom: tip_y + tip_h,
    };
    let _ = DrawTextW(hdc, &mut tip_txt, &mut tip_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    let _ = SelectObject(hdc, old_f);
    let _ = DeleteObject(HGDIOBJ(font_tip.0));
}

pub unsafe fn draw_tab_bar(
    hdc: HDC,
    client_w: i32,
    tab_bar_h: i32,
    tabs: &[SessionTab],
    active_idx: usize,
    hover_state: i32,
) {
    let bar_rect = RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: tab_bar_h,
    };
    let bg_brush = CreateSolidBrush(PK_BG);
    FillRect(hdc, &bar_rect, bg_brush);
    let _ = DeleteObject(HGDIOBJ(bg_brush.0));

    let border_brush = CreateSolidBrush(PK_CARD_BORDER);
    let border_rect = RECT {
        left: 0,
        top: tab_bar_h - 1,
        right: client_w,
        bottom: tab_bar_h,
    };
    FillRect(hdc, &border_rect, border_brush);

    // Left Brand Cell
    let brand_w = 92;
    let div1 = RECT {
        left: brand_w,
        top: 0,
        right: brand_w + 1,
        bottom: tab_bar_h,
    };
    FillRect(hdc, &div1, border_brush);

    let font_brand = create_segoe_font(14, FW_BOLD);
    let old_font = SelectObject(hdc, HGDIOBJ(font_brand.0));
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, PK_GREEN_DARK);
    let mut brand_txt: Vec<u16> = "DESKY".encode_utf16().collect();
    let mut brand_rc = RECT {
        left: 0,
        top: 0,
        right: brand_w,
        bottom: tab_bar_h,
    };
    let _ = DrawTextW(hdc, &mut brand_txt, &mut brand_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    let font_tab = create_segoe_font(12, FW_SEMIBOLD);

    let start_x = brand_w + 8;
    let tab_0_w = 96;
    let rem_start_x = start_x + tab_0_w + 4;
    let rem_count = (tabs.len().saturating_sub(1)).max(1) as i32;
    let tab_w = 175.min(((client_w - rem_start_x - 300) / rem_count).max(110));
    let tab_h = 30;
    let tab_y = 5;

    for (i, tab) in tabs.iter().enumerate() {
        let is_active = i == active_idx;
        let is_tab_hover = hover_state == i as i32;

        let tab_bg = if is_active {
            PK_CARD
        } else if is_tab_hover {
            PK_BTN_HOVER
        } else {
            PK_INPUT_BG
        };
        let tab_border = if is_active { PK_GREEN } else { PK_CARD_BORDER };

        if i == 0 {
            let tx = start_x;
            draw_rounded_box(hdc, tx, tab_y, tab_0_w, tab_h, tab_bg, tab_border, 6);

            if is_active {
                let accent_brush = CreateSolidBrush(PK_GREEN);
                let accent_rc = RECT {
                    left: tx + 6,
                    top: tab_y + tab_h - 3,
                    right: tx + tab_0_w - 6,
                    bottom: tab_y + tab_h - 1,
                };
                FillRect(hdc, &accent_rc, accent_brush);
                let _ = DeleteObject(HGDIOBJ(accent_brush.0));
            }

            crate::ui::draw_home(
                hdc,
                tx + 10,
                tab_y + (tab_h - 14) / 2,
                14,
                if is_active { PK_GREEN_DARK } else { PK_MUTED },
                2,
            );

            let _ = SelectObject(hdc, HGDIOBJ(font_tab.0));
            SetTextColor(hdc, if is_active { PK_GREEN_DARK } else { PK_MUTED });
            let mut title_txt: Vec<u16> = "Home".encode_utf16().collect();
            let mut title_rc = RECT {
                left: tx + 28,
                top: tab_y,
                right: tx + tab_0_w - 6,
                bottom: tab_y + tab_h,
            };
            let _ = DrawTextW(hdc, &mut title_txt, &mut title_rc, DT_SINGLELINE | DT_VCENTER);
        } else {
            let tx = rem_start_x + ((i - 1) as i32) * (tab_w + 4);
            let is_close_hover = hover_state == 1000 + i as i32;

            draw_rounded_box(hdc, tx, tab_y, tab_w, tab_h, tab_bg, tab_border, 6);

            if is_active {
                let accent_brush = CreateSolidBrush(PK_GREEN);
                let accent_rc = RECT {
                    left: tx + 6,
                    top: tab_y + tab_h - 3,
                    right: tx + tab_w - 6,
                    bottom: tab_y + tab_h - 1,
                };
                FillRect(hdc, &accent_rc, accent_brush);
                let _ = DeleteObject(HGDIOBJ(accent_brush.0));
            }

            crate::ui::draw_radio(
                hdc,
                tx + 8,
                tab_y + (tab_h - 12) / 2,
                12,
                if is_active { PK_GREEN } else { PK_MUTED },
                2,
            );

            let _ = SelectObject(hdc, HGDIOBJ(font_tab.0));
            SetTextColor(hdc, if is_active { PK_TEXT } else { PK_MUTED });
            let mut title_txt: Vec<u16> = tab.title.encode_utf16().collect();
            let mut title_rc = RECT {
                left: tx + 24,
                top: tab_y,
                right: tx + tab_w - 24,
                bottom: tab_y + tab_h,
            };
            let _ = DrawTextW(
                hdc,
                &mut title_txt,
                &mut title_rc,
                DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
            );

            let close_x = tx + tab_w - 22;
            let close_y = tab_y + 6;
            let close_sz = 18;
            if is_close_hover {
                let ch_brush = CreateSolidBrush(COLORREF(0x00EBF0FF));
                let ch_rc = RECT {
                    left: close_x,
                    top: close_y,
                    right: close_x + close_sz,
                    bottom: close_y + close_sz,
                };
                FillRect(hdc, &ch_rc, ch_brush);
                let _ = DeleteObject(HGDIOBJ(ch_brush.0));
            }
            let close_fg = if is_close_hover { PK_RED } else { PK_MUTED };
            crate::ui::draw_x(hdc, close_x + 3, close_y + 3, 12, close_fg, 2);
        }
    }

    // New Tab '+' Button
    let plus_x = if tabs.len() <= 1 {
        rem_start_x
    } else {
        rem_start_x + ((tabs.len() - 1) as i32) * (tab_w + 4)
    };
    let plus_hover = hover_state == 999;
    let plus_bg = if plus_hover { PK_BTN_HOVER } else { PK_INPUT_BG };
    let plus_border = if plus_hover { PK_GREEN } else { PK_CARD_BORDER };
    let plus_fg = if plus_hover { PK_GREEN_DARK } else { PK_MUTED };
    draw_rounded_box(hdc, plus_x, tab_y, 30, tab_h, plus_bg, plus_border, 6);
    crate::ui::draw_plus(hdc, plus_x + 8, tab_y + 8, 14, plus_fg, 2);

    // Right Side of Toolbar
    let mode_y = 5;
    let btn_w = 32;
    let is_home = active_idx == 0;
    let is_settings_open = SHOW_SETTINGS_MODAL.load(std::sync::atomic::Ordering::Relaxed);
    let is_admin = VIEWPORT_IS_ADMIN_MODE.load(std::sync::atomic::Ordering::Relaxed);
    let active_mon = ACTIVE_REMOTE_MONITOR.load(std::sync::atomic::Ordering::Relaxed);
    let is_muted = crate::windows::audio::is_audio_muted();
    let is_voice = crate::windows::audio::is_intercom_voice_active();
    let is_privacy = REMOTE_PRIVACY_MODE.load(std::sync::atomic::Ordering::Relaxed);
    let is_wb = is_whiteboard_active();
    let is_rec = crate::windows::recorder::is_recording();
    let is_zap = is_quick_actions_open();

    let settings_x = client_w - btn_w - 12;
    let s_hover = hover_state == 9006;
    let (s_bg, s_border, s_fg, s_bw) = if is_settings_open {
        (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
    } else if s_hover {
        (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
    } else {
        (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
    };
    draw_tb_button_box(hdc, settings_x, mode_y, btn_w, tab_h, s_bg, s_border, s_bw);
    crate::ui::draw_settings(hdc, settings_x + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, s_fg, 2);

    let (admin_x, desk_x, priv_x, voice_x, audio_x, rec_x, wb_x, zap_x, mon3_x, mon2_x, mon1_x, chat_x) = if is_home {
        let is_online = crate::p2p::is_server_online();
        let ping = crate::p2p::get_server_ping_ms();
        let (pill_text_str, pill_bg, pill_fg) = if is_online {
            let s = if ping > 0 { format!("Online ({}ms)", ping) } else { "Online".to_string() };
            (s, TB_ACTIVE_BG, TB_ACTIVE_BORDER)
        } else {
            ("Offline".to_string(), COLORREF(0x00EBF0FF), COLORREF(0x002B37DF))
        };

        let _ = SelectObject(hdc, HGDIOBJ(font_tab.0));
        SetTextColor(hdc, pill_fg);
        let mut pill_text: Vec<u16> = pill_text_str.encode_utf16().collect();
        let mut pill_sz = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &mut pill_text, &mut pill_sz);

        let icon_sz = 14;
        let gap = 8;
        let pad_x = 12;
        let pill_w = pill_sz.cx + gap + icon_sz + pad_x * 2;
        let pill_x = settings_x - pill_w - 8;
        draw_rounded_box(hdc, pill_x, mode_y, pill_w, tab_h, pill_bg, PK_CARD_BORDER, 6);

        crate::ui::draw_radio(hdc, pill_x + pad_x, mode_y + (tab_h - icon_sz) / 2, icon_sz, pill_fg, 2);
        let mut pill_rect = RECT {
            left: pill_x + pad_x + icon_sz + gap,
            top: mode_y,
            right: pill_x + pill_w - pad_x,
            bottom: mode_y + tab_h,
        };
        let _ = DrawTextW(hdc, &mut pill_text, &mut pill_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

        // INSTALL / PIN TO START & DESKTOP BUTTON (Hit ID 9020)
        let is_installed = crate::windows::install::is_installed();
        let just_installed = JUST_INSTALLED.load(Ordering::Relaxed);
        let btn_label = if just_installed {
            "✓ Installed"
        } else if !is_installed {
            "📌 Pin to Start & Desktop"
        } else {
            "📌 Create Desktop Shortcut"
        };

        let mut inst_txt: Vec<u16> = btn_label.encode_utf16().collect();
        let mut inst_sz = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &mut inst_txt, &mut inst_sz);
        let inst_w = inst_sz.cx + 22;
        let inst_x = pill_x - inst_w - 8;

        if inst_x > (plus_x + 36) {
            INSTALL_BTN_X.store(inst_x, Ordering::Relaxed);
            INSTALL_BTN_W.store(inst_w, Ordering::Relaxed);

            let is_inst_hover = hover_state == 9020;
            let (i_bg, i_border, i_fg) = if just_installed {
                (TB_ACTIVE_BG, TB_ACTIVE_BORDER, PK_GREEN_DARK)
            } else if is_inst_hover {
                (TB_HOVER_BG, PK_GREEN_DARK, PK_GREEN_DARK)
            } else if !is_installed {
                (COLORREF(0x00F0FDF4), PK_GREEN, PK_GREEN_DARK)
            } else {
                (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG)
            };

            draw_rounded_box(hdc, inst_x, mode_y, inst_w, tab_h, i_bg, i_border, 6);
            let _ = SelectObject(hdc, HGDIOBJ(font_tab.0));
            SetTextColor(hdc, i_fg);
            let mut inst_rc = RECT {
                left: inst_x,
                top: mode_y,
                right: inst_x + inst_w,
                bottom: mode_y + tab_h,
            };
            let _ = DrawTextW(hdc, &mut inst_txt, &mut inst_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
        } else {
            INSTALL_BTN_W.store(0, Ordering::Relaxed);
        }

        (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    } else {
        INSTALL_BTN_W.store(0, Ordering::Relaxed);
        let chat_x = settings_x - btn_w - 5;
        let admin_x = chat_x - btn_w - 5;
        let desk_x = admin_x - btn_w - 5;
        let priv_x = desk_x - btn_w - 5;
        let voice_x = priv_x - btn_w - 5;
        let audio_x = voice_x - btn_w - 5;
        let rec_x = audio_x - btn_w - 5;
        let wb_x = rec_x - btn_w - 5;
        let zap_x = wb_x - btn_w - 5;
        let div_x = zap_x - 7;
        let mon3_x = div_x - 7 - btn_w;
        let mon2_x = mon3_x - btn_w - 4;
        let mon1_x = mon2_x - btn_w - 4;

        let div_rc = RECT {
            left: div_x,
            top: mode_y + 4,
            right: div_x + 1,
            bottom: mode_y + tab_h - 4,
        };
        FillRect(hdc, &div_rc, border_brush);

        // Active Codec Badge (Hit 9011)
        let codec_str = crate::p2p::get_active_stream_codec_badge();
        let is_hw = crate::p2p::get_active_stream_codec() == 0x16 || crate::p2p::get_active_stream_codec() == 0x17;
        let pill_w = 80;
        let pill_x = mon1_x - pill_w - 8;
        let (c_bg, c_border, c_fg, c_bw) = if hover_state == 9011 {
            (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
        } else if is_hw {
            (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 1)
        } else {
            (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
        };
        draw_tb_button_box(hdc, pill_x, mode_y, pill_w, tab_h, c_bg, c_border, c_bw);
        let _ = SelectObject(hdc, HGDIOBJ(font_tab.0));
        SetTextColor(hdc, c_fg);
        let mut codec_txt: Vec<u16> = codec_str.encode_utf16().collect();
        let mut codec_rc = RECT {
            left: pill_x,
            top: mode_y,
            right: pill_x + pill_w,
            bottom: mode_y + tab_h,
        };
        let _ = DrawTextW(hdc, &mut codec_txt, &mut codec_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

        // Mon 1, 2, 3
        for (idx, mx, hid) in [(0u32, mon1_x, 9101), (1, mon2_x, 9102), (2, mon3_x, 9103)] {
            let m_hover = hover_state == hid;
            let (m_bg, m_border, m_fg, m_bw) = if active_mon == idx {
                (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
            } else if m_hover {
                (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
            } else {
                (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
            };
            draw_tb_button_box(hdc, mx, mode_y, btn_w, tab_h, m_bg, m_border, m_bw);
            crate::ui::draw_monitor_num(hdc, mx + (btn_w - 18) / 2, mode_y + (tab_h - 18) / 2, 18, idx + 1, m_fg, 2);
        }

        // Quick Actions Menu (Zap 9007)
        let zap_hover = hover_state == 9007;
        let (z_bg, z_border, z_fg, z_bw) = if is_zap {
            (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
        } else if zap_hover {
            (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
        } else {
            (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
        };
        draw_tb_button_box(hdc, zap_x, mode_y, btn_w, tab_h, z_bg, z_border, z_bw);
        crate::ui::draw_zap(hdc, zap_x + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, z_fg, 2);

        // Whiteboard Pen (Pen 9008)
        let wb_hover = hover_state == 9008;
        let (wb_bg, wb_border, wb_fg, wb_bw) = if is_wb {
            (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
        } else if wb_hover {
            (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
        } else {
            (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
        };
        draw_tb_button_box(hdc, wb_x, mode_y, btn_w, tab_h, wb_bg, wb_border, wb_bw);
        crate::ui::draw_pen(hdc, wb_x + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, wb_fg, 2);

        // Session Recording (Record 9009)
        let rec_hover = hover_state == 9009;
        let (r_bg, r_border, r_fg, r_bw) = if is_rec {
            (TB_REC_BG, TB_REC_BORDER, TB_REC_FG, 2)
        } else if rec_hover {
            (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
        } else {
            (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
        };
        draw_tb_button_box(hdc, rec_x, mode_y, btn_w, tab_h, r_bg, r_border, r_bw);
        crate::ui::draw_record(hdc, rec_x + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, r_fg, 2, is_rec);

        // Audio Mute/Unmute (9003)
        let a_hover = hover_state == 9003;
        let (a_bg, a_border, a_fg, a_bw) = if !is_muted {
            // Audio ACTIVE: Streaming remote audio
            (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
        } else if a_hover {
            (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
        } else {
            // Audio MUTED / PASSIVE: Soft red alert
            (TB_MUTED_BG, TB_MUTED_BORDER, TB_MUTED_FG, 1)
        };
        draw_tb_button_box(hdc, audio_x, mode_y, btn_w, tab_h, a_bg, a_border, a_bw);
        if is_muted {
            crate::ui::draw_volume_x(hdc, audio_x + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, a_fg, 2);
        } else {
            crate::ui::draw_volume(hdc, audio_x + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, a_fg, 2);
        }

        // Voice Intercom (9005)
        let vox_hover = hover_state == 9005;
        let (vox_bg, vox_border, vox_fg, vox_bw) = if is_voice {
            // Microphone ACTIVE: Transmitting
            (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
        } else if vox_hover {
            (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
        } else {
            (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
        };
        draw_tb_button_box(hdc, voice_x, mode_y, btn_w, tab_h, vox_bg, vox_border, vox_bw);
        if is_voice {
            crate::ui::draw_mic(hdc, voice_x + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, vox_fg, 2);
        } else {
            crate::ui::draw_mic_off(hdc, voice_x + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, vox_fg, 2);
        }

        // Privacy Lock (9004)
        let priv_hover = hover_state == 9004;
        let (p_bg, p_border, p_fg, p_bw) = if is_privacy {
            // Screen Blackout / Input Lock ACTIVE: Amber security highlight
            (TB_PRIV_BG, TB_PRIV_BORDER, TB_PRIV_FG, 2)
        } else if priv_hover {
            (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
        } else {
            (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
        };
        draw_tb_button_box(hdc, priv_x, mode_y, btn_w, tab_h, p_bg, p_border, p_bw);
        crate::ui::draw_lock(hdc, priv_x + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, p_fg, 2);

        // Desktop View (9001)
        let desk_hover = hover_state == 9001;
        let (d_bg, d_border, d_fg, d_bw) = if !is_admin {
            (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
        } else if desk_hover {
            (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
        } else {
            (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
        };
        draw_tb_button_box(hdc, desk_x, mode_y, btn_w, tab_h, d_bg, d_border, d_bw);
        crate::ui::draw_monitor(hdc, desk_x + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, d_fg, 2);

        // Admin Terminal (9002)
        let admin_hover = hover_state == 9002;
        let (ad_bg, ad_border, ad_fg, ad_bw) = if is_admin {
            (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
        } else if admin_hover {
            (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
        } else {
            (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
        };
        draw_tb_button_box(hdc, admin_x, mode_y, btn_w, tab_h, ad_bg, ad_border, ad_bw);
        crate::ui::draw_terminal(hdc, admin_x + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, ad_fg, 2);

        // In-Session Floating Chat (9010)
        let is_chat = crate::p2p::is_chat_open();
        let unread = crate::p2p::get_unread_count();
        let chat_hover = hover_state == 9010;
        let (c_bg, c_border, c_fg, c_bw) = if is_chat {
            (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
        } else if chat_hover {
            (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
        } else {
            (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
        };
        draw_tb_button_box(hdc, chat_x, mode_y, btn_w, tab_h, c_bg, c_border, c_bw);
        crate::ui::draw_message_square(hdc, chat_x + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, c_fg, 2);
        if unread > 0 {
            let badge_brush = CreateSolidBrush(PK_RED);
            let badge_rc = RECT {
                left: chat_x + btn_w - 9,
                top: mode_y + 3,
                right: chat_x + btn_w - 3,
                bottom: mode_y + 9,
            };
            FillRect(hdc, &badge_rc, badge_brush);
            let _ = DeleteObject(HGDIOBJ(badge_brush.0));
        }

        (admin_x, desk_x, priv_x, voice_x, audio_x, rec_x, wb_x, zap_x, mon3_x, mon2_x, mon1_x, chat_x)
    };

    let tooltip_data: Option<(&str, i32)> = match hover_state {
        9020 => Some((
            if JUST_INSTALLED.load(Ordering::Relaxed) {
                "Desky is installed & integrated into Windows Start Search"
            } else if !crate::windows::install::is_installed() {
                "Install Desky to Start Menu, Desktop & Windows Search"
            } else {
                "Create shortcut on Windows Desktop"
            },
            INSTALL_BTN_X.load(Ordering::Relaxed) + INSTALL_BTN_W.load(Ordering::Relaxed) / 2,
        )),
        9011 if !is_home => Some((crate::p2p::get_active_stream_codec_name(), mon1_x - 48)),
        9006 => Some((if is_settings_open { "Close Settings" } else { "Settings" }, settings_x + btn_w / 2)),
        9010 if !is_home => Some((if crate::p2p::is_chat_open() { "Close Chat" } else { "In-Session Chat" }, chat_x + btn_w / 2)),
        9002 if !is_home => Some((if is_admin { "Admin Terminal: Active" } else { "Open Remote Admin Terminal" }, admin_x + btn_w / 2)),
        9001 if !is_home => Some((if !is_admin { "Desktop View: Active" } else { "Switch to Desktop View" }, desk_x + btn_w / 2)),
        9004 if !is_home => Some((if is_privacy { "Privacy Mode: ACTIVE (Screen Blacked Out)" } else { "Privacy Mode: OFF (Blackout Screen)" }, priv_x + btn_w / 2)),
        9005 if !is_home => Some((if is_voice { "Microphone: ACTIVE (Click to Mute)" } else { "Microphone: Muted (Click to Talk)" }, voice_x + btn_w / 2)),
        9003 if !is_home => Some((if !is_muted { "Audio: ACTIVE (Streaming - Click to Mute)" } else { "Audio: MUTED (Click to Unmute)" }, audio_x + btn_w / 2)),
        9009 if !is_home => Some((if is_rec { "Recording: ACTIVE (Click to Stop)" } else { "Start Screen Recording" }, rec_x + btn_w / 2)),
        9008 if !is_home => Some((if is_wb { "Whiteboard: ACTIVE (Click to Close)" } else { "Whiteboard: OFF (Click to Open)" }, wb_x + btn_w / 2)),
        9007 if !is_home => Some(("Quick Actions (Ctrl+Alt+Del, Win+L...)", zap_x + btn_w / 2)),
        9103 if !is_home => Some((if active_mon == 2 { "Monitor 3: Active" } else { "Switch to Monitor 3" }, mon3_x + btn_w / 2)),
        9102 if !is_home => Some((if active_mon == 1 { "Monitor 2: Active" } else { "Switch to Monitor 2" }, mon2_x + btn_w / 2)),
        9101 if !is_home => Some((if active_mon == 0 { "Monitor 1: Active" } else { "Switch to Monitor 1" }, mon1_x + btn_w / 2)),
        _ => None,
    };

    if let Some((text, center_x)) = tooltip_data {
        draw_toolbar_tooltip(hdc, client_w, tab_h + mode_y, text, center_x);
    }

    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(HGDIOBJ(border_brush.0));
    let _ = DeleteObject(HGDIOBJ(font_brand.0));
    let _ = DeleteObject(HGDIOBJ(font_tab.0));
}

pub fn get_tab_hit_state(hwnd: HWND, client_x: i32, client_y: i32) -> i32 {
    let mut rc = RECT::default();
    unsafe { let _ = GetClientRect(hwnd, &mut rc); }
    let client_w = rc.right - rc.left;
    if client_y < 0 || client_y >= TOOLBAR_HEIGHT {
        return -1;
    }
    let tabs = super::get_session_tabs();
    let brand_w = 92;
    if client_x < brand_w {
        return 0;
    }

    let tab_0_w = 96;
    let start_x = brand_w + 8;

    if client_x >= start_x && client_x < start_x + tab_0_w {
        return 0;
    }

    let rem_start_x = start_x + tab_0_w + 4;
    let rem_count = (tabs.len().saturating_sub(1)).max(1) as i32;
    let tab_w = 175.min(((client_w - rem_start_x - 300) / rem_count).max(110));

    for (i, _) in tabs.iter().enumerate().skip(1) {
        let tx = rem_start_x + ((i - 1) as i32) * (tab_w + 4);
        if client_x >= tx && client_x < tx + tab_w {
            let close_x = tx + tab_w - 24;
            let close_sz = 20;
            if client_x >= close_x && client_x < close_x + close_sz {
                return 1000 + i as i32;
            }
            return i as i32;
        }
    }

    let plus_x = if tabs.len() <= 1 {
        rem_start_x
    } else {
        rem_start_x + ((tabs.len() - 1) as i32) * (tab_w + 4)
    };
    if client_x >= plus_x && client_x < plus_x + 32 {
        return 999;
    }

    let btn_w = 32;
    let settings_x = client_w - btn_w - 12;
    let tab_y = 5;
    let tab_h = 30;
    if client_y >= tab_y && client_y < tab_y + tab_h {
        if client_x >= settings_x && client_x < settings_x + btn_w {
            return 9006;
        }
    }

    let active_idx = ACTIVE_TAB_INDEX.load(std::sync::atomic::Ordering::Relaxed);
    if active_idx == 0 {
        let inst_w = INSTALL_BTN_W.load(Ordering::Relaxed);
        if inst_w > 0 && client_y >= tab_y && client_y < tab_y + tab_h {
            let inst_x = INSTALL_BTN_X.load(Ordering::Relaxed);
            if client_x >= inst_x && client_x < inst_x + inst_w {
                return 9020;
            }
        }
    } else {
        let chat_x = settings_x - btn_w - 5;
        let admin_x = chat_x - btn_w - 5;
        let desk_x = admin_x - btn_w - 5;
        let priv_x = desk_x - btn_w - 5;
        let voice_x = priv_x - btn_w - 5;
        let audio_x = voice_x - btn_w - 5;
        let rec_x = audio_x - btn_w - 5;
        let wb_x = rec_x - btn_w - 5;
        let zap_x = wb_x - btn_w - 5;
        let div_x = zap_x - 7;
        let mon3_x = div_x - 7 - btn_w;
        let mon2_x = mon3_x - btn_w - 4;
        let mon1_x = mon2_x - btn_w - 4;

        if client_y >= tab_y && client_y < tab_y + tab_h {
            if client_x >= chat_x && client_x < chat_x + btn_w { return 9010; }
            if client_x >= desk_x && client_x < desk_x + btn_w { return 9001; }
            if client_x >= admin_x && client_x < admin_x + btn_w { return 9002; }
            if client_x >= priv_x && client_x < priv_x + btn_w { return 9004; }
            if client_x >= voice_x && client_x < voice_x + btn_w { return 9005; }
            if client_x >= audio_x && client_x < audio_x + btn_w { return 9003; }
            if client_x >= rec_x && client_x < rec_x + btn_w { return 9009; }
            if client_x >= wb_x && client_x < wb_x + btn_w { return 9008; }
            if client_x >= zap_x && client_x < zap_x + btn_w { return 9007; }
            let pill_w = 80;
            let pill_x = mon1_x - pill_w - 8;
            if client_x >= pill_x && client_x < pill_x + pill_w { return 9011; }
            if client_x >= mon1_x && client_x < mon1_x + btn_w { return 9101; }
            if client_x >= mon2_x && client_x < mon2_x + btn_w { return 9102; }
            if client_x >= mon3_x && client_x < mon3_x + btn_w { return 9103; }
        }
    }

    -1
}
