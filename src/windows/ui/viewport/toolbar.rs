//! Viewport Top Tab Bar & Responsive Interactive Toolbar
//! Renders session tabs, dynamic responsive collapsing, vector buttons, and floating tooltips.

#![allow(unsafe_op_in_unsafe_fn)]

use std::sync::atomic::{AtomicI32, Ordering};
use windows::Win32::Foundation::{COLORREF, HWND, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreatePen, CreateRectRgn, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetDC,
    GetTextExtentPoint32W, Rectangle, ReleaseDC, SelectClipRgn, SelectObject, SetBkMode,
    SetTextColor, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER,
    FW_BOLD, FW_SEMIBOLD, HBRUSH, HDC, HGDIOBJ, PS_SOLID, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

use super::overflow_menu::is_overflow_menu_open;
use super::quick_actions::is_quick_actions_open;
use super::settings_modal::SHOW_SETTINGS_MODAL;
use super::theme::{
    create_segoe_font, draw_rounded_box, PK_BG, PK_BTN_HOVER, PK_CARD, PK_CARD_BORDER, PK_GREEN,
    PK_GREEN_DARK, PK_INPUT_BG, PK_MUTED, PK_RED, PK_TEXT,
};
use super::whiteboard::is_whiteboard_active;
use super::{
    SessionTab, ACTIVE_REMOTE_MONITOR, ACTIVE_TAB_INDEX, REMOTE_PRIVACY_MODE,
    VIEWPORT_IS_ADMIN_MODE,
};

pub const TOOLBAR_HEIGHT: i32 = 40;
pub static TAB_HOVER_STATE: AtomicI32 = AtomicI32::new(-1);
pub static TAB_SCROLL_OFFSET: AtomicI32 = AtomicI32::new(0);

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
        PK_CARD,
        PK_CARD_BORDER,
        6,
    );

    SetTextColor(hdc, PK_TEXT);
    let mut tip_rc = RECT {
        left: tip_x,
        top: tip_y,
        right: tip_x + tip_w,
        bottom: tip_y + tip_h,
    };
    let _ = DrawTextW(hdc, &mut tip_txt, &mut tip_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_NOPREFIX);

    let _ = SelectObject(hdc, old_f);
    let _ = DeleteObject(HGDIOBJ(font_tip.0));
}

#[derive(Clone, Debug)]
pub struct TabGeometry {
    pub index: usize,
    pub x: i32,
    pub w: i32,
    pub close_x: i32,
    pub close_w: i32,
}

#[derive(Clone, Debug)]
pub struct ToolbarLayout {
    pub brand_w: i32,
    pub home_x: i32,
    pub home_w: i32,
    pub home_is_compact: bool,

    pub tabs_area_x: i32,
    pub session_tabs: Vec<TabGeometry>,
    pub plus_x: i32,
    pub plus_w: i32,

    pub has_scroll_controls: bool,
    pub scroll_left_x: i32,
    pub scroll_right_x: i32,

    pub settings_x: i32,
    pub chat_x: Option<i32>,
    pub admin_x: Option<i32>,
    pub desk_x: Option<i32>,
    pub priv_x: Option<i32>,
    pub voice_x: Option<i32>,
    pub audio_x: Option<i32>,
    pub rec_x: Option<i32>,
    pub wb_x: Option<i32>,
    pub zap_x: Option<i32>,
    pub more_x: Option<i32>,

    pub is_mon_collapsed: bool,
    pub mon1_x: Option<i32>,
    pub mon2_x: Option<i32>,
    pub mon3_x: Option<i32>,
    pub div_x: Option<i32>,

    pub codec_x: Option<i32>,
    pub codec_w: i32,

    pub home_pill_x: Option<i32>,
    pub home_pill_w: i32,

    pub right_buttons_start_x: i32,
}

fn get_home_pill_label() -> (String, bool) {
    let is_online = crate::p2p::is_server_online();
    if is_online {
        let ping = crate::p2p::get_server_ping_ms();
        let s = if ping > 0 {
            format!("Online ({}ms)", ping)
        } else {
            "Online".to_string()
        };
        (s, true)
    } else {
        ("Offline".to_string(), false)
    }
}

fn measure_home_pill_width(text: &str) -> i32 {
    let icon_sz = 14;
    let gap = 8;
    let pad_x = 12;
    let text_w = unsafe {
        let hdc = GetDC(None);
        if !hdc.is_invalid() {
            let font = create_segoe_font(12, FW_SEMIBOLD);
            let old = SelectObject(hdc, HGDIOBJ(font.0));
            let wide: Vec<u16> = text.encode_utf16().collect();
            let mut sz = SIZE::default();
            let ok = GetTextExtentPoint32W(hdc, &wide, &mut sz).as_bool();
            let _ = SelectObject(hdc, old);
            let _ = DeleteObject(HGDIOBJ(font.0));
            let _ = ReleaseDC(None, hdc);
            if ok {
                sz.cx
            } else {
                (text.len() as i32) * 7
            }
        } else {
            (text.len() as i32) * 7
        }
    };
    (pad_x * 2 + icon_sz + gap + text_w + 2).max(88)
}

