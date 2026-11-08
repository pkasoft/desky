//! In-Session Quick Actions Dropdown Menu
//! Fast system hotkey dispatching: Ctrl+Alt+Del, Win+L, Task Manager, Show Desktop, Safe Reboot.

#![allow(unsafe_op_in_unsafe_fn)]

use crate::protocol::{AdminReq, SystemAction};
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::{COLORREF, HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    DrawTextW, SelectObject, SetBkMode, SetTextColor, DT_LEFT, DT_SINGLELINE, DT_VCENTER,
    FW_BOLD, FW_SEMIBOLD, HDC, HGDIOBJ, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::MessageBoxW;
use windows::core::w;

pub static SHOW_QUICK_ACTIONS: AtomicBool = AtomicBool::new(false);

pub struct QuickActionItem {
    pub id: i32,
    pub title: &'static str,
    pub shortcut: &'static str,
    pub action: SystemAction,
    pub requires_confirm: bool,
}

pub const QUICK_ACTIONS: [QuickActionItem; 7] = [
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

pub unsafe fn draw_quick_actions_popup(
    hdc: HDC,
    btn_x: i32,
    btn_w: i32,
    toolbar_h: i32,
    hover_state: i32,
) {
    if !is_quick_actions_open() {
        return;
    }

    let menu_w = 230;
    let item_h = 32;
    let menu_h = (QUICK_ACTIONS.len() as i32) * item_h + 16;
    let menu_x = (btn_x + btn_w / 2 - menu_w / 2).max(10);
    let menu_y = toolbar_h + 6;

    // Card background & border
    super::theme::draw_rounded_box(
        hdc,
        menu_x,
        menu_y,
        menu_w,
        menu_h,
        super::theme::PK_CARD,
        super::theme::PK_CARD_BORDER,
        8,
    );

    let font_title = super::theme::create_segoe_font(12, FW_SEMIBOLD);
    let font_shortcut = super::theme::create_segoe_font(11, FW_BOLD);
    SetBkMode(hdc, TRANSPARENT);

    for (i, item) in QUICK_ACTIONS.iter().enumerate() {
        let iy = menu_y + 8 + (i as i32) * item_h;
        let is_hover = hover_state == item.id;

        if is_hover {
            let h_bg = if item.requires_confirm {
                COLORREF(0x00EBF0FF) // Light red tint
            } else {
                super::theme::PK_BTN_HOVER
            };
            super::theme::draw_rounded_box(
                hdc,
                menu_x + 6,
                iy,
                menu_w - 12,
                item_h - 2,
                h_bg,
                super::theme::PK_CARD_BORDER,
                6,
            );
        }

        // Draw title
        let _ = SelectObject(hdc, HGDIOBJ(font_title.0));
        let title_fg = if is_hover && item.requires_confirm {
            super::theme::PK_RED
        } else if is_hover {
            super::theme::PK_TEXT
        } else {
            super::theme::PK_MUTED
        };
        SetTextColor(hdc, title_fg);

        let mut title_txt: Vec<u16> = item.title.encode_utf16().collect();
        let mut title_rc = RECT {
            left: menu_x + 16,
            top: iy,
            right: menu_x + 150,
            bottom: iy + item_h - 2,
        };
        let _ = DrawTextW(hdc, &mut title_txt, &mut title_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

        // Draw shortcut / badge on right
        let _ = SelectObject(hdc, HGDIOBJ(font_shortcut.0));
        let badge_fg = if is_hover && item.requires_confirm {
            super::theme::PK_RED
        } else if is_hover {
            super::theme::PK_GREEN_DARK
        } else {
            COLORREF(0x0090A0B0)
        };
        SetTextColor(hdc, badge_fg);

        let mut sc_txt: Vec<u16> = item.shortcut.encode_utf16().collect();
        let mut sc_rc = RECT {
            left: menu_x + 140,
            top: iy,
            right: menu_x + menu_w - 16,
            bottom: iy + item_h - 2,
        };
        let _ = DrawTextW(hdc, &mut sc_txt, &mut sc_rc, DT_SINGLELINE | DT_VCENTER);
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

    let menu_w = 230;
    let item_h = 32;
    let menu_h = (QUICK_ACTIONS.len() as i32) * item_h + 16;
    let menu_x = (btn_x + btn_w / 2 - menu_w / 2).max(10);
    let menu_y = toolbar_h + 6;

    if client_x < menu_x || client_x >= menu_x + menu_w || client_y < menu_y || client_y >= menu_y + menu_h {
        return -1;
    }

    for (i, item) in QUICK_ACTIONS.iter().enumerate() {
        let iy = menu_y + 8 + (i as i32) * item_h;
        if client_y >= iy && client_y < iy + item_h {
            return item.id;
        }
    }

    9200 // Inside menu container
}

pub unsafe fn execute_quick_action(parent_hwnd: HWND, hit_id: i32) {
    if let Some(item) = QUICK_ACTIONS.iter().find(|it| it.id == hit_id) {
        if item.requires_confirm {
            let msg = if item.action == SystemAction::RebootSafe {
                w!("Are you sure you want to reboot the remote computer into SAFE MODE?\r\n\r\nDesky will automatically configure RunOnce reconnect and reboot.")
            } else {
                w!("Are you sure you want to RESTART the remote computer now?\r\n\r\nDesky will reconnect automatically once Windows boots up.")
            };
            let ret = MessageBoxW(
                Some(parent_hwnd),
                msg,
                w!("Desky - Remote Reboot Confirmation"),
                windows::Win32::UI::WindowsAndMessaging::MB_YESNO | windows::Win32::UI::WindowsAndMessaging::MB_ICONWARNING,
            );
            if ret != windows::Win32::UI::WindowsAndMessaging::IDYES {
                return;
            }
        }

        crate::p2p::send_admin_request(AdminReq::TriggerSystemAction { action: item.action });
        SHOW_QUICK_ACTIONS.store(false, Ordering::Relaxed);
    }
}
