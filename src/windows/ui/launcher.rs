//! Main Win32 Launcher Window & Application Lifecycle
//!
//! Exposes responsive Home page rendering, layout calculations, and
//! application lifecycle for the unified Desky window.

#![allow(unsafe_op_in_unsafe_fn)]
use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU8, Ordering};
use tracing::info;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetTextExtentPoint32W, InvalidateRect,
    SelectObject, SetBkMode, SetTextColor, DT_CENTER, DT_LEFT, DT_SINGLELINE, DT_VCENTER,
    DT_WORDBREAK, FW_BOLD, FW_NORMAL, FW_SEMIBOLD, HBRUSH, HDC, HGDIOBJ, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DispatchMessageW, GetMessageW, GetWindowTextLengthW, GetWindowTextW,
    IsWindow, LoadCursorW, PostMessageW, RegisterClassExW, SendMessageW, SetCursor,
    SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, IDC_ARROW, IDC_HAND, MSG, SWP_NOACTIVATE,
    SWP_NOZORDER, SW_HIDE, SW_SHOW, WINDOW_EX_STYLE, WM_SETFONT,
    WNDCLASSEXW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

use super::theme::{
    copy_to_clipboard, create_segoe_font, draw_rounded_box, PK_BG, PK_BTN_HOVER, PK_CARD,
    PK_CARD_BORDER, PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_RED, PK_TEXT,
};
use super::viewport::{viewport_wndproc, VIEWPORT_BRUSH};

pub const WM_APP_REDRAW: u32 = 0x8001;
pub const WM_APP_BRING_FRONT: u32 = 0x8002;
pub const WM_APP_SHOW_PROGRESS: u32 = 0x8004;

pub const ID_EDIT_TARGET_CODE: usize = 1003;

static IS_RUNNING: AtomicBool = AtomicBool::new(false);

pub(crate) static mut HWND_MAIN_WINDOW: HWND = HWND(std::ptr::null_mut());
pub(crate) static mut HWND_EDIT_TARGET: HWND = HWND(std::ptr::null_mut());
static mut BG_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());
static mut EDIT_BG_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());

static HOVER_BUTTON: AtomicU8 = AtomicU8::new(0); // 0 = none, 1 = copy, 2 = connect, 3 = accept, 4 = decline
static CURRENT_EDIT_FONT: AtomicIsize = AtomicIsize::new(0);

