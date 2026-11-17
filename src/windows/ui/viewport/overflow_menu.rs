//! Viewport Secondary Tools Overflow Dropdown Menu
//! Used on narrow viewports (< 680px) to house secondary tools:
//! Whiteboard, Recording, Voice Intercom, Privacy Mode, Admin Terminal / Desktop View.

#![allow(unsafe_op_in_unsafe_fn)]

use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    DrawTextW, SelectObject, SetBkMode, SetTextColor, DT_LEFT, DT_SINGLELINE, DT_VCENTER,
    FW_BOLD, FW_SEMIBOLD, HDC, HGDIOBJ, TRANSPARENT,
};

use super::theme::{
    create_segoe_font, draw_rounded_box, PK_BTN_HOVER, PK_CARD, PK_CARD_BORDER,
    PK_GREEN_DARK, PK_MUTED, PK_RED, PK_TEXT,
};
use super::whiteboard::is_whiteboard_active;
use super::{REMOTE_PRIVACY_MODE, VIEWPORT_IS_ADMIN_MODE};

pub static SHOW_OVERFLOW_MENU: AtomicBool = AtomicBool::new(false);

pub fn is_overflow_menu_open() -> bool {
    SHOW_OVERFLOW_MENU.load(Ordering::Relaxed)
}

pub fn toggle_overflow_menu() -> bool {
    let next = !SHOW_OVERFLOW_MENU.load(Ordering::Relaxed);
    SHOW_OVERFLOW_MENU.store(next, Ordering::Relaxed);
    next
}

pub fn set_overflow_menu_open(open: bool) {
    SHOW_OVERFLOW_MENU.store(open, Ordering::Relaxed);
}

pub struct OverflowItem {
    pub id: i32,
    pub title: &'static str,
    pub shortcut: &'static str,
    pub is_active: bool,
    pub active_color: COLORREF,
}

pub fn get_overflow_items() -> [OverflowItem; 5] {
    let is_admin = VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed);
    let is_privacy = REMOTE_PRIVACY_MODE.load(Ordering::Relaxed);
    let is_voice = crate::windows::audio::is_intercom_voice_active();
    let is_rec = crate::windows::recorder::is_recording();
    let is_wb = is_whiteboard_active();

    [
        if is_admin {
            OverflowItem {
                id: 9001,
                title: "Desktop View",
                shortcut: "Screen",
                is_active: true,
                active_color: PK_GREEN_DARK,
            }
        } else {
            OverflowItem {
                id: 9002,
                title: "Admin Terminal",
                shortcut: "Command Line",
                is_active: false,
                active_color: PK_GREEN_DARK,
            }
        },
        OverflowItem {
            id: 9004,
            title: "Privacy Screen",
            shortcut: if is_privacy { "ACTIVE" } else { "Blackout" },
            is_active: is_privacy,
            active_color: COLORREF(0x000E82E6), // Amber
        },
        OverflowItem {
            id: 9005,
            title: "Voice Intercom",
            shortcut: if is_voice { "LIVE" } else { "Muted" },
            is_active: is_voice,
            active_color: PK_GREEN_DARK,
        },
        OverflowItem {
            id: 9009,
            title: "Screen Recording",
            shortcut: if is_rec { "REC" } else { "Record MP4" },
            is_active: is_rec,
            active_color: PK_RED,
        },
        OverflowItem {
            id: 9008,
            title: "Whiteboard Pen",
            shortcut: if is_wb { "ON" } else { "Draw" },
            is_active: is_wb,
            active_color: PK_GREEN_DARK,
        },
    ]
}

