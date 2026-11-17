//! Viewport Secondary Tools Overflow Dropdown Menu (Linux X11)
//!
//! Mirrors `src/windows/ui/viewport/overflow_menu.rs` for narrow viewports (< 680px).

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};

use super::tabs::{REMOTE_PRIVACY_MODE, VIEWPORT_IS_ADMIN_MODE};
use super::whiteboard::is_whiteboard_active;
use crate::linux::ui::theme::{
    PixelCanvas, PK_AMBER, PK_BTN_HOVER, PK_CARD, PK_CARD_BORDER, PK_GREEN_DARK, PK_GREEN_TINT,
    PK_MUTED, PK_RED, PK_TEXT,
};

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
    pub active_color: u32,
}

pub fn get_overflow_items() -> [OverflowItem; 5] {
    let is_admin = VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed);
    let is_privacy = REMOTE_PRIVACY_MODE.load(Ordering::Relaxed);
    let is_rec = crate::recorder::is_recording();
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
            active_color: PK_AMBER,
        },
        OverflowItem {
            id: 9005,
            title: "Voice Intercom",
            shortcut: "Disabled",
            is_active: false,
            active_color: PK_MUTED,
        },
        OverflowItem {
            id: 9009,
            title: "Screen Recording",
            shortcut: if is_rec { "REC" } else { "Record" },
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

pub fn draw_overflow_menu(
    canvas: &mut PixelCanvas,
    btn_x: i32,
    btn_w: i32,
    toolbar_h: i32,
    hover_state: i32,
) {
    if !is_overflow_menu_open() {
        return;
    }

    let items = get_overflow_items();
    let menu_w = 216;
    let item_h = 32;
    let menu_h = (items.len() as i32) * item_h + 16;
    let menu_x = (btn_x + btn_w / 2 - menu_w / 2).max(10);
    let menu_y = toolbar_h + 6;

    canvas.draw_rounded_box(menu_x, menu_y, menu_w, menu_h, PK_CARD, PK_CARD_BORDER, 8);

    for (i, item) in items.iter().enumerate() {
        let iy = menu_y + 8 + (i as i32) * item_h;
        let is_hover = hover_state == item.id;

        if is_hover {
            canvas.draw_rounded_box(
                menu_x + 6,
                iy,
                menu_w - 12,
                item_h - 2,
                PK_BTN_HOVER,
                PK_CARD_BORDER,
                6,
            );
        } else if item.is_active {
            canvas.draw_rounded_box(
                menu_x + 6,
                iy,
                menu_w - 12,
                item_h - 2,
                PK_GREEN_TINT,
                PK_CARD_BORDER,
                6,
            );
        }

        let icon_x = menu_x + 14;
        let icon_y = iy + (item_h - 2 - 16) / 2;
        let icon_col = if item.is_active {
            item.active_color
        } else if is_hover {
            PK_TEXT
        } else {
            PK_MUTED
        };

        match item.id {
            9001 => canvas.draw_icon_monitor(icon_x, icon_y, 16, icon_col, None),
            9002 => canvas.draw_icon_terminal(icon_x, icon_y, 16, icon_col),
            9004 => canvas.draw_icon_lock(icon_x, icon_y, 16, icon_col),
            9005 => canvas.draw_icon_mic(icon_x, icon_y, 16, icon_col, true),
            9009 => canvas.draw_icon_record(icon_x, icon_y, 16, icon_col, item.is_active),
            9008 => canvas.draw_icon_pen(icon_x, icon_y, 16, icon_col),
            _ => {}
        }

        let title_fg = if item.is_active {
            item.active_color
        } else {
            PK_TEXT
        };
        canvas.draw_text_vcenter(
            menu_x + 36,
            iy,
            106,
            item_h - 2,
            item.title,
            12.5,
            false,
            title_fg,
        );

        let badge_fg = if item.is_active {
            item.active_color
        } else {
            PK_MUTED
        };
        canvas.draw_text_vcenter(
            menu_x + 142,
            iy,
            menu_w - 150,
            item_h - 2,
            item.shortcut,
            11.0,
            true,
            badge_fg,
        );
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
    let menu_w = 216;
    let item_h = 32;
    let menu_h = (items.len() as i32) * item_h + 16;
    let menu_x = (btn_x + btn_w / 2 - menu_w / 2).max(10);
    let menu_y = toolbar_h + 6;

    if client_x < menu_x
        || client_x >= menu_x + menu_w
        || client_y < menu_y
        || client_y >= menu_y + menu_h
    {
        return -1;
    }

    for (i, item) in items.iter().enumerate() {
        let iy = menu_y + 8 + (i as i32) * item_h;
        if client_y >= iy && client_y < iy + item_h {
            return item.id;
        }
    }

    9020
}