pub static LOCAL_DESKY_ID: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
pub static BOTTOM_TAB_RADAR: AtomicBool = AtomicBool::new(false);
static STATUS_LOGS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub fn request_main_redraw() {
    unsafe {
        if !HWND_MAIN_WINDOW.0.is_null() && IsWindow(Some(HWND_MAIN_WINDOW)).as_bool() {
            let _ = InvalidateRect(Some(HWND_MAIN_WINDOW), None, false);
            let _ = PostMessageW(Some(HWND_MAIN_WINDOW), WM_APP_REDRAW, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn bring_main_window_to_front() {
    unsafe {
        if !HWND_MAIN_WINDOW.0.is_null() && IsWindow(Some(HWND_MAIN_WINDOW)).as_bool() {
            let _ = InvalidateRect(Some(HWND_MAIN_WINDOW), None, false);
            let _ = PostMessageW(
                Some(HWND_MAIN_WINDOW),
                WM_APP_BRING_FRONT,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

pub fn append_status_log(msg: &str) {
    if let Ok(mut logs) = STATUS_LOGS.lock() {
        logs.push(msg.to_string());
        if logs.len() > 5 {
            logs.remove(0);
        }
    }
    request_main_redraw();
}

pub fn prompt_incoming_request(client_id: &str) {
    append_status_log(&format!(
        "Incoming request from ID: {} - Click Accept or Decline!",
        client_id
    ));
    bring_main_window_to_front();
}

pub fn set_assigned_desky_id(id: &str) {
    if let Ok(mut guard) = LOCAL_DESKY_ID.lock() {
        *guard = id.to_string();
    }
    append_status_log(&format!("Online | ID: {}", id));
    request_main_redraw();
}

pub fn get_hover_button() -> u8 {
    HOVER_BUTTON.load(Ordering::Relaxed)
}

pub fn set_main_window_hwnd(hwnd: HWND) {
    unsafe {
        HWND_MAIN_WINDOW = hwnd;
    }
}

pub fn set_edit_target_hwnd(hwnd: HWND) {
    unsafe {
        HWND_EDIT_TARGET = hwnd;
    }
}

pub fn set_edit_target_visibility(visible: bool) {
    unsafe {
        if !HWND_EDIT_TARGET.0.is_null() && IsWindow(Some(HWND_EDIT_TARGET)).as_bool() {
            let _ = ShowWindow(HWND_EDIT_TARGET, if visible { SW_SHOW } else { SW_HIDE });
        }
    }
}

pub fn get_edit_bg_brush() -> HBRUSH {
    unsafe { EDIT_BG_BRUSH }
}

#[derive(Clone, Copy, Debug)]
pub struct ResponsiveLayout {
    pub c1_x: i32,
    pub c2_x: i32,
    pub card_w: i32,
    pub card_h: i32,
    pub card_y: i32,

    pub id_box_x: i32,
    pub id_box_y: i32,
    pub id_box_w: i32,
    pub id_box_h: i32,

    pub copy_btn_x: i32,
    pub copy_btn_y: i32,
    pub copy_btn_w: i32,
    pub copy_btn_h: i32,

    pub input_box_x: i32,
    pub input_box_y: i32,
    pub input_box_w: i32,
    pub input_box_h: i32,

    pub edit_x: i32,
    pub edit_y: i32,
    pub edit_w: i32,
    pub edit_h: i32,

    pub conn_btn_x: i32,
    pub conn_btn_y: i32,
    pub conn_btn_w: i32,
    pub conn_btn_h: i32,

    pub col1_x: i32,
    pub col2_x: i32,
    pub col_w: i32,
    pub col_h: i32,
    pub col_y: i32,

    pub font_scale: f32,
}

/// Computes responsive layout geometry for any window size with top tab bar offset
pub fn get_responsive_layout(w: i32, h: i32) -> ResponsiveLayout {
    let top_offset = 40;
    let margin = 24;
    let gap = 18;
    let card_w = (w - (2 * margin) - gap).max(220) / 2;

    let scale_w = w as f32 / 1024.0;
    let scale_h = h as f32 / 680.0;
    let font_scale = (scale_w.min(scale_h)).clamp(1.0, 1.4);

    let card_h = ((185.0 * font_scale) as i32).min((h - top_offset - 200) / 2).max(175);
    let card_y = top_offset + 14;

    let c1_x = margin;
    let c2_x = margin + card_w + gap;

    // Card 1: This Desk elements
    let id_box_x = c1_x + 18;
    let id_box_y = card_y + (46.0 * font_scale) as i32;
    let id_box_w = card_w - 36;
    let id_box_h = (64.0 * font_scale) as i32;

    let copy_btn_x = c1_x + 18;
    let copy_btn_y = id_box_y + id_box_h + (14.0 * font_scale) as i32;
    let copy_btn_w = card_w - 36;
    let copy_btn_h = (42.0 * font_scale) as i32;

    // Card 2: Remote Desk elements
    let input_box_x = c2_x + 18;
    let input_box_y = card_y + (46.0 * font_scale) as i32;
    let input_box_w = card_w - 36;
    let input_box_h = (48.0 * font_scale) as i32;

    let edit_x = input_box_x + 10;
    let edit_y = input_box_y + 8;
    let edit_w = (input_box_w - 20).max(80);
    let edit_h = (input_box_h - 16).max(28);

    let conn_btn_x = c2_x + 18;
    let conn_btn_y = copy_btn_y;
    let conn_btn_w = card_w - 36;
    let conn_btn_h = copy_btn_h;

    // Bottom Section: Split into 2 columns (Recent Sessions & System Diagnostics)
    let col_y = card_y + card_h + 16;
    let col_w = card_w;
    let col_h = (h - col_y - margin).max(140);
    let col1_x = c1_x;
    let col2_x = c2_x;

    ResponsiveLayout {
        c1_x,
        c2_x,
        card_w,
        card_h,
        card_y,
        id_box_x,
        id_box_y,
        id_box_w,
        id_box_h,
        copy_btn_x,
        copy_btn_y,
        copy_btn_w,
        copy_btn_h,
        input_box_x,
        input_box_y,
        input_box_w,
        input_box_h,
        edit_x,
        edit_y,
        edit_w,
        edit_h,
        conn_btn_x,
        conn_btn_y,
        conn_btn_w,
        conn_btn_h,
        col1_x,
        col2_x,
        col_w,
        col_h,
        col_y,
        font_scale,
    }
}

pub unsafe fn update_edit_layout(w: i32, h: i32) {
    if !HWND_EDIT_TARGET.0.is_null() && IsWindow(Some(HWND_EDIT_TARGET)).as_bool() {
        let l = get_responsive_layout(w, h);
        let f_edit_size = (20.0 * l.font_scale) as i32;
        let font_edit = create_segoe_font(f_edit_size, FW_SEMIBOLD);
        let old = CURRENT_EDIT_FONT.swap(font_edit.0 as isize, Ordering::Relaxed);
        if old != 0 {
            let _ = DeleteObject(HGDIOBJ(old as *mut _));
        }
        let _ = SendMessageW(
            HWND_EDIT_TARGET,
            WM_SETFONT,
            Some(WPARAM(font_edit.0 as usize)),
            Some(LPARAM(1)),
        );
        let _ = SetWindowPos(
            HWND_EDIT_TARGET,
            None,
            l.edit_x,
            l.edit_y,
            l.edit_w,
            l.edit_h,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

pub struct NativeWin32App;

impl NativeWin32App {
    pub fn new() -> Result<Self> {
        unsafe {
            let h_module = GetModuleHandleW(None)?;
            let h_instance: HINSTANCE = h_module.into();

            BG_BRUSH = CreateSolidBrush(PK_BG);
            EDIT_BG_BRUSH = CreateSolidBrush(PK_INPUT_BG);
            VIEWPORT_BRUSH = CreateSolidBrush(COLORREF(0x000C0A09));

            // Register Viewport Window Class as the single primary application window class
            let viewport_class_name = w!("DeskyViewportClass");
            let vwc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(viewport_wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: h_instance,
                hIcon: Default::default(),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                hbrBackground: BG_BRUSH,
                lpszMenuName: PCWSTR::null(),
                lpszClassName: viewport_class_name,
                hIconSm: Default::default(),
            };
            RegisterClassExW(&vwc);

            // Initialize Session Tabs with Tab 0 permanently as [ Home ]
            super::viewport::init_session_tabs();

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                viewport_class_name,
                w!("Desky | pkasoft.com"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                1024,
                680,
                None,
                None,
                Some(h_instance),
                None,
            )?;
            HWND_MAIN_WINDOW = hwnd;

            // Light theme titlebar
            let dark_mode: i32 = 0;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark_mode as *const _ as *const _,
                std::mem::size_of::<i32>() as u32,
            );

            // Start dedicated viewport render thread immediately for unified window
            super::viewport::start_viewport_render_thread(hwnd);

            info!("Native Win32 Unified Desky Window spawned successfully (HWND: {:?})", hwnd);
            Ok(Self)
        }
    }

    pub fn run_message_loop(&self) {
        IS_RUNNING.store(true, Ordering::SeqCst);
        unsafe {
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        IS_RUNNING.store(false, Ordering::SeqCst);
        info!("Native Win32 message loop terminated.");
    }
}

/// Renders the complete pkasoft.com White Theme Homepage onto the provided DC
pub unsafe fn draw_homepage_view(mem_dc: HDC, w: i32, h: i32, hover: u8) {
    let rc = RECT {
        left: 0,
        top: 40,
        right: w,
        bottom: h,
    };
    FillRect(mem_dc, &rc, BG_BRUSH);
    SetBkMode(mem_dc, TRANSPARENT);

    let l = get_responsive_layout(w, h);
    let f_sub = ((14.0 * l.font_scale) as i32).max(13);
    let f_badge = ((13.0 * l.font_scale) as i32).max(12);
    let f_id = ((34.0 * l.font_scale) as i32).max(28);
    let f_btn = ((16.0 * l.font_scale) as i32).max(14);
    let f_console = ((13.0 * l.font_scale) as i32).max(12);

    let font_sub = create_segoe_font(f_sub, FW_NORMAL);
    let font_badge = create_segoe_font(f_badge, FW_BOLD);
    let font_id = create_segoe_font(f_id, FW_BOLD);
    let font_btn = create_segoe_font(f_btn, FW_BOLD);
    let font_console = create_segoe_font(f_console, FW_NORMAL);

    let pending_req = crate::p2p::get_pending_approval();

    if let Some(req) = pending_req {
        // INCOMING SESSION APPROVAL VIEW (Modal Card)
        let mw = (w - 64).clamp(440, 620);
        let mh = 290;
        let mx = (w - mw) / 2;
        let my = ((h - mh) / 2).max(68);

        draw_rounded_box(mem_dc, mx, my, mw, mh, PK_CARD, PK_GREEN, 14);

        // Badge Pill with Lucide Bell vector icon
        let badge_w = 260;
        let badge_x = mx + (mw - badge_w) / 2;
        draw_rounded_box(mem_dc, badge_x, my + 22, badge_w, 28, PK_GREEN_TINT, PK_GREEN, 14);
        crate::ui::draw_bell(mem_dc, badge_x + 10, my + 28, 16, PK_GREEN, 2);
        let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
        SetTextColor(mem_dc, PK_GREEN);
        let mut b_txt: Vec<u16> = "INCOMING CONNECTION REQUEST".encode_utf16().collect();
        let mut b_rc = RECT {
            left: badge_x + 28,
            top: my + 22,
            right: badge_x + badge_w - 6,
            bottom: my + 50,
        };
        let _ = DrawTextW(mem_dc, &mut b_txt, &mut b_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

        // Client ID Display
        let _ = SelectObject(mem_dc, HGDIOBJ(font_id.0));
        SetTextColor(mem_dc, PK_TEXT);
        let mut cid_txt: Vec<u16> = format!("ID: {}", req.client_id).encode_utf16().collect();
        let mut cid_rc = RECT {
            left: mx + 20,
            top: my + 66,
            right: mx + mw - 20,
            bottom: my + 116,
        };
        let _ = DrawTextW(mem_dc, &mut cid_txt, &mut cid_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

        // Subtitle prompt
        let _ = SelectObject(mem_dc, HGDIOBJ(font_sub.0));
        SetTextColor(mem_dc, PK_MUTED);
        let mut p_txt: Vec<u16> =
            "This remote client is requesting permission to view and control your desktop."
                .encode_utf16()
                .collect();
        let mut p_rc = RECT {
            left: mx + 24,
            top: my + 124,
            right: mx + mw - 24,
            bottom: my + 156,
        };
        let _ = DrawTextW(mem_dc, &mut p_txt, &mut p_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

        // [ Accept ] and [ Decline ] buttons with Lucide Check & X vector icons
        let btn_w = (mw - 72) / 2;
        let btn_h = 50;
        let btn_y = my + 190;
        let acc_x = mx + 24;
        let dec_x = mx + 48 + btn_w;

        let acc_bg = if hover == 3 { PK_GREEN_DARK } else { PK_GREEN };
        draw_rounded_box(mem_dc, acc_x, btn_y, btn_w, btn_h, acc_bg, acc_bg, 10);
        crate::ui::draw_check(mem_dc, acc_x + 18, btn_y + 16, 18, COLORREF(0x00FFFFFF), 2);
        let _ = SelectObject(mem_dc, HGDIOBJ(font_btn.0));
        SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
        let mut acc_txt: Vec<u16> = "Accept Connection".encode_utf16().collect();
        let mut acc_rc = RECT {
            left: acc_x + 36,
            top: btn_y,
            right: acc_x + btn_w,
            bottom: btn_y + btn_h,
        };
        let _ = DrawTextW(mem_dc, &mut acc_txt, &mut acc_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

        draw_rounded_box(mem_dc, dec_x, btn_y, btn_w, btn_h, PK_RED, PK_RED, 10);
        crate::ui::draw_x(mem_dc, dec_x + 22, btn_y + 16, 18, COLORREF(0x00FFFFFF), 2);
        SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
        let mut dec_txt: Vec<u16> = "Decline".encode_utf16().collect();
        let mut dec_rc = RECT {
            left: dec_x + 36,
            top: btn_y,
            right: dec_x + btn_w,
            bottom: btn_y + btn_h,
        };
        let _ = DrawTextW(mem_dc, &mut dec_txt, &mut dec_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    } else {
        // Normal Homepage Cards

        // 1. Card 1: THIS DESK (Left)
        draw_rounded_box(mem_dc, l.c1_x, l.card_y, l.card_w, l.card_h, PK_CARD, PK_GREEN, 12);

        let tag1_w = (92.0 * l.font_scale) as i32;
        let tag1_h = (22.0 * l.font_scale) as i32;
        draw_rounded_box(mem_dc, l.c1_x + 18, l.card_y + 14, tag1_w, tag1_h, PK_GREEN_TINT, PK_GREEN, 6);
        let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
        SetTextColor(mem_dc, PK_GREEN);
        let mut tag1: Vec<u16> = "THIS DESK".encode_utf16().collect();
        let mut tag1_rect = RECT {
            left: l.c1_x + 18,
            top: l.card_y + 14,
            right: l.c1_x + 18 + tag1_w,
            bottom: l.card_y + 14 + tag1_h,
        };
        let _ = DrawTextW(mem_dc, &mut tag1, &mut tag1_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

        draw_rounded_box(mem_dc, l.id_box_x, l.id_box_y, l.id_box_w, l.id_box_h, PK_INPUT_BG, PK_CARD_BORDER, 8);
        SetTextColor(mem_dc, PK_MUTED);
        let mut id_label: Vec<u16> = "YOUR ID".encode_utf16().collect();
        let mut id_label_rect = RECT {
            left: l.id_box_x + 14,
            top: l.id_box_y + 6,
            right: l.id_box_x + l.id_box_w,
            bottom: l.id_box_y + 6 + f_badge + 4,
        };
        let _ = DrawTextW(mem_dc, &mut id_label, &mut id_label_rect, DT_SINGLELINE | DT_VCENTER);

        let current_id = {
            let guard = LOCAL_DESKY_ID.lock().unwrap();
            if !guard.is_empty() {
                guard.clone()
            } else {
                crate::identity::load_or_create_identity()
                    .desky_id
                    .unwrap_or_else(|| "Connecting...".into())
            }
        };
        let _ = SelectObject(mem_dc, HGDIOBJ(font_id.0));
        SetTextColor(mem_dc, PK_GREEN);
        let mut id_wide: Vec<u16> = current_id.encode_utf16().collect();
        let mut id_rect = RECT {
            left: l.id_box_x + 14,
            top: l.id_box_y + 4 + f_badge,
            right: l.id_box_x + l.id_box_w,
            bottom: l.id_box_y + l.id_box_h,
        };
        let _ = DrawTextW(mem_dc, &mut id_wide, &mut id_rect, DT_SINGLELINE | DT_VCENTER);

        let copy_bg = if hover == 1 { PK_BTN_HOVER } else { PK_CARD };
        let copy_border = if hover == 1 { PK_GREEN } else { PK_CARD_BORDER };
        draw_rounded_box(mem_dc, l.copy_btn_x, l.copy_btn_y, l.copy_btn_w, l.copy_btn_h, copy_bg, copy_border, 8);
        let copy_fg = if hover == 1 { PK_GREEN_DARK } else { PK_TEXT };

        let _ = SelectObject(mem_dc, HGDIOBJ(font_btn.0));
        SetTextColor(mem_dc, copy_fg);
        let mut copy_text: Vec<u16> = "Copy ID".encode_utf16().collect();
        let mut copy_sz = SIZE::default();
        let _ = GetTextExtentPoint32W(mem_dc, &copy_text, &mut copy_sz);

        let icon_sz = 16;
        let gap = 8;
        let copy_total_w = copy_sz.cx + gap + icon_sz;
        let copy_start_x = l.copy_btn_x + (l.copy_btn_w - copy_total_w) / 2;
        let copy_icon_y = l.copy_btn_y + (l.copy_btn_h - icon_sz) / 2;

        crate::ui::draw_copy(mem_dc, copy_start_x, copy_icon_y, icon_sz, copy_fg, 2);

        let mut copy_rect = RECT {
            left: copy_start_x + icon_sz + gap,
            top: l.copy_btn_y,
            right: copy_start_x + copy_total_w + 4,
            bottom: l.copy_btn_y + l.copy_btn_h,
        };
        let _ = DrawTextW(mem_dc, &mut copy_text, &mut copy_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

        // 2. Card 2: REMOTE DESK (Right)
        draw_rounded_box(mem_dc, l.c2_x, l.card_y, l.card_w, l.card_h, PK_CARD, PK_CARD_BORDER, 12);

        let tag2_w = (110.0 * l.font_scale) as i32;
        let tag2_h = (22.0 * l.font_scale) as i32;
        draw_rounded_box(mem_dc, l.c2_x + 18, l.card_y + 14, tag2_w, tag2_h, PK_INPUT_BG, PK_CARD_BORDER, 6);
        let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
        SetTextColor(mem_dc, PK_MUTED);
        let mut tag2: Vec<u16> = "REMOTE DESK".encode_utf16().collect();
        let mut tag2_rect = RECT {
            left: l.c2_x + 18,
            top: l.card_y + 14,
            right: l.c2_x + 18 + tag2_w,
            bottom: l.card_y + 14 + tag2_h,
        };
        let _ = DrawTextW(mem_dc, &mut tag2, &mut tag2_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

        draw_rounded_box(mem_dc, l.input_box_x, l.input_box_y, l.input_box_w, l.input_box_h, PK_INPUT_BG, PK_CARD_BORDER, 8);

        let conn_bg = if hover == 2 { PK_GREEN_DARK } else { PK_GREEN };
        draw_rounded_box(mem_dc, l.conn_btn_x, l.conn_btn_y, l.conn_btn_w, l.conn_btn_h, conn_bg, conn_bg, 8);

        let _ = SelectObject(mem_dc, HGDIOBJ(font_btn.0));
        SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
        let mut conn_text: Vec<u16> = "Connect".encode_utf16().collect();
        let mut conn_sz = SIZE::default();
        let _ = GetTextExtentPoint32W(mem_dc, &conn_text, &mut conn_sz);

        let conn_total_w = conn_sz.cx + gap + icon_sz;
        let conn_start_x = l.conn_btn_x + (l.conn_btn_w - conn_total_w) / 2;
        let conn_icon_y = l.conn_btn_y + (l.conn_btn_h - icon_sz) / 2;

        let mut conn_rect = RECT {
            left: conn_start_x,
            top: l.conn_btn_y,
            right: conn_start_x + conn_sz.cx,
            bottom: l.conn_btn_y + l.conn_btn_h,
        };
        let _ = DrawTextW(mem_dc, &mut conn_text, &mut conn_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

        let conn_icon_x = conn_start_x + conn_sz.cx + gap;
        crate::ui::draw_arrow_right(mem_dc, conn_icon_x, conn_icon_y, icon_sz, COLORREF(0x00FFFFFF), 2);

        // 3. Card 3: RECENT SESSIONS & LAN RADAR (Bottom Left)
        draw_rounded_box(mem_dc, l.col1_x, l.col_y, l.col_w, l.col_h, PK_CARD, PK_CARD_BORDER, 12);

        let is_radar = BOTTOM_TAB_RADAR.load(Ordering::Relaxed);
        let lan_peers = crate::p2p::get_lan_peers();

        // Tag 1: RECENT SESSIONS
        let tag3_w = (124.0 * l.font_scale) as i32;
        let tag3_h = (22.0 * l.font_scale) as i32;
        let (t1_bg, t1_border, t1_fg) = if !is_radar {
            (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
        } else {
            (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
        };
        draw_rounded_box(mem_dc, l.col1_x + 18, l.col_y + 14, tag3_w, tag3_h, t1_bg, t1_border, 6);
        let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
        SetTextColor(mem_dc, t1_fg);
        let mut tag3: Vec<u16> = "RECENT SESSIONS".encode_utf16().collect();
        let mut tag3_rect = RECT {
            left: l.col1_x + 18,
            top: l.col_y + 14,
            right: l.col1_x + 18 + tag3_w,
            bottom: l.col_y + 14 + tag3_h,
        };
        let _ = DrawTextW(mem_dc, &mut tag3, &mut tag3_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

        // Tag 2: LAN RADAR (with peer count)
        let tag_radar_x = l.col1_x + 18 + tag3_w + 8;
        let tag_radar_w = (116.0 * l.font_scale) as i32;
        let (t2_bg, t2_border, t2_fg) = if is_radar {
            (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
        } else {
            (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
        };
        draw_rounded_box(mem_dc, tag_radar_x, l.col_y + 14, tag_radar_w, tag3_h, t2_bg, t2_border, 6);
        SetTextColor(mem_dc, t2_fg);
        let radar_tag_text = if !lan_peers.is_empty() {
            format!("LAN RADAR ({})", lan_peers.len())
        } else {
            "LAN RADAR".to_string()
        };
        let mut tag_radar_utf16: Vec<u16> = radar_tag_text.encode_utf16().collect();
        let mut tag_radar_rc = RECT {
            left: tag_radar_x,
            top: l.col_y + 14,
            right: tag_radar_x + tag_radar_w,
            bottom: l.col_y + 14 + tag3_h,
        };
        let _ = DrawTextW(mem_dc, &mut tag_radar_utf16, &mut tag_radar_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

        // When in Radar mode, show Scan button at top-right
        if is_radar {
            let scan_w = 54;
            let scan_x = l.col1_x + l.col_w - scan_w - 18;
            draw_rounded_box(mem_dc, scan_x, l.col_y + 14, scan_w, tag3_h, PK_INPUT_BG, PK_CARD_BORDER, 6);
            SetTextColor(mem_dc, PK_MUTED);
            let mut scan_txt: Vec<u16> = "Scan".encode_utf16().collect();
            let mut scan_rc = RECT {
                left: scan_x,
                top: l.col_y + 14,
                right: scan_x + scan_w,
                bottom: l.col_y + 14 + tag3_h,
            };
            let _ = DrawTextW(mem_dc, &mut scan_txt, &mut scan_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
        }

        if is_radar {
            // Render LAN Radar View
            if lan_peers.is_empty() {
                crate::ui::draw_radio(mem_dc, l.col1_x + 22, l.col_y + 50, 18, PK_GREEN, 2);

                let _ = SelectObject(mem_dc, HGDIOBJ(font_btn.0));
                SetTextColor(mem_dc, PK_TEXT);
                let mut empty_msg1: Vec<u16> = "Zero-Config LAN Radar active".encode_utf16().collect();
                let mut empty_rc1 = RECT {
                    left: l.col1_x + 48,
                    top: l.col_y + 48,
                    right: l.col1_x + l.col_w - 20,
                    bottom: l.col_y + 70,
                };
                let _ = DrawTextW(mem_dc, &mut empty_msg1, &mut empty_rc1, DT_SINGLELINE | DT_VCENTER);

                let _ = SelectObject(mem_dc, HGDIOBJ(font_console.0));
                SetTextColor(mem_dc, PK_MUTED);
                let mut empty_msg2: Vec<u16> =
                    "Desky instances on your local Wi-Fi or subnet appear here with 1-click zero-cloud connect."
                        .encode_utf16()
                        .collect();
                let mut empty_rc2 = RECT {
                    left: l.col1_x + 20,
                    top: l.col_y + 74,
                    right: l.col1_x + l.col_w - 20,
                    bottom: l.col_y + 114,
                };
                let _ = DrawTextW(mem_dc, &mut empty_msg2, &mut empty_rc2, DT_WORDBREAK);
            } else {
                let item_h = (44.0 * l.font_scale) as i32;
                let item_w = l.col_w - 36;
                for (idx, peer) in lan_peers.iter().take(2).enumerate() {
                    let item_x = l.col1_x + 18;
                    let item_y = l.col_y + 46 + (idx as i32 * (item_h + 8));

                    draw_rounded_box(mem_dc, item_x, item_y, item_w, item_h, PK_INPUT_BG, PK_CARD_BORDER, 8);

                    crate::ui::draw_radio(mem_dc, item_x + 10, item_y + 12, 18, PK_GREEN, 2);

                    let _ = SelectObject(mem_dc, HGDIOBJ(font_btn.0));
                    SetTextColor(mem_dc, PK_TEXT);
                    let title_text = if !peer.user.is_empty() {
                        format!("{} ({})", peer.hostname, peer.user)
                    } else {
                        peer.hostname.clone()
                    };
                    let mut title_txt: Vec<u16> = title_text.encode_utf16().collect();
                    let mut title_rc = RECT {
                        left: item_x + 36,
                        top: item_y + 4,
                        right: item_x + item_w - 86,
                        bottom: item_y + 24,
                    };
                    let _ = DrawTextW(mem_dc, &mut title_txt, &mut title_rc, DT_SINGLELINE | DT_VCENTER);

                    let _ = SelectObject(mem_dc, HGDIOBJ(font_console.0));
                    SetTextColor(mem_dc, PK_MUTED);
                    let sub_text = format!(
                        "ID: {} | <{}ms | {}",
                        peer.id,
                        peer.latency_ms,
                        peer.ip.map(|i| i.to_string()).unwrap_or_default()
                    );
                    let mut sub_txt: Vec<u16> = sub_text.encode_utf16().collect();
                    let mut sub_rc = RECT {
                        left: item_x + 36,
                        top: item_y + 24,
                        right: item_x + item_w - 86,
                        bottom: item_y + item_h - 4,
                    };
                    let _ = DrawTextW(mem_dc, &mut sub_txt, &mut sub_rc, DT_SINGLELINE | DT_VCENTER);

                    let pill_btn_w = 72;
                    let pill_btn_h = 28;
                    let pill_btn_x = item_x + item_w - pill_btn_w - 6;
                    let pill_btn_y = item_y + (item_h - pill_btn_h) / 2;
                    draw_rounded_box(mem_dc, pill_btn_x, pill_btn_y, pill_btn_w, pill_btn_h, PK_GREEN_TINT, PK_GREEN, 6);

                    let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
                    SetTextColor(mem_dc, PK_GREEN);
                    let mut c_txt: Vec<u16> = "Connect".encode_utf16().collect();
                    let mut c_rc = RECT {
                        left: pill_btn_x,
                        top: pill_btn_y,
                        right: pill_btn_x + pill_btn_w,
                        bottom: pill_btn_y + pill_btn_h,
                    };
                    let _ = DrawTextW(mem_dc, &mut c_txt, &mut c_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
                }
            }
        } else {
            // Render Recent Sessions View
            let recent_sessions = crate::identity::get_recent_sessions();
            if recent_sessions.is_empty() {
                let _ = SelectObject(mem_dc, HGDIOBJ(font_console.0));
                SetTextColor(mem_dc, PK_MUTED);
                let mut empty_msg1: Vec<u16> = "No recent sessions yet.".encode_utf16().collect();
                let mut empty_rc1 = RECT {
                    left: l.col1_x + 20,
                    top: l.col_y + 48,
                    right: l.col1_x + l.col_w - 20,
                    bottom: l.col_y + 70,
                };
                let _ = DrawTextW(mem_dc, &mut empty_msg1, &mut empty_rc1, DT_SINGLELINE | DT_VCENTER);

                let mut empty_msg2: Vec<u16> =
                    "Remote desks you connect to will appear here for fast 1-click reconnect."
                        .encode_utf16()
                        .collect();
                let mut empty_rc2 = RECT {
                    left: l.col1_x + 20,
                    top: l.col_y + 72,
                    right: l.col1_x + l.col_w - 20,
                    bottom: l.col_y + 112,
                };
                let _ = DrawTextW(mem_dc, &mut empty_msg2, &mut empty_rc2, DT_WORDBREAK);
            } else {
                let item_h = (36.0 * l.font_scale) as i32;
                let item_w = l.col_w - 36;
                for (idx, session) in recent_sessions.iter().take(2).enumerate() {
                    let item_x = l.col1_x + 18;
                    let item_y = l.col_y + 46 + (idx as i32 * (item_h + 8));

                    draw_rounded_box(mem_dc, item_x, item_y, item_w, item_h, PK_INPUT_BG, PK_CARD_BORDER, 8);

                    let _ = SelectObject(mem_dc, HGDIOBJ(font_btn.0));
                    SetTextColor(mem_dc, PK_TEXT);
                    let mut id_txt: Vec<u16> = session.encode_utf16().collect();
                    let mut id_rc = RECT {
                        left: item_x + 14,
                        top: item_y,
                        right: item_x + item_w - 86,
                        bottom: item_y + item_h,
                    };
                    let _ = DrawTextW(mem_dc, &mut id_txt, &mut id_rc, DT_SINGLELINE | DT_VCENTER);

                    let pill_btn_w = 72;
                    let pill_btn_h = item_h - 10;
                    let pill_btn_x = item_x + item_w - pill_btn_w - 6;
                    let pill_btn_y = item_y + 5;
                    draw_rounded_box(mem_dc, pill_btn_x, pill_btn_y, pill_btn_w, pill_btn_h, PK_GREEN_TINT, PK_GREEN, 6);

                    let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
                    SetTextColor(mem_dc, PK_GREEN);
                    let mut c_txt: Vec<u16> = "Connect".encode_utf16().collect();
                    let mut c_rc = RECT {
                        left: pill_btn_x,
                        top: pill_btn_y,
                        right: pill_btn_x + pill_btn_w,
                        bottom: pill_btn_y + pill_btn_h,
                    };
                    let _ = DrawTextW(mem_dc, &mut c_txt, &mut c_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
                }
            }
        }

        // 4. Card 4: SECURITY & SERVER (Bottom Right)
        draw_rounded_box(mem_dc, l.col2_x, l.col_y, l.col_w, l.col_h, PK_CARD, PK_CARD_BORDER, 12);

        let tag4_w = (144.0 * l.font_scale) as i32;
        let tag4_h = (22.0 * l.font_scale) as i32;
        draw_rounded_box(mem_dc, l.col2_x + 18, l.col_y + 14, tag4_w, tag4_h, PK_INPUT_BG, PK_CARD_BORDER, 6);
        let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
        SetTextColor(mem_dc, PK_MUTED);
        let mut tag4: Vec<u16> = "SECURITY & SERVER".encode_utf16().collect();
        let mut tag4_rect = RECT {
            left: l.col2_x + 18,
            top: l.col_y + 14,
            right: l.col2_x + 18 + tag4_w,
            bottom: l.col_y + 14 + tag4_h,
        };
        let _ = DrawTextW(mem_dc, &mut tag4, &mut tag4_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

        let console_box_x = l.col2_x + 18;
        let console_box_y = l.col_y + 46;
        let console_box_w = l.col_w - 36;
        let console_box_h = l.col_h - 58;

        draw_rounded_box(
            mem_dc,
            console_box_x,
            console_box_y,
            console_box_w,
            console_box_h,
            COLORREF(0x00F8FAF8),
            PK_CARD_BORDER,
            8,
        );

        let _ = SelectObject(mem_dc, HGDIOBJ(font_console.0));
        SetTextColor(mem_dc, PK_TEXT);

        let logs = STATUS_LOGS.lock().unwrap();
        let line_height = (18.0 * l.font_scale) as i32;
        let max_visible_lines = ((console_box_h - 16) / line_height).max(1) as usize;

        let display_logs: Vec<String> = if logs.len() > max_visible_lines {
            logs[logs.len() - max_visible_lines..].to_vec()
        } else {
            logs.clone()
        };

        for (i, log) in display_logs.iter().enumerate() {
            let mut log_wide: Vec<u16> = log.encode_utf16().collect();
            let mut line_rect = RECT {
                left: console_box_x + 12,
                top: console_box_y + 8 + (i as i32 * line_height),
                right: console_box_x + console_box_w - 12,
                bottom: console_box_y + 8 + ((i + 1) as i32 * line_height),
            };
            let _ = DrawTextW(
                mem_dc,
                &mut log_wide,
                &mut line_rect,
                DT_SINGLELINE | DT_VCENTER,
            );
        }
    }

    let _ = DeleteObject(HGDIOBJ(font_sub.0));
    let _ = DeleteObject(HGDIOBJ(font_badge.0));
    let _ = DeleteObject(HGDIOBJ(font_id.0));
    let _ = DeleteObject(HGDIOBJ(font_btn.0));
    let _ = DeleteObject(HGDIOBJ(font_console.0));
}

/// Handles mouse clicks on the Homepage view
pub unsafe fn handle_homepage_click(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) -> bool {
    // 1. Pending Approval Modal clicks
    if crate::p2p::get_pending_approval().is_some() {
        let mw = (w - 64).clamp(440, 620);
        let mh = 290;
        let mx = (w - mw) / 2;
        let my = ((h - mh) / 2).max(68);
        let btn_w = (mw - 72) / 2;
        let btn_h = 50;
        let btn_y = my + 190;
        let acc_x = mx + 24;
        let dec_x = mx + 48 + btn_w;

        if x >= acc_x && x <= acc_x + btn_w && y >= btn_y && y <= btn_y + btn_h {
            crate::p2p::respond_to_pending_approval(true);
            let _ = InvalidateRect(Some(hwnd), None, false);
            return true;
        } else if x >= dec_x && x <= dec_x + btn_w && y >= btn_y && y <= btn_y + btn_h {
            crate::p2p::respond_to_pending_approval(false);
            let _ = InvalidateRect(Some(hwnd), None, false);
            return true;
        }
        return false;
    }

    let l = get_responsive_layout(w, h);

    // 2. Copy Button Clicked
    if x >= l.copy_btn_x
        && x <= l.copy_btn_x + l.copy_btn_w
        && y >= l.copy_btn_y
        && y <= l.copy_btn_y + l.copy_btn_h
    {
        let id_to_copy = {
            let guard = LOCAL_DESKY_ID.lock().unwrap();
            if !guard.is_empty() {
                guard.clone()
            } else {
                crate::identity::load_or_create_identity()
                    .desky_id
                    .unwrap_or_default()
            }
        };

        if !id_to_copy.is_empty() {
            copy_to_clipboard(&id_to_copy);
            append_status_log(&format!("Desky ID '{}' copied to Windows Clipboard!", id_to_copy));
        }
        return true;
    }

    // 3. Connect Button Clicked
    if x >= l.conn_btn_x
        && x <= l.conn_btn_x + l.conn_btn_w
        && y >= l.conn_btn_y
        && y <= l.conn_btn_y + l.conn_btn_h
    {
        handle_homepage_connect(hwnd);
        return true;
    }

    // 4. Card 3 Header Tabs: RECENT SESSIONS vs LAN RADAR
    let tag3_w = (124.0 * l.font_scale) as i32;
    let tag3_h = (22.0 * l.font_scale) as i32;
    let tag_radar_x = l.col1_x + 18 + tag3_w + 8;
    let tag_radar_w = (116.0 * l.font_scale) as i32;
    let is_radar = BOTTOM_TAB_RADAR.load(Ordering::Relaxed);

    if y >= l.col_y + 14 && y <= l.col_y + 14 + tag3_h {
        if x >= l.col1_x + 18 && x <= l.col1_x + 18 + tag3_w {
            BOTTOM_TAB_RADAR.store(false, Ordering::Relaxed);
            let _ = InvalidateRect(Some(hwnd), None, false);
            return true;
        }
        if x >= tag_radar_x && x <= tag_radar_x + tag_radar_w {
            BOTTOM_TAB_RADAR.store(true, Ordering::Relaxed);
            crate::p2p::trigger_lan_ping();
            let _ = InvalidateRect(Some(hwnd), None, false);
            return true;
        }
        if is_radar {
            let scan_w = 54;
            let scan_x = l.col1_x + l.col_w - scan_w - 18;
            if x >= scan_x && x <= scan_x + scan_w {
                crate::p2p::trigger_lan_ping();
                append_status_log("LAN Radar: Scanning local network...");
                let _ = InvalidateRect(Some(hwnd), None, false);
                return true;
            }
        }
    }

    // 5. Card 3 Content: LAN Radar peers or Recent Sessions
    if is_radar {
        let lan_peers = crate::p2p::get_lan_peers();
        let item_h = (44.0 * l.font_scale) as i32;
        let item_w = l.col_w - 36;
        for (idx, peer) in lan_peers.iter().take(2).enumerate() {
            let item_x = l.col1_x + 18;
            let item_y = l.col_y + 46 + (idx as i32 * (item_h + 8));

            if x >= item_x && x <= item_x + item_w && y >= item_y && y <= item_y + item_h {
                let wide_id: Vec<u16> = peer.id.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = SetWindowTextW(HWND_EDIT_TARGET, PCWSTR(wide_id.as_ptr()));

                let pill_btn_w = 72;
                let pill_btn_x = item_x + item_w - pill_btn_w - 6;
                if x >= pill_btn_x {
                    let target_display = peer.id.clone();
                    info!("Initiating direct LAN P2P session to: {} ({})", target_display, peer.hostname);
                    crate::identity::save_recent_session(&target_display);
                    append_status_log(&format!(
                        "Initiating LAN connection to {} ({}) at {}...",
                        peer.hostname,
                        target_display,
                        peer.ip.map(|i| i.to_string()).unwrap_or_default()
                    ));
                    super::viewport::spawn_or_add_tab_viewport(&target_display);
                } else {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return true;
            }
        }
    } else {
        let recents = crate::identity::get_recent_sessions();
        let item_h = (36.0 * l.font_scale) as i32;
        let item_w = l.col_w - 36;
        for (idx, session) in recents.iter().take(2).enumerate() {
            let item_x = l.col1_x + 18;
            let item_y = l.col_y + 46 + (idx as i32 * (item_h + 8));

            if x >= item_x && x <= item_x + item_w && y >= item_y && y <= item_y + item_h {
                let wide_id: Vec<u16> = session.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = SetWindowTextW(HWND_EDIT_TARGET, PCWSTR(wide_id.as_ptr()));

                let pill_btn_w = 72;
                let pill_btn_x = item_x + item_w - pill_btn_w - 6;
                if x >= pill_btn_x {
                    let target_display = session.clone();
                    info!("Initiating P2P session to remote Desky ID: {}", target_display);
                    crate::identity::save_recent_session(&target_display);
                    append_status_log(&format!(
                        "Requesting session with Remote Desky ID: {}...",
                        target_display
                    ));

                    super::viewport::spawn_or_add_tab_viewport(&target_display);
                } else {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return true;
            }
        }
    }

    false
}

/// Validates entered remote Desky ID and initiates connection in the unified window
pub unsafe fn handle_homepage_connect(hwnd: HWND) {
    let mut buffer = [0u16; 128];
    let len = GetWindowTextLengthW(HWND_EDIT_TARGET) as usize;
    let raw_input = if len > 0 {
        let _ = GetWindowTextW(HWND_EDIT_TARGET, &mut buffer);
        String::from_utf16_lossy(&buffer[..len])
    } else {
        String::new()
    };

    let digits: String = raw_input.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        append_status_log("[!] Please enter a 9-digit remote Desky ID (e.g. 800 258 756)");
        let _ = InvalidateRect(Some(hwnd), None, false);
        return;
    }

    let local_id_digits: String = {
        let guard = LOCAL_DESKY_ID.lock().unwrap();
        let s = if !guard.is_empty() {
            guard.clone()
        } else {
            crate::identity::load_or_create_identity()
                .desky_id
                .unwrap_or_default()
        };
        s.chars().filter(|c| c.is_ascii_digit()).collect()
    };

    if !local_id_digits.is_empty() && digits == local_id_digits {
        append_status_log("[!] That is THIS computer's own ID! Please enter the remote computer's ID.");
        let _ = InvalidateRect(Some(hwnd), None, false);
        return;
    }

    let target_display = if digits.len() == 9 {
        format!("{} {} {}", &digits[0..3], &digits[3..6], &digits[6..9])
    } else {
        digits.clone()
    };

    info!("Initiating P2P session to remote Desky ID: {}", target_display);
    crate::identity::save_recent_session(&target_display);
    append_status_log(&format!(
        "Requesting session with Remote Desky ID: {}...",
        target_display
    ));

    super::viewport::spawn_or_add_tab_viewport(&target_display);
}

/// Tracks hover state for Homepage buttons and changes cursor
pub unsafe fn handle_homepage_mousemove(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
    let new_hover = if crate::p2p::get_pending_approval().is_some() {
        let mw = (w - 64).clamp(440, 620);
        let mh = 290;
        let mx = (w - mw) / 2;
        let my = ((h - mh) / 2).max(68);
        let btn_w = (mw - 72) / 2;
        let btn_h = 50;
        let btn_y = my + 190;
        let acc_x = mx + 24;
        let dec_x = mx + 48 + btn_w;

        if x >= acc_x && x <= acc_x + btn_w && y >= btn_y && y <= btn_y + btn_h {
            3
        } else if x >= dec_x && x <= dec_x + btn_w && y >= btn_y && y <= btn_y + btn_h {
            4
        } else {
            0
        }
    } else {
        let l = get_responsive_layout(w, h);
        if x >= l.copy_btn_x
            && x <= l.copy_btn_x + l.copy_btn_w
            && y >= l.copy_btn_y
            && y <= l.copy_btn_y + l.copy_btn_h
        {
            1
        } else if x >= l.conn_btn_x
            && x <= l.conn_btn_x + l.conn_btn_w
            && y >= l.conn_btn_y
            && y <= l.conn_btn_y + l.conn_btn_h
        {
            2
        } else {
            0
        }
    };

    let prev_hover = HOVER_BUTTON.swap(new_hover, Ordering::Relaxed);
    if new_hover != prev_hover {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }

    if new_hover != 0 {
        let _ = SetCursor(Some(LoadCursorW(None, IDC_HAND).unwrap()));
    } else {
        let _ = SetCursor(Some(LoadCursorW(None, IDC_ARROW).unwrap()));
    }
}