pub unsafe fn draw_overflow_menu(
    hdc: HDC,
    btn_x: i32,
    btn_w: i32,
    toolbar_h: i32,
    hover_state: i32,
) {
    if !is_overflow_menu_open() {
        return;
    }

    let items = get_overflow_items();
    let menu_w = 210;
    let item_h = 32;
    let menu_h = (items.len() as i32) * item_h + 16;
    let menu_x = (btn_x + btn_w / 2 - menu_w / 2).max(10);
    let menu_y = toolbar_h + 6;

    // Card background & border
    draw_rounded_box(
        hdc,
        menu_x,
        menu_y,
        menu_w,
        menu_h,
        PK_CARD,
        PK_CARD_BORDER,
        8,
    );

    let font_title = create_segoe_font(12, FW_SEMIBOLD);
    let font_badge = create_segoe_font(11, FW_BOLD);
    SetBkMode(hdc, TRANSPARENT);

    for (i, item) in items.iter().enumerate() {
        let iy = menu_y + 8 + (i as i32) * item_h;
        let is_hover = hover_state == item.id;

        if is_hover {
            draw_rounded_box(
                hdc,
                menu_x + 6,
                iy,
                menu_w - 12,
                item_h - 2,
                PK_BTN_HOVER,
                PK_CARD_BORDER,
                6,
            );
        } else if item.is_active {
            draw_rounded_box(
                hdc,
                menu_x + 6,
                iy,
                menu_w - 12,
                item_h - 2,
                COLORREF(0x00EBFBF0), // Subtle mint
                PK_CARD_BORDER,
                6,
            );
        }

        // Draw icon
        let icon_x = menu_x + 14;
        let icon_y = iy + (item_h - 2 - 16) / 2;
        let icon_col = if item.is_active { item.active_color } else if is_hover { PK_TEXT } else { PK_MUTED };

        match item.id {
            9001 => crate::ui::draw_monitor(hdc, icon_x, icon_y, 16, icon_col, 2),
            9002 => crate::ui::draw_terminal(hdc, icon_x, icon_y, 16, icon_col, 2),
            9004 => crate::ui::draw_lock(hdc, icon_x, icon_y, 16, icon_col, 2),
            9005 => {
                if item.is_active {
                    crate::ui::draw_mic(hdc, icon_x, icon_y, 16, icon_col, 2);
                } else {
                    crate::ui::draw_mic_off(hdc, icon_x, icon_y, 16, icon_col, 2);
                }
            }
            9009 => crate::ui::draw_record(hdc, icon_x, icon_y, 16, icon_col, 2, item.is_active),
            9008 => crate::ui::draw_pen(hdc, icon_x, icon_y, 16, icon_col, 2),
            _ => {}
        }

        // Draw title
        let _ = SelectObject(hdc, HGDIOBJ(font_title.0));
        let title_fg = if item.is_active {
            item.active_color
        } else if is_hover {
            PK_TEXT
        } else {
            PK_TEXT
        };
        SetTextColor(hdc, title_fg);

        let mut title_txt: Vec<u16> = item.title.encode_utf16().collect();
        let mut title_rc = RECT {
            left: menu_x + 36,
            top: iy,
            right: menu_x + 140,
            bottom: iy + item_h - 2,
        };
        let _ = DrawTextW(hdc, &mut title_txt, &mut title_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

        // Draw badge
        let _ = SelectObject(hdc, HGDIOBJ(font_badge.0));
        let badge_fg = if item.is_active {
            item.active_color
        } else if is_hover {
            PK_MUTED
        } else {
            COLORREF(0x0090A0B0)
        };
        SetTextColor(hdc, badge_fg);

        let mut sc_txt: Vec<u16> = item.shortcut.encode_utf16().collect();
        let mut sc_rc = RECT {
            left: menu_x + 130,
            top: iy,
            right: menu_x + menu_w - 14,
            bottom: iy + item_h - 2,
        };
        let _ = DrawTextW(hdc, &mut sc_txt, &mut sc_rc, DT_SINGLELINE | DT_VCENTER);
    }
}

pub fn hit_test_overflow_menu(
    btn_x: i32,
    btn_w: i32,
    toolbar_h: i32,
    client_x: i32,
    client_y: i32,
) -> i32 {
    if !is_overflow_menu_open() {
        return -1;
    }

    let items = get_overflow_items();
    let menu_w = 210;
    let item_h = 32;
    let menu_h = (items.len() as i32) * item_h + 16;
    let menu_x = (btn_x + btn_w / 2 - menu_w / 2).max(10);
    let menu_y = toolbar_h + 6;

    if client_x < menu_x || client_x >= menu_x + menu_w || client_y < menu_y || client_y >= menu_y + menu_h {
        return -1;
    }

    for (i, item) in items.iter().enumerate() {
        let iy = menu_y + 8 + (i as i32) * item_h;
        if client_y >= iy && client_y < iy + item_h {
            return item.id;
        }
    }

    9020 // Inside container
}
