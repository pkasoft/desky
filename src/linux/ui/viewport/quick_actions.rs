//! In-Session Quick Actions Dropdown Menu (Linux X11)
//!
//! Mirrors `src/windows/ui/viewport/quick_actions.rs` for fast system hotkey dispatching:
//! Ctrl+Alt+Del, Win+L, Task Manager, Show Desktop, Safe Reboot.

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};

use crate::linux::ui::theme::{
    PixelCanvas, PK_BTN_HOVER, PK_CARD, PK_CARD_BORDER, PK_GREEN_DARK, PK_MUTED, PK_RED,
    PK_RED_TINT, PK_TEXT,
};
use crate::protocol::{AdminReq, SystemAction};

pub static SHOW_QUICK_ACTIONS: AtomicBool = AtomicBool::new(false);

pub struct QuickActionItem {
    pub id: i32,
    pub title: &'static str,
    pub shortcut: &'static str,
    pub action: SystemAction,
    pub requires_confirm: bool,
}

pub const QUICK_ACTIONS: [QuickActionItem; 8] = [
    QuickActionItem {
        id: 9201,
        title: "Send Ctrl+Alt+Del",
        shortcut: "SAS / Login",
        action: SystemAction::CtrlAltDel,
        requires_confirm: false,
    },
    QuickActionItem {
        id: 9202,
        title: "Lock Workstation",
        shortcut: "Win+L",
        action: SystemAction::LockWorkstation,
        requires_confirm: false,
    },
    QuickActionItem {
        id: 9203,
        title: "Task Manager",
        shortcut: "Ctrl+Shift+Esc",
        action: SystemAction::TaskManager,
        requires_confirm: false,
    },
    QuickActionItem {
        id: 9204,
        title: "Show Desktop",
        shortcut: "Win+D",
        action: SystemAction::ShowDesktop,
        requires_confirm: false,
    },
    QuickActionItem {
        id: 9205,
        title: "Open Run Dialog",
        shortcut: "Win+R",
        action: SystemAction::OpenRun,
        requires_confirm: false,
    },
    QuickActionItem {
        id: 9206,
        title: "Restart Computer",
        shortcut: "Reboot",
        action: SystemAction::RebootNormal,
        requires_confirm: true,
    },
    QuickActionItem {
        id: 9207,
        title: "Safe Mode Restart",
        shortcut: "Safe Boot",
        action: SystemAction::RebootSafe,
        requires_confirm: true,
    },
    QuickActionItem {
        id: 9208,
        title: "Elevate Remote Host",
        shortcut: "UAC / Admin",
        action: SystemAction::ElevatePrivileges,
        requires_confirm: false,
    },
];

pub fn is_quick_actions_open() -> bool {
    SHOW_QUICK_ACTIONS.load(Ordering::Relaxed)
}

pub fn toggle_quick_actions() -> bool {
    let new_val = !SHOW_QUICK_ACTIONS.load(Ordering::Relaxed);
    SHOW_QUICK_ACTIONS.store(new_val, Ordering::Relaxed);
    new_val
}

pub fn set_quick_actions_open(open: bool) {
    SHOW_QUICK_ACTIONS.store(open, Ordering::Relaxed);
}

pub fn draw_quick_actions_popup(
    canvas: &mut PixelCanvas,
    btn_x: i32,
    btn_w: i32,
    toolbar_h: i32,
    hover_state: i32,
) {
    if !is_quick_actions_open() {
        return;
    }

    let menu_w = 236;
    let item_h = 32;
    let menu_h = (QUICK_ACTIONS.len() as i32) * item_h + 16;
    let menu_x = (btn_x + btn_w / 2 - menu_w / 2).max(10);
    let menu_y = toolbar_h + 6;

    canvas.draw_rounded_box(menu_x, menu_y, menu_w, menu_h, PK_CARD, PK_CARD_BORDER, 8);

    for (i, item) in QUICK_ACTIONS.iter().enumerate() {
        let iy = menu_y + 8 + (i as i32) * item_h;
        let is_hover = hover_state == item.id;

        if is_hover {
            let h_bg = if item.requires_confirm {
                PK_RED_TINT
            } else {
                PK_BTN_HOVER
            };
            canvas.draw_rounded_box(menu_x + 6, iy, menu_w - 12, item_h - 2, h_bg, PK_CARD_BORDER, 6);
        }

        let title_fg = if is_hover && item.requires_confirm {
            PK_RED
        } else if is_hover {
            PK_TEXT
        } else {
            PK_MUTED
        };
        canvas.draw_text_vcenter(
            menu_x + 14,
            iy,
            134,
            item_h - 2,
            item.title,
            12.5,
            false,
            title_fg,
        );

        let badge_fg = if is_hover && item.requires_confirm {
            PK_RED
        } else {
            PK_GREEN_DARK
        };
        canvas.draw_text_vcenter(
            menu_x + 150,
            iy,
            menu_w - 160,
            item_h - 2,
            item.shortcut,
            11.0,
            true,
            badge_fg,
        );
    }
}

pub fn hit_test_quick_actions(
    btn_x: i32,
    btn_w: i32,
    toolbar_h: i32,
    client_x: i32,
    client_y: i32,
) -> i32 {
    if !is_quick_actions_open() {
        return -1;
    }

    let menu_w = 236;
    let item_h = 32;
    let menu_h = (QUICK_ACTIONS.len() as i32) * item_h + 16;
    let menu_x = (btn_x + btn_w / 2 - menu_w / 2).max(10);
    let menu_y = toolbar_h + 6;

    if client_x < menu_x
        || client_x >= menu_x + menu_w
        || client_y < menu_y
        || client_y >= menu_y + menu_h
    {
        return -1;
    }

    for (i, item) in QUICK_ACTIONS.iter().enumerate() {
        let iy = menu_y + 8 + (i as i32) * item_h;
        if client_y >= iy && client_y < iy + item_h {
            return item.id;
        }
    }

    9200
}

pub fn execute_quick_action(hit_id: i32) {
    if let Some(item) = QUICK_ACTIONS.iter().find(|it| it.id == hit_id) {
        crate::p2p::send_admin_request(AdminReq::TriggerSystemAction {
            action: item.action,
        });
        SHOW_QUICK_ACTIONS.store(false, Ordering::Relaxed);
    }
}