impl ToolbarLayout {
    pub fn compute(client_w: i32, num_tabs: usize, active_idx: usize) -> Self {
        let btn_w = 32;
        let btn_gap = 4;
        let is_home = active_idx == 0;

        let brand_w = if client_w < 700 { 76 } else { 92 };
        let start_x = brand_w + 6;

        let settings_x = client_w - btn_w - 12;

        let mut chat_x = None;
        let mut admin_x = None;
        let mut desk_x = None;
        let mut priv_x = None;
        let mut voice_x = None;
        let mut audio_x = None;
        let mut rec_x = None;
        let mut wb_x = None;
        let mut zap_x = None;
        let mut more_x = None;

        let mut is_mon_collapsed = false;
        let mut mon1_x = None;
        let mut mon2_x = None;
        let mut mon3_x = None;
        let mut div_x = None;

        let mut codec_x = None;
        let codec_w = 80;

        let mut home_pill_x = None;
        let mut home_pill_w = 0;

        let right_buttons_start_x;

        if is_home {
            let (pill_text, _) = get_home_pill_label();
            home_pill_w = measure_home_pill_width(&pill_text);
            let px = settings_x - home_pill_w - 8;
            home_pill_x = Some(px);
            right_buttons_start_x = px;
        } else {
            // Calculate minimum width required by left tabs:
            // Home (34) + 1 remote tab min (70) + plus (28) = ~140px + brand_w
            let min_left_w = start_x + 34 + 4 + 70 + 4 + 28 + 12;
            let avail_for_right = client_w - min_left_w - 12;

            let show_codec = client_w >= 1000 && avail_for_right >= 530;
            let show_separate_mon = client_w >= 820 && avail_for_right >= 440;
            let use_overflow = !show_separate_mon && (client_w < 680 || avail_for_right < 380);

            let mut cur_x = settings_x - btn_w - btn_gap;
            chat_x = Some(cur_x);

            if !use_overflow {
                cur_x -= btn_w + btn_gap; admin_x = Some(cur_x);
                cur_x -= btn_w + btn_gap; desk_x = Some(cur_x);
                cur_x -= btn_w + btn_gap; priv_x = Some(cur_x);
                cur_x -= btn_w + btn_gap; voice_x = Some(cur_x);
                cur_x -= btn_w + btn_gap; audio_x = Some(cur_x);
                cur_x -= btn_w + btn_gap; rec_x = Some(cur_x);
                cur_x -= btn_w + btn_gap; wb_x = Some(cur_x);
                cur_x -= btn_w + btn_gap; zap_x = Some(cur_x);
            } else {
                cur_x -= btn_w + btn_gap; audio_x = Some(cur_x);
                cur_x -= btn_w + btn_gap; more_x = Some(cur_x);
                cur_x -= btn_w + btn_gap; zap_x = Some(cur_x);
            }

            cur_x -= 6;
            div_x = Some(cur_x);
            cur_x -= 6;

            if show_separate_mon {
                cur_x -= btn_w; mon3_x = Some(cur_x);
                cur_x -= btn_w + btn_gap; mon2_x = Some(cur_x);
                cur_x -= btn_w + btn_gap; mon1_x = Some(cur_x);
                is_mon_collapsed = false;
            } else {
                cur_x -= btn_w; mon1_x = Some(cur_x);
                is_mon_collapsed = true;
            }

            if show_codec {
                cur_x -= codec_w + 6;
                codec_x = Some(cur_x);
            }

            right_buttons_start_x = cur_x;
        }

        // Calculate Left Tabs Area
        let max_tabs_end_x = right_buttons_start_x - 8;
        let avail_tabs_w = (max_tabs_end_x - start_x).max(60);

        let home_is_compact = client_w < 920 || avail_tabs_w < 350;
        let home_w = if home_is_compact { 34 } else { 90 };
        let home_x = start_x;

        let rem_count = num_tabs.saturating_sub(1);
        let plus_w = 28;

        let tabs_area_x = home_x + home_w + 4;
        let tabs_area_w = (max_tabs_end_x - tabs_area_x).max(20);

        let mut session_tabs = Vec::new();
        let has_scroll_controls;
        let mut scroll_left_x = 0;
        let mut scroll_right_x = 0;

        if rem_count == 0 {
            has_scroll_controls = false;
        } else {
            let ideal_tab_w = 175;
            let needed_w = (rem_count as i32) * (ideal_tab_w + 4) + plus_w + 4;

            if needed_w <= tabs_area_w {
                has_scroll_controls = false;
                for i in 1..num_tabs {
                    let tx = tabs_area_x + ((i - 1) as i32) * (ideal_tab_w + 4);
                    session_tabs.push(TabGeometry {
                        index: i,
                        x: tx,
                        w: ideal_tab_w,
                        close_x: tx + ideal_tab_w - 22,
                        close_w: 18,
                    });
                }
            } else {
                let dynamic_tab_w = (((tabs_area_w - plus_w - 4) / rem_count as i32) - 4).max(64);
                let dynamic_needed_w = (rem_count as i32) * (dynamic_tab_w + 4) + plus_w + 4;

                if dynamic_needed_w <= tabs_area_w {
                    has_scroll_controls = false;
                    for i in 1..num_tabs {
                        let tx = tabs_area_x + ((i - 1) as i32) * (dynamic_tab_w + 4);
                        session_tabs.push(TabGeometry {
                            index: i,
                            x: tx,
                            w: dynamic_tab_w,
                            close_x: tx + dynamic_tab_w - 20,
                            close_w: 18,
                        });
                    }
                } else {
                    // Requires scroll controls
                    has_scroll_controls = true;
                    let scroll_btn_w = 16;
                    scroll_right_x = max_tabs_end_x - scroll_btn_w;
                    scroll_left_x = scroll_right_x - scroll_btn_w - 2;

                    let tab_w = 110;
                    let scroll_offset = TAB_SCROLL_OFFSET.load(Ordering::Relaxed);
                    for i in 1..num_tabs {
                        let tx = tabs_area_x + scroll_offset + ((i - 1) as i32) * (tab_w + 4);
                        session_tabs.push(TabGeometry {
                            index: i,
                            x: tx,
                            w: tab_w,
                            close_x: tx + tab_w - 20,
                            close_w: 18,
                        });
                    }
                }
            }
        }

        let plus_x = if rem_count == 0 {
            tabs_area_x
        } else if let Some(last_tab) = session_tabs.last() {
            last_tab.x + last_tab.w + 4
        } else {
            tabs_area_x
        };

        ToolbarLayout {
            brand_w,
            home_x,
            home_w,
            home_is_compact,
            tabs_area_x,
            session_tabs,
            plus_x,
            plus_w,
            has_scroll_controls,
            scroll_left_x,
            scroll_right_x,
            settings_x,
            chat_x,
            admin_x,
            desk_x,
            priv_x,
            voice_x,
            audio_x,
            rec_x,
            wb_x,
            zap_x,
            more_x,
            is_mon_collapsed,
            mon1_x,
            mon2_x,
            mon3_x,
            div_x,
            codec_x,
            codec_w,
            home_pill_x,
            home_pill_w,
            right_buttons_start_x,
        }
    }
}

pub fn get_zap_x(client_w: i32, num_tabs: usize, active_idx: usize) -> i32 {
    let layout = ToolbarLayout::compute(client_w, num_tabs, active_idx);
    layout.zap_x.unwrap_or(layout.settings_x - (32 + 4) * 3)
}

pub fn get_more_x(client_w: i32, num_tabs: usize, active_idx: usize) -> Option<i32> {
    let layout = ToolbarLayout::compute(client_w, num_tabs, active_idx);
    layout.more_x
}

pub fn scroll_tabs(delta: i32) {
    let cur = TAB_SCROLL_OFFSET.load(Ordering::Relaxed);
    let step = if delta > 0 { 60 } else { -60 };
    let next = (cur + step).min(0);
    TAB_SCROLL_OFFSET.store(next, Ordering::Relaxed);
}

pub unsafe fn draw_tab_bar(
    hdc: HDC,
    client_w: i32,
    tab_bar_h: i32,
    tabs: &[SessionTab],
    active_idx: usize,
    hover_state: i32,
) {
    let layout = ToolbarLayout::compute(client_w, tabs.len(), active_idx);

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
    let div1 = RECT {
        left: layout.brand_w,
        top: 0,
        right: layout.brand_w + 1,
        bottom: tab_bar_h,
    };
    FillRect(hdc, &div1, border_brush);

    let font_brand = create_segoe_font(13, FW_BOLD);
    let old_font = SelectObject(hdc, HGDIOBJ(font_brand.0));
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, PK_GREEN_DARK);
    let mut brand_txt: Vec<u16> = "DESKY".encode_utf16().collect();
    let mut brand_rc = RECT {
        left: 0,
        top: 0,
        right: layout.brand_w,
        bottom: tab_bar_h,
    };
    let _ = DrawTextW(hdc, &mut brand_txt, &mut brand_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    let font_tab = create_segoe_font(12, FW_SEMIBOLD);
    let tab_h = 30;
    let tab_y = 5;

    // Home Tab
    let is_home_active = active_idx == 0;
    let is_home_hover = hover_state == 0;
    let home_bg = if is_home_active {
        PK_CARD
    } else if is_home_hover {
        PK_BTN_HOVER
    } else {
        PK_INPUT_BG
    };
    let home_border = if is_home_active { PK_GREEN } else { PK_CARD_BORDER };

    draw_rounded_box(hdc, layout.home_x, tab_y, layout.home_w, tab_h, home_bg, home_border, 6);

    if is_home_active {
        let accent_brush = CreateSolidBrush(PK_GREEN);
        let accent_rc = RECT {
            left: layout.home_x + 6,
            top: tab_y + tab_h - 3,
            right: layout.home_x + layout.home_w - 6,
            bottom: tab_y + tab_h - 1,
        };
        FillRect(hdc, &accent_rc, accent_brush);
        let _ = DeleteObject(HGDIOBJ(accent_brush.0));
    }

    let home_icon_x = if layout.home_is_compact {
        layout.home_x + (layout.home_w - 14) / 2
    } else {
        layout.home_x + 10
    };

    crate::ui::draw_home(
        hdc,
        home_icon_x,
        tab_y + (tab_h - 14) / 2,
        14,
        if is_home_active { PK_GREEN_DARK } else { PK_MUTED },
        2,
    );

    if !layout.home_is_compact {
        let _ = SelectObject(hdc, HGDIOBJ(font_tab.0));
        SetTextColor(hdc, if is_home_active { PK_GREEN_DARK } else { PK_MUTED });
        let mut title_txt: Vec<u16> = "Home".encode_utf16().collect();
        let mut title_rc = RECT {
            left: layout.home_x + 28,
            top: tab_y,
            right: layout.home_x + layout.home_w - 6,
            bottom: tab_y + tab_h,
        };
        let _ = DrawTextW(hdc, &mut title_txt, &mut title_rc, DT_SINGLELINE | DT_VCENTER);
    }

    // Remote Session Tabs with GDI Clipping to prevent ANY spill into right toolbar
    let clip_end_x = if layout.has_scroll_controls {
        layout.scroll_left_x - 4
    } else {
        layout.right_buttons_start_x - 6
    };

    let clip_rgn = CreateRectRgn(layout.tabs_area_x, 0, clip_end_x.max(layout.tabs_area_x + 10), tab_bar_h);
    let _ = SelectClipRgn(hdc, Some(clip_rgn));

    for tab_geom in &layout.session_tabs {
        let i = tab_geom.index;
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

        draw_rounded_box(hdc, tab_geom.x, tab_y, tab_geom.w, tab_h, tab_bg, tab_border, 6);

        if is_active {
            let accent_brush = CreateSolidBrush(PK_GREEN);
            let accent_rc = RECT {
                left: tab_geom.x + 6,
                top: tab_y + tab_h - 3,
                right: tab_geom.x + tab_geom.w - 6,
                bottom: tab_y + tab_h - 1,
            };
            FillRect(hdc, &accent_rc, accent_brush);
            let _ = DeleteObject(HGDIOBJ(accent_brush.0));
        }

        let radio_fg = if is_active { PK_GREEN } else { PK_MUTED };
        crate::ui::draw_radio(hdc, tab_geom.x + 8, tab_y + (tab_h - 12) / 2, 12, radio_fg, 2);

        let _ = SelectObject(hdc, HGDIOBJ(font_tab.0));
        SetTextColor(hdc, if is_active { PK_TEXT } else { PK_MUTED });
        let title_str = tabs.get(i).map(|t| t.title.as_str()).unwrap_or("Session");
        let mut title_txt: Vec<u16> = title_str.encode_utf16().collect();
        let mut title_rc = RECT {
            left: tab_geom.x + 24,
            top: tab_y,
            right: tab_geom.close_x - 4,
            bottom: tab_y + tab_h,
        };
        let _ = DrawTextW(
            hdc,
            &mut title_txt,
            &mut title_rc,
            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
        );

        let is_close_hover = hover_state == 1000 + i as i32;
        if is_close_hover {
            let ch_brush = CreateSolidBrush(COLORREF(0x00EBF0FF));
            let ch_rc = RECT {
                left: tab_geom.close_x,
                top: tab_y + 6,
                right: tab_geom.close_x + tab_geom.close_w,
                bottom: tab_y + 6 + tab_geom.close_w,
            };
            FillRect(hdc, &ch_rc, ch_brush);
            let _ = DeleteObject(HGDIOBJ(ch_brush.0));
        }
        let close_fg = if is_close_hover { PK_RED } else { PK_MUTED };
        crate::ui::draw_x(hdc, tab_geom.close_x + 3, tab_y + 9, 12, close_fg, 2);
    }

    // New Tab '+' Button
    let plus_hover = hover_state == 999;
    let plus_bg = if plus_hover { PK_BTN_HOVER } else { PK_INPUT_BG };
    let plus_border = if plus_hover { PK_GREEN } else { PK_CARD_BORDER };
    let plus_fg = if plus_hover { PK_GREEN_DARK } else { PK_MUTED };
    draw_rounded_box(hdc, layout.plus_x, tab_y, layout.plus_w, tab_h, plus_bg, plus_border, 6);
    crate::ui::draw_plus(hdc, layout.plus_x + 7, tab_y + 8, 14, plus_fg, 2);

    // Remove clipping
    let _ = SelectClipRgn(hdc, None);
    let _ = DeleteObject(HGDIOBJ(clip_rgn.0));

    // Draw Tab Scroll Controls if active
    if layout.has_scroll_controls {
        let scroll_l_hover = hover_state == 997;
        let l_bg = if scroll_l_hover { PK_BTN_HOVER } else { PK_INPUT_BG };
        let l_fg = if scroll_l_hover { PK_GREEN_DARK } else { PK_MUTED };
        draw_rounded_box(hdc, layout.scroll_left_x, tab_y + 4, 16, tab_h - 8, l_bg, PK_CARD_BORDER, 4);
        crate::ui::draw_chevron_left(hdc, layout.scroll_left_x + 2, tab_y + 8, 12, l_fg, 2);

        let scroll_r_hover = hover_state == 998;
        let r_bg = if scroll_r_hover { PK_BTN_HOVER } else { PK_INPUT_BG };
        let r_fg = if scroll_r_hover { PK_GREEN_DARK } else { PK_MUTED };
        draw_rounded_box(hdc, layout.scroll_right_x, tab_y + 4, 16, tab_h - 8, r_bg, PK_CARD_BORDER, 4);
        crate::ui::draw_chevron_right(hdc, layout.scroll_right_x + 2, tab_y + 8, 12, r_fg, 2);
    }

    // Right Side Toolbar Buttons
    let mode_y = 5;
    let btn_w = 32;

    let is_home = active_idx == 0;
    let is_settings_open = SHOW_SETTINGS_MODAL.load(Ordering::Relaxed);
    let is_admin = VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed);
    let active_mon = ACTIVE_REMOTE_MONITOR.load(Ordering::Relaxed);
    let is_muted = crate::windows::audio::is_audio_muted();
    let is_voice = crate::windows::audio::is_intercom_voice_active();
    let is_privacy = REMOTE_PRIVACY_MODE.load(Ordering::Relaxed);
    let is_wb = is_whiteboard_active();
    let is_rec = crate::windows::recorder::is_recording();
    let is_zap = is_quick_actions_open();
    let is_overflow = is_overflow_menu_open();

    // Settings (9006)
    let s_hover = hover_state == 9006;
    let (s_bg, s_border, s_fg, s_bw) = if is_settings_open {
        (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
    } else if s_hover {
        (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
    } else {
        (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
    };
    draw_tb_button_box(hdc, layout.settings_x, mode_y, btn_w, tab_h, s_bg, s_border, s_bw);
    crate::ui::draw_settings(hdc, layout.settings_x + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, s_fg, 2);

    if is_home {
        if let Some(pill_x) = layout.home_pill_x {
            let (pill_text_str, is_online) = get_home_pill_label();
            let (pill_bg, pill_fg) = if is_online {
                (TB_ACTIVE_BG, TB_ACTIVE_BORDER)
            } else {
                (COLORREF(0x00EBF0FF), COLORREF(0x002B37DF))
            };

            let _ = SelectObject(hdc, HGDIOBJ(font_tab.0));
            SetTextColor(hdc, pill_fg);
            let mut pill_text: Vec<u16> = pill_text_str.encode_utf16().collect();

            let icon_sz = 14;
            let gap = 8;
            let pad_x = 12;
            let pill_w = layout.home_pill_w;
            draw_rounded_box(hdc, pill_x, mode_y, pill_w, tab_h, pill_bg, PK_CARD_BORDER, 6);

            crate::ui::draw_radio(hdc, pill_x + pad_x, mode_y + (tab_h - icon_sz) / 2, icon_sz, pill_fg, 2);
            let mut pill_rect = RECT {
                left: pill_x + pad_x + icon_sz + gap,
                top: mode_y,
                right: pill_x + pill_w - pad_x,
                bottom: mode_y + tab_h,
            };
            let _ = DrawTextW(hdc, &mut pill_text, &mut pill_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
        }
    } else {
        // Chat (9010)
        if let Some(cx) = layout.chat_x {
            let is_chat = crate::p2p::is_chat_open();
            let unread = crate::p2p::get_unread_count();
            let c_hover = hover_state == 9010;
            let (c_bg, c_border, c_fg, c_bw) = if is_chat {
                (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
            } else if c_hover {
                (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
            } else {
                (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
            };
            draw_tb_button_box(hdc, cx, mode_y, btn_w, tab_h, c_bg, c_border, c_bw);
            crate::ui::draw_message_square(hdc, cx + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, c_fg, 2);
            if unread > 0 {
                let badge_brush = CreateSolidBrush(PK_RED);
                let badge_rc = RECT {
                    left: cx + btn_w - 9,
                    top: mode_y + 3,
                    right: cx + btn_w - 3,
                    bottom: mode_y + 9,
                };
                FillRect(hdc, &badge_rc, badge_brush);
                let _ = DeleteObject(HGDIOBJ(badge_brush.0));
            }
        }

        // Admin Terminal (9002)
        if let Some(ax) = layout.admin_x {
            let ad_hover = hover_state == 9002;
            let (ad_bg, ad_border, ad_fg, ad_bw) = if is_admin {
                (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
            } else if ad_hover {
                (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
            } else {
                (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
            };
            draw_tb_button_box(hdc, ax, mode_y, btn_w, tab_h, ad_bg, ad_border, ad_bw);
            crate::ui::draw_terminal(hdc, ax + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, ad_fg, 2);
        }

        // Desktop View (9001)
        if let Some(dx) = layout.desk_x {
            let desk_hover = hover_state == 9001;
            let (d_bg, d_border, d_fg, d_bw) = if !is_admin {
                (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
            } else if desk_hover {
                (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
            } else {
                (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
            };
            draw_tb_button_box(hdc, dx, mode_y, btn_w, tab_h, d_bg, d_border, d_bw);
            crate::ui::draw_monitor(hdc, dx + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, d_fg, 2);
        }

        // Privacy Mode (9004)
        if let Some(px) = layout.priv_x {
            let p_hover = hover_state == 9004;
            let (p_bg, p_border, p_fg, p_bw) = if is_privacy {
                (TB_PRIV_BG, TB_PRIV_BORDER, TB_PRIV_FG, 2)
            } else if p_hover {
                (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
            } else {
                (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
            };
            draw_tb_button_box(hdc, px, mode_y, btn_w, tab_h, p_bg, p_border, p_bw);
            crate::ui::draw_lock(hdc, px + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, p_fg, 2);
        }

        // Voice Intercom (9005)
        if let Some(vx) = layout.voice_x {
            let vox_hover = hover_state == 9005;
            let (vox_bg, vox_border, vox_fg, vox_bw) = if is_voice {
                (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
            } else if vox_hover {
                (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
            } else {
                (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
            };
            draw_tb_button_box(hdc, vx, mode_y, btn_w, tab_h, vox_bg, vox_border, vox_bw);
            if is_voice {
                crate::ui::draw_mic(hdc, vx + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, vox_fg, 2);
            } else {
                crate::ui::draw_mic_off(hdc, vx + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, vox_fg, 2);
            }
        }

        // Audio Mute/Unmute (9003)
        if let Some(ax) = layout.audio_x {
            let a_hover = hover_state == 9003;
            let (a_bg, a_border, a_fg, a_bw) = if !is_muted {
                (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
            } else if a_hover {
                (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
            } else {
                (TB_MUTED_BG, TB_MUTED_BORDER, TB_MUTED_FG, 1)
            };
            draw_tb_button_box(hdc, ax, mode_y, btn_w, tab_h, a_bg, a_border, a_bw);
            if is_muted {
                crate::ui::draw_volume_x(hdc, ax + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, a_fg, 2);
            } else {
                crate::ui::draw_volume(hdc, ax + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, a_fg, 2);
            }
        }

        // Session Recording (9009)
        if let Some(rx) = layout.rec_x {
            let rec_hover = hover_state == 9009;
            let (r_bg, r_border, r_fg, r_bw) = if is_rec {
                (TB_REC_BG, TB_REC_BORDER, TB_REC_FG, 2)
            } else if rec_hover {
                (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
            } else {
                (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
            };
            draw_tb_button_box(hdc, rx, mode_y, btn_w, tab_h, r_bg, r_border, r_bw);
            crate::ui::draw_record(hdc, rx + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, r_fg, 2, is_rec);
        }

        // Whiteboard Pen (9008)
        if let Some(wx) = layout.wb_x {
            let wb_hover = hover_state == 9008;
            let (wb_bg, wb_border, wb_fg, wb_bw) = if is_wb {
                (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
            } else if wb_hover {
                (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
            } else {
                (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
            };
            draw_tb_button_box(hdc, wx, mode_y, btn_w, tab_h, wb_bg, wb_border, wb_bw);
            crate::ui::draw_pen(hdc, wx + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, wb_fg, 2);
        }

        // Overflow More Actions Menu (9020)
        if let Some(mx) = layout.more_x {
            let m_hover = hover_state == 9020;
            let (m_bg, m_border, m_fg, m_bw) = if is_overflow {
                (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
            } else if m_hover {
                (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
            } else {
                (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
            };
            draw_tb_button_box(hdc, mx, mode_y, btn_w, tab_h, m_bg, m_border, m_bw);
            crate::ui::draw_more_horizontal(hdc, mx + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, m_fg);
        }

        // Quick Actions Menu (9007)
        if let Some(zx) = layout.zap_x {
            let zap_hover = hover_state == 9007;
            let (z_bg, z_border, z_fg, z_bw) = if is_zap {
                (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 2)
            } else if zap_hover {
                (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
            } else {
                (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
            };
            draw_tb_button_box(hdc, zx, mode_y, btn_w, tab_h, z_bg, z_border, z_bw);
            crate::ui::draw_zap(hdc, zx + (btn_w - 16) / 2, mode_y + (tab_h - 16) / 2, 16, z_fg, 2);
        }

        // Divider
        if let Some(dx) = layout.div_x {
            let div_rc = RECT {
                left: dx,
                top: mode_y + 4,
                right: dx + 1,
                bottom: mode_y + tab_h - 4,
            };
            FillRect(hdc, &div_rc, border_brush);
        }

        // Monitors
        if layout.is_mon_collapsed {
            // Single Smart Monitor Button (9100)
            if let Some(mx) = layout.mon1_x {
                let m_hover = hover_state == 9100;
                let (m_bg, m_border, m_fg, m_bw) = if m_hover {
                    (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
                } else {
                    (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 1)
                };
                draw_tb_button_box(hdc, mx, mode_y, btn_w, tab_h, m_bg, m_border, m_bw);
                crate::ui::draw_monitor_num(hdc, mx + (btn_w - 18) / 2, mode_y + (tab_h - 18) / 2, 18, active_mon + 1, m_fg, 2);
            }
        } else {
            // Separate Monitors (9101, 9102, 9103)
            let mons = [
                (0u32, layout.mon1_x, 9101),
                (1, layout.mon2_x, 9102),
                (2, layout.mon3_x, 9103),
            ];
            for (idx, mx_opt, hid) in mons {
                if let Some(mx) = mx_opt {
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
            }
        }

        // Codec Badge (9011)
        if let Some(cx) = layout.codec_x {
            let codec_str = crate::p2p::get_active_stream_codec_badge();
            let is_hw = crate::p2p::get_active_stream_codec() == 0x16 || crate::p2p::get_active_stream_codec() == 0x17;
            let (c_bg, c_border, c_fg, c_bw) = if hover_state == 9011 {
                (TB_HOVER_BG, TB_HOVER_BORDER, TB_HOVER_FG, 1)
            } else if is_hw {
                (TB_ACTIVE_BG, TB_ACTIVE_BORDER, TB_ACTIVE_FG, 1)
            } else {
                (TB_PASSIVE_BG, TB_PASSIVE_BORDER, TB_PASSIVE_FG, 1)
            };
            draw_tb_button_box(hdc, cx, mode_y, layout.codec_w, tab_h, c_bg, c_border, c_bw);
            let _ = SelectObject(hdc, HGDIOBJ(font_tab.0));
            SetTextColor(hdc, c_fg);
            let mut codec_txt: Vec<u16> = codec_str.encode_utf16().collect();
            let mut codec_rc = RECT {
                left: cx,
                top: mode_y,
                right: cx + layout.codec_w,
                bottom: mode_y + tab_h,
            };
            let _ = DrawTextW(hdc, &mut codec_txt, &mut codec_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
        }
    }

    // Tooltips
    let tooltip_data: Option<(&str, i32)> = match hover_state {
        9011 if !is_home => layout.codec_x.map(|cx| (crate::p2p::get_active_stream_codec_name(), cx + layout.codec_w / 2)),
        9006 => Some((if is_settings_open { "Close Settings" } else { "Settings" }, layout.settings_x + btn_w / 2)),
        9010 if !is_home => layout.chat_x.map(|cx| (if crate::p2p::is_chat_open() { "Close Chat" } else { "In-Session Chat" }, cx + btn_w / 2)),
        9020 if !is_home => layout.more_x.map(|mx| (if is_overflow_menu_open() { "Close More Actions" } else { "More Actions & Tools" }, mx + btn_w / 2)),
        9002 if !is_home => layout.admin_x.map(|ax| (if is_admin { "Admin Terminal: Active" } else { "Open Remote Admin Terminal" }, ax + btn_w / 2)),
        9001 if !is_home => layout.desk_x.map(|dx| (if !is_admin { "Desktop View: Active" } else { "Switch to Desktop View" }, dx + btn_w / 2)),
        9004 if !is_home => layout.priv_x.map(|px| (if is_privacy { "Privacy Mode: ACTIVE (Screen Blacked Out)" } else { "Privacy Mode: OFF (Blackout Screen)" }, px + btn_w / 2)),
        9005 if !is_home => layout.voice_x.map(|vx| (if is_voice { "Microphone: ACTIVE (Click to Mute)" } else { "Microphone: Muted (Click to Talk)" }, vx + btn_w / 2)),
        9003 if !is_home => layout.audio_x.map(|ax| (if !is_muted { "Audio: ACTIVE (Streaming - Click to Mute)" } else { "Audio: MUTED (Click to Unmute)" }, ax + btn_w / 2)),
        9009 if !is_home => layout.rec_x.map(|rx| (if is_rec { "Recording: ACTIVE (Click to Stop)" } else { "Start Screen Recording" }, rx + btn_w / 2)),
        9008 if !is_home => layout.wb_x.map(|wx| (if is_wb { "Whiteboard: ACTIVE (Click to Close)" } else { "Whiteboard: OFF (Click to Open)" }, wx + btn_w / 2)),
        9007 if !is_home => layout.zap_x.map(|zx| ("Quick Actions (Ctrl+Alt+Del, Win+L...)", zx + btn_w / 2)),
        9100 if !is_home => layout.mon1_x.map(|mx| ("Active Monitor (Click to Cycle 1->2->3)", mx + btn_w / 2)),
        9103 if !is_home => layout.mon3_x.map(|mx| (if active_mon == 2 { "Monitor 3: Active" } else { "Switch to Monitor 3" }, mx + btn_w / 2)),
        9102 if !is_home => layout.mon2_x.map(|mx| (if active_mon == 1 { "Monitor 2: Active" } else { "Switch to Monitor 2" }, mx + btn_w / 2)),
        9101 if !is_home => layout.mon1_x.map(|mx| (if active_mon == 0 { "Monitor 1: Active" } else { "Switch to Monitor 1" }, mx + btn_w / 2)),
        997 => Some(("Scroll Tabs Left", layout.scroll_left_x + 8)),
        998 => Some(("Scroll Tabs Right", layout.scroll_right_x + 8)),
        999 => Some(("New Remote Session", layout.plus_x + layout.plus_w / 2)),
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
    let active_idx = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
    let layout = ToolbarLayout::compute(client_w, tabs.len(), active_idx);

    if client_x < layout.brand_w {
        return 0;
    }

    // Check Home Tab
    if client_x >= layout.home_x && client_x < layout.home_x + layout.home_w {
        return 0;
    }

    // Check Right Side Buttons FIRST so they take precedence over tab strip
    let tab_y = 5;
    let tab_h = 30;
    let btn_w = 32;

    if client_y >= tab_y && client_y < tab_y + tab_h {
        if client_x >= layout.settings_x && client_x < layout.settings_x + btn_w {
            return 9006;
        }

        if active_idx > 0 {
            if let Some(cx) = layout.chat_x {
                if client_x >= cx && client_x < cx + btn_w { return 9010; }
            }
            if let Some(mx) = layout.more_x {
                if client_x >= mx && client_x < mx + btn_w { return 9020; }
            }
            if let Some(dx) = layout.desk_x {
                if client_x >= dx && client_x < dx + btn_w { return 9001; }
            }
            if let Some(ax) = layout.admin_x {
                if client_x >= ax && client_x < ax + btn_w { return 9002; }
            }
            if let Some(px) = layout.priv_x {
                if client_x >= px && client_x < px + btn_w { return 9004; }
            }
            if let Some(vx) = layout.voice_x {
                if client_x >= vx && client_x < vx + btn_w { return 9005; }
            }
            if let Some(ax) = layout.audio_x {
                if client_x >= ax && client_x < ax + btn_w { return 9003; }
            }
            if let Some(rx) = layout.rec_x {
                if client_x >= rx && client_x < rx + btn_w { return 9009; }
            }
            if let Some(wx) = layout.wb_x {
                if client_x >= wx && client_x < wx + btn_w { return 9008; }
            }
            if let Some(zx) = layout.zap_x {
                if client_x >= zx && client_x < zx + btn_w { return 9007; }
            }
            if layout.is_mon_collapsed {
                if let Some(mx) = layout.mon1_x {
                    if client_x >= mx && client_x < mx + btn_w { return 9100; }
                }
            } else {
                if let Some(mx) = layout.mon1_x {
                    if client_x >= mx && client_x < mx + btn_w { return 9101; }
                }
                if let Some(mx) = layout.mon2_x {
                    if client_x >= mx && client_x < mx + btn_w { return 9102; }
                }
                if let Some(mx) = layout.mon3_x {
                    if client_x >= mx && client_x < mx + btn_w { return 9103; }
                }
            }
            if let Some(cx) = layout.codec_x {
                if client_x >= cx && client_x < cx + layout.codec_w { return 9011; }
            }
        }
    }

    // Scroll Controls
    if layout.has_scroll_controls {
        let scroll_btn_w = 16;
        if client_x >= layout.scroll_left_x && client_x < layout.scroll_left_x + scroll_btn_w {
            return 997;
        }
        if client_x >= layout.scroll_right_x && client_x < layout.scroll_right_x + scroll_btn_w {
            return 998;
        }
    }

    // Check Session Tabs (only if before clip boundary)
    let clip_end_x = if layout.has_scroll_controls {
        layout.scroll_left_x - 4
    } else {
        layout.right_buttons_start_x - 6
    };

    if client_x < clip_end_x {
        for tab_geom in &layout.session_tabs {
            if client_x >= tab_geom.x && client_x < tab_geom.x + tab_geom.w {
                if client_x >= tab_geom.close_x && client_x < tab_geom.close_x + tab_geom.close_w {
                    return 1000 + tab_geom.index as i32;
                }
                return tab_geom.index as i32;
            }
        }

        if client_x >= layout.plus_x && client_x < layout.plus_x + layout.plus_w {
            return 999;
        }
    }

    -1
}
