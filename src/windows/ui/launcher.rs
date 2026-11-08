//! Main Win32 Launcher Window & Application Lifecycle

#![allow(unsafe_op_in_unsafe_fn)]
use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU8, Ordering};
use tracing::info;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush,
    DeleteObject, DrawTextW, EndPaint, FillRect, InvalidateRect, SelectObject, SetBkColor,
    SetBkMode, SetTextColor, SRCCOPY, DT_CENTER, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK,
    FW_BOLD, FW_NORMAL, FW_SEMIBOLD, HBRUSH, HDC, HGDIOBJ, OPAQUE, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect,
    GetMessageW, GetWindowTextLengthW, GetWindowTextW, IsIconic, IsWindow, KillTimer, LoadCursorW,
    PostMessageW, PostQuitMessage, RegisterClassExW, SendMessageW, SetCursor,
    SetForegroundWindow, SetTimer, SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, HMENU, IDC_ARROW, IDC_HAND, MINMAXINFO, MSG,
    SWP_NOACTIVATE, SWP_NOZORDER, SW_HIDE, SW_RESTORE, SW_SHOW, WINDOW_EX_STYLE, WM_CREATE,
    WM_CTLCOLOREDIT, WM_DESTROY, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_LBUTTONDOWN,
    WM_MOUSEMOVE, WM_PAINT, WM_SETFONT, WM_SIZE, WM_TIMER,
    WNDCLASSEXW, WS_CHILD, WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_VISIBLE,
};

use super::progress::spawn_progress_window;
use super::theme::{
    copy_to_clipboard, create_segoe_font, draw_rounded_box, PK_BG, PK_BTN_HOVER, PK_CARD,
    PK_CARD_BORDER, PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_RED, PK_TEXT,
};
use super::viewport::{
    spawn_or_add_tab_viewport, viewport_wndproc, CAPTURE_CURSOR, TARGET_FPS, VIEWPORT_BRUSH,
    VIEWPORT_RUNNING,
};

pub const WM_APP_REDRAW: u32 = 0x8001;
pub const WM_APP_BRING_FRONT: u32 = 0x8002;
pub const WM_APP_SHOW_PROGRESS: u32 = 0x8004;

const ID_EDIT_TARGET_CODE: usize = 1003;

static IS_RUNNING: AtomicBool = AtomicBool::new(false);

pub(crate) static mut HWND_MAIN_WINDOW: HWND = HWND(std::ptr::null_mut());
static mut HWND_EDIT_TARGET: HWND = HWND(std::ptr::null_mut());
static mut BG_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());
static mut CARD_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());
static mut EDIT_BG_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());

static HOVER_BUTTON: AtomicU8 = AtomicU8::new(0); // 0 = none, 1 = copy, 2 = connect, 3 = accept, 4 = decline
static CURRENT_EDIT_FONT: AtomicIsize = AtomicIsize::new(0);

pub static LOCAL_DESKY_ID: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
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

pub fn set_assigned_desky_id(id: &str) {
    if let Ok(mut guard) = LOCAL_DESKY_ID.lock() {
        *guard = id.to_string();
    }
    append_status_log(&format!("âœ… Online â€¢ ID: {}", id));
    request_main_redraw();
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

/// Computes responsive layout geometry for any window size
fn get_responsive_layout(w: i32, h: i32) -> ResponsiveLayout {
    let margin = 24;
    let gap = 18;
    let card_w = (w - (2 * margin) - gap).max(220) / 2;

    let scale_w = w as f32 / 760.0;
    let scale_h = h as f32 / 580.0;
    let font_scale = (scale_w.min(scale_h)).clamp(1.0, 1.6);

    let card_h = ((192.0 * font_scale) as i32).min((h - 160) / 2).max(185);
    let card_y = ((68.0 * font_scale) as i32).max(64);

    let c1_x = margin;
    let c2_x = margin + card_w + gap;

    // Card 1: This Desk elements
    let id_box_x = c1_x + 18;
    let id_box_y = card_y + (48.0 * font_scale) as i32;
    let id_box_w = card_w - 36;
    let id_box_h = (68.0 * font_scale) as i32;

    let copy_btn_x = c1_x + 18;
    let copy_btn_y = id_box_y + id_box_h + (14.0 * font_scale) as i32;
    let copy_btn_w = card_w - 36;
    let copy_btn_h = (44.0 * font_scale) as i32;

    // Card 2: Remote Desk elements
    let input_box_x = c2_x + 18;
    let input_box_y = card_y + (48.0 * font_scale) as i32;
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
    let col_h = (h - col_y - margin).max(130);
    let col1_x = c1_x;
    let col2_x = c2_x;

    ResponsiveLayout {
        c1_x, c2_x, card_w, card_h, card_y,
        id_box_x, id_box_y, id_box_w, id_box_h,
        copy_btn_x, copy_btn_y, copy_btn_w, copy_btn_h,
        input_box_x, input_box_y, input_box_w, input_box_h,
        edit_x, edit_y, edit_w, edit_h,
        conn_btn_x, conn_btn_y, conn_btn_w, conn_btn_h,
        col1_x, col2_x, col_w, col_h, col_y,
        font_scale,
    }
}

pub struct NativeWin32App;

impl NativeWin32App {
    pub fn new() -> Result<Self> {
        unsafe {
            let h_module = GetModuleHandleW(None)?;
            let h_instance: HINSTANCE = h_module.into();

            BG_BRUSH = CreateSolidBrush(PK_BG);
            CARD_BRUSH = CreateSolidBrush(PK_CARD);
            VIEWPORT_BRUSH = CreateSolidBrush(COLORREF(0x000C0A09));
            EDIT_BG_BRUSH = CreateSolidBrush(PK_INPUT_BG);

            // 1. Register Main Launcher Window Class
            let class_name = w!("DeskyWindowClass");
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: h_instance,
                hIcon: Default::default(),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                hbrBackground: BG_BRUSH,
                lpszMenuName: PCWSTR::null(),
                lpszClassName: class_name,
                hIconSm: Default::default(),
            };
            RegisterClassExW(&wc);

            // 2. Register Viewport Window Class
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
                hbrBackground: VIEWPORT_BRUSH,
                lpszMenuName: PCWSTR::null(),
                lpszClassName: viewport_class_name,
                hIconSm: Default::default(),
            };
            RegisterClassExW(&vwc);

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                w!("Desky â€¢ pkasoft.com"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                760,
                580,
                None,
                None,
                Some(h_instance),
                None,
            )?;
            HWND_MAIN_WINDOW = hwnd;

            // Light theme titlebar for launcher window
            let dark_mode: i32 = 0;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark_mode as *const _ as *const _,
                std::mem::size_of::<i32>() as u32,
            );

            info!("Native Win32 Launcher Window spawned successfully (HWND: {:?})", hwnd);

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

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            unsafe {
                HWND_MAIN_WINDOW = hwnd;
                let h_module = GetModuleHandleW(None).unwrap_or_default();
                let h_instance: HINSTANCE = h_module.into();

                {
                    let mut logs = STATUS_LOGS.lock().unwrap();
                    if logs.is_empty() {
                        logs.push("â€¢ Ready to connect".into());
                    }
                }

                let l = get_responsive_layout(760, 580);

                HWND_EDIT_TARGET = CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("EDIT"),
                    w!(""),
                    WS_VISIBLE | WS_CHILD | WS_TABSTOP,
                    l.edit_x,
                    l.edit_y,
                    l.edit_w,
                    l.edit_h,
                    Some(hwnd),
                    Some(HMENU(ID_EDIT_TARGET_CODE as *mut _)),
                    Some(h_instance),
                    None,
                ).unwrap();

                let font_edit = create_segoe_font(20, FW_SEMIBOLD);
                CURRENT_EDIT_FONT.store(font_edit.0 as isize, Ordering::Relaxed);
                let _ = SendMessageW(
                    HWND_EDIT_TARGET,
                    WM_SETFONT,
                    Some(WPARAM(font_edit.0 as usize)),
                    Some(LPARAM(1)),
                );

                // Auto-refresh timer every 2 seconds for live ping & online/offline status
                let _ = SetTimer(Some(hwnd), 1001, 2000, None);
            }
            LRESULT(0)
        }

        WM_TIMER => {
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }

        WM_GETMINMAXINFO => {
            unsafe {
                if lparam.0 != 0 {
                    let mmi = &mut *(lparam.0 as *mut MINMAXINFO);
                    mmi.ptMinTrackSize.x = 720;
                    mmi.ptMinTrackSize.y = 540;
                }
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),

        WM_SIZE => {
            let w = (lparam.0 & 0xffff) as i16 as i32;
            let h = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            if w > 100 {
                let l = get_responsive_layout(w, h);
                unsafe {
                    if !HWND_EDIT_TARGET.0.is_null() && IsWindow(Some(HWND_EDIT_TARGET)).as_bool() {
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
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }

        WM_PAINT => {
            unsafe {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);

                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let w = rc.right - rc.left;
                let h = rc.bottom - rc.top;

                if w > 100 && h > 100 {
                    let pending_req = crate::p2p::get_pending_approval();
                    if !HWND_EDIT_TARGET.0.is_null() && IsWindow(Some(HWND_EDIT_TARGET)).as_bool() {
                        let _ = ShowWindow(
                            HWND_EDIT_TARGET,
                            if pending_req.is_some() { SW_HIDE } else { SW_SHOW },
                        );
                    }

                    let mem_dc = CreateCompatibleDC(Some(hdc));
                    let mem_bmp = CreateCompatibleBitmap(hdc, w, h);
                    let old_bmp = SelectObject(mem_dc, HGDIOBJ(mem_bmp.0));

                    FillRect(mem_dc, &rc, BG_BRUSH);
                    SetBkMode(mem_dc, TRANSPARENT);

                    let l = get_responsive_layout(w, h);
                    let f_title = ((26.0 * l.font_scale) as i32).max(24);
                    let f_sub = ((14.0 * l.font_scale) as i32).max(13);
                    let f_badge = ((13.0 * l.font_scale) as i32).max(12);
                    let f_id = ((34.0 * l.font_scale) as i32).max(28);
                    let f_btn = ((16.0 * l.font_scale) as i32).max(14);
                    let f_console = ((13.0 * l.font_scale) as i32).max(12);

                    let font_title = create_segoe_font(f_title, FW_BOLD);
                    let font_sub = create_segoe_font(f_sub, FW_NORMAL);
                    let font_badge = create_segoe_font(f_badge, FW_BOLD);
                    let font_id = create_segoe_font(f_id, FW_BOLD);
                    let font_btn = create_segoe_font(f_btn, FW_BOLD);
                    let font_console = create_segoe_font(f_console, FW_NORMAL);

                    // Header
                    let old_font = SelectObject(mem_dc, HGDIOBJ(font_title.0));
                    SetTextColor(mem_dc, PK_GREEN_DARK);
                    let mut title_text: Vec<u16> = "DESKY".encode_utf16().collect();
                    let mut title_rect = RECT { left: 24, top: 14, right: 260, bottom: 14 + f_title + 6 };
                    let _ = DrawTextW(mem_dc, &mut title_text, &mut title_rect, DT_SINGLELINE | DT_VCENTER);

                    let is_online = crate::p2p::is_server_online();
                    let ping = crate::p2p::get_server_ping_ms();
                    let (pill_text_str, pill_bg, pill_fg) = if is_online {
                        let s = if ping > 0 {
                            format!("â— Online ({}ms)", ping)
                        } else {
                            "â— Online".to_string()
                        };
                        (s, PK_GREEN_TINT, PK_GREEN)
                    } else {
                        ("â— Offline".to_string(), COLORREF(0x00EBF0FF), COLORREF(0x002B37DF))
                    };

                    let pill_w = (118.0 * l.font_scale) as i32;
                    let pill_h = (30.0 * l.font_scale) as i32;
                    let pill_x = (w - pill_w - 24).max(24);
                    draw_rounded_box(mem_dc, pill_x, 16, pill_w, pill_h, pill_bg, PK_CARD_BORDER, pill_h / 2);
                    let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
                    SetTextColor(mem_dc, pill_fg);
                    let mut pill_text: Vec<u16> = pill_text_str.encode_utf16().collect();
                    let mut pill_rect = RECT { left: pill_x, top: 16, right: pill_x + pill_w, bottom: 16 + pill_h };
                    let _ = DrawTextW(mem_dc, &mut pill_text, &mut pill_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                    let hover = HOVER_BUTTON.load(Ordering::Relaxed);

                    if let Some(req) = pending_req {
                        // ==============================================================
                        // INCOMING SESSION APPROVAL VIEW (Full main area card)
                        // ==============================================================
                        let mw = (w - 64).clamp(440, 620);
                        let mh = 290;
                        let mx = (w - mw) / 2;
                        let my = ((h - mh) / 2).max(68);

                        draw_rounded_box(mem_dc, mx, my, mw, mh, PK_CARD, PK_GREEN, 14);

                        // Badge Pill
                        let badge_w = 250;
                        let badge_x = mx + (mw - badge_w) / 2;
                        draw_rounded_box(mem_dc, badge_x, my + 22, badge_w, 28, PK_GREEN_TINT, PK_GREEN, 14);
                        let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
                        SetTextColor(mem_dc, PK_GREEN);
                        let mut b_txt: Vec<u16> = "ğŸ”” INCOMING CONNECTION REQUEST".encode_utf16().collect();
                        let mut b_rc = RECT { left: badge_x, top: my + 22, right: badge_x + badge_w, bottom: my + 50 };
                        let _ = DrawTextW(mem_dc, &mut b_txt, &mut b_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                        // Client ID Display
                        let _ = SelectObject(mem_dc, HGDIOBJ(font_id.0));
                        SetTextColor(mem_dc, PK_TEXT);
                        let mut cid_txt: Vec<u16> = format!("ID: {}", req.client_id).encode_utf16().collect();
                        let mut cid_rc = RECT { left: mx + 20, top: my + 66, right: mx + mw - 20, bottom: my + 116 };
                        let _ = DrawTextW(mem_dc, &mut cid_txt, &mut cid_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                        // Subtitle prompt
                        let _ = SelectObject(mem_dc, HGDIOBJ(font_sub.0));
                        SetTextColor(mem_dc, PK_MUTED);
                        let mut p_txt: Vec<u16> =
                            "This remote client is requesting permission to view and control your desktop."
                                .encode_utf16()
                                .collect();
                        let mut p_rc = RECT { left: mx + 24, top: my + 124, right: mx + mw - 24, bottom: my + 156 };
                        let _ = DrawTextW(mem_dc, &mut p_txt, &mut p_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                        // [ âœ“ Accept ] and [ âœ• Decline ] buttons
                        let btn_w = (mw - 72) / 2;
                        let btn_h = 50;
                        let btn_y = my + 190;
                        let acc_x = mx + 24;
                        let dec_x = mx + 48 + btn_w;

                        let acc_bg = if hover == 3 { PK_GREEN_DARK } else { PK_GREEN };
                        draw_rounded_box(mem_dc, acc_x, btn_y, btn_w, btn_h, acc_bg, acc_bg, 10);
                        let _ = SelectObject(mem_dc, HGDIOBJ(font_btn.0));
                        SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
                        let mut acc_txt: Vec<u16> = "âœ“  Accept Connection".encode_utf16().collect();
                        let mut acc_rc = RECT { left: acc_x, top: btn_y, right: acc_x + btn_w, bottom: btn_y + btn_h };
                        let _ = DrawTextW(mem_dc, &mut acc_txt, &mut acc_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                        draw_rounded_box(mem_dc, dec_x, btn_y, btn_w, btn_h, PK_RED, PK_RED, 10);
                        SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
                        let mut dec_txt: Vec<u16> = "âœ•  Decline".encode_utf16().collect();
                        let mut dec_rc = RECT { left: dec_x, top: btn_y, right: dec_x + btn_w, bottom: btn_y + btn_h };
                        let _ = DrawTextW(mem_dc, &mut dec_txt, &mut dec_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
                    } else {
                        // Normal Launcher Cards
                        let c1_x = l.c1_x;
                        let c2_x = l.c2_x;
                        let card_w = l.card_w;
                        let card_h = l.card_h;
                        let card_y = l.card_y;
                        let id_box_x = l.id_box_x;
                        let id_box_y = l.id_box_y;
                        let id_box_w = l.id_box_w;
                        let id_box_h = l.id_box_h;
                        let copy_btn_x = l.copy_btn_x;
                        let copy_btn_y = l.copy_btn_y;
                        let copy_btn_w = l.copy_btn_w;
                        let copy_btn_h = l.copy_btn_h;
                        let input_box_x = l.input_box_x;
                        let input_box_y = l.input_box_y;
                        let input_box_w = l.input_box_w;
                        let input_box_h = l.input_box_h;
                        let conn_btn_x = l.conn_btn_x;
                        let conn_btn_y = l.conn_btn_y;
                        let conn_btn_w = l.conn_btn_w;
                        let conn_btn_h = l.conn_btn_h;
                        let col1_x = l.col1_x;
                        let col2_x = l.col2_x;
                        let col_w = l.col_w;
                        let col_h = l.col_h;
                        let col_y = l.col_y;

                        // 1. Card 1: THIS DESK (Left)
                        draw_rounded_box(mem_dc, c1_x, card_y, card_w, card_h, PK_CARD, PK_GREEN, 12);

                        let tag1_w = (92.0 * l.font_scale) as i32;
                        let tag1_h = (22.0 * l.font_scale) as i32;
                        draw_rounded_box(mem_dc, c1_x + 18, card_y + 14, tag1_w, tag1_h, PK_GREEN_TINT, PK_GREEN, 6);
                        let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
                        SetTextColor(mem_dc, PK_GREEN);
                        let mut tag1: Vec<u16> = "THIS DESK".encode_utf16().collect();
                        let mut tag1_rect = RECT { left: c1_x + 18, top: card_y + 14, right: c1_x + 18 + tag1_w, bottom: card_y + 14 + tag1_h };
                        let _ = DrawTextW(mem_dc, &mut tag1, &mut tag1_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                        draw_rounded_box(mem_dc, id_box_x, id_box_y, id_box_w, id_box_h, PK_INPUT_BG, PK_CARD_BORDER, 8);
                        SetTextColor(mem_dc, PK_MUTED);
                        let mut id_label: Vec<u16> = "YOUR ID".encode_utf16().collect();
                        let mut id_label_rect = RECT { left: id_box_x + 14, top: id_box_y + 6, right: id_box_x + id_box_w, bottom: id_box_y + 6 + f_badge + 4 };
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
                        let mut id_rect = RECT { left: id_box_x + 14, top: id_box_y + 4 + f_badge, right: id_box_x + id_box_w, bottom: id_box_y + id_box_h };
                        let _ = DrawTextW(mem_dc, &mut id_wide, &mut id_rect, DT_SINGLELINE | DT_VCENTER);

                        let copy_bg = if hover == 1 { PK_BTN_HOVER } else { PK_CARD };
                        let copy_border = if hover == 1 { PK_GREEN } else { PK_CARD_BORDER };
                        draw_rounded_box(mem_dc, copy_btn_x, copy_btn_y, copy_btn_w, copy_btn_h, copy_bg, copy_border, 8);
                        let _ = SelectObject(mem_dc, HGDIOBJ(font_btn.0));
                        SetTextColor(mem_dc, if hover == 1 { PK_GREEN_DARK } else { PK_TEXT });
                        let mut copy_text: Vec<u16> = "Copy ID".encode_utf16().collect();
                        let mut copy_rect = RECT { left: copy_btn_x, top: copy_btn_y, right: copy_btn_x + copy_btn_w, bottom: copy_btn_y + copy_btn_h };
                        let _ = DrawTextW(mem_dc, &mut copy_text, &mut copy_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                        // 2. Card 2: REMOTE DESK
                        draw_rounded_box(mem_dc, c2_x, card_y, card_w, card_h, PK_CARD, PK_CARD_BORDER, 12);

                        let tag2_w = (110.0 * l.font_scale) as i32;
                        let tag2_h = (22.0 * l.font_scale) as i32;
                        draw_rounded_box(mem_dc, c2_x + 18, card_y + 14, tag2_w, tag2_h, PK_INPUT_BG, PK_CARD_BORDER, 6);
                        let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
                        SetTextColor(mem_dc, PK_MUTED);
                        let mut tag2: Vec<u16> = "REMOTE DESK".encode_utf16().collect();
                        let mut tag2_rect = RECT { left: c2_x + 18, top: card_y + 14, right: c2_x + 18 + tag2_w, bottom: card_y + 14 + tag2_h };
                        let _ = DrawTextW(mem_dc, &mut tag2, &mut tag2_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                        draw_rounded_box(mem_dc, input_box_x, input_box_y, input_box_w, input_box_h, PK_INPUT_BG, PK_CARD_BORDER, 8);

                        let conn_bg = if hover == 2 { PK_GREEN_DARK } else { PK_GREEN };
                        draw_rounded_box(mem_dc, conn_btn_x, conn_btn_y, conn_btn_w, conn_btn_h, conn_bg, conn_bg, 8);
                        let _ = SelectObject(mem_dc, HGDIOBJ(font_btn.0));
                        SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
                        let mut conn_text: Vec<u16> = "Connect".encode_utf16().collect();
                        let mut conn_rect = RECT { left: conn_btn_x, top: conn_btn_y, right: conn_btn_x + conn_btn_w, bottom: conn_btn_y + conn_btn_h };
                        let _ = DrawTextW(mem_dc, &mut conn_text, &mut conn_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                        // 3. Card 3: RECENT SESSIONS (Bottom Left)
                        draw_rounded_box(mem_dc, col1_x, col_y, col_w, col_h, PK_CARD, PK_CARD_BORDER, 12);

                        let tag3_w = (128.0 * l.font_scale) as i32;
                        let tag3_h = (22.0 * l.font_scale) as i32;
                        draw_rounded_box(mem_dc, col1_x + 18, col_y + 14, tag3_w, tag3_h, PK_INPUT_BG, PK_CARD_BORDER, 6);
                        let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
                        SetTextColor(mem_dc, PK_MUTED);
                        let mut tag3: Vec<u16> = "RECENT SESSIONS".encode_utf16().collect();
                        let mut tag3_rect = RECT { left: col1_x + 18, top: col_y + 14, right: col1_x + 18 + tag3_w, bottom: col_y + 14 + tag3_h };
                        let _ = DrawTextW(mem_dc, &mut tag3, &mut tag3_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                        let recent_sessions = crate::identity::get_recent_sessions();
                        if recent_sessions.is_empty() {
                            let _ = SelectObject(mem_dc, HGDIOBJ(font_console.0));
                            SetTextColor(mem_dc, PK_MUTED);
                            let mut empty_msg1: Vec<u16> = "No recent sessions yet.".encode_utf16().collect();
                            let mut empty_rc1 = RECT {
                                left: col1_x + 20,
                                top: col_y + 48,
                                right: col1_x + col_w - 20,
                                bottom: col_y + 70,
                            };
                            let _ = DrawTextW(mem_dc, &mut empty_msg1, &mut empty_rc1, DT_SINGLELINE | DT_VCENTER);

                            let mut empty_msg2: Vec<u16> = "Remote desks you connect to will appear here for fast 1-click reconnect.".encode_utf16().collect();
                            let mut empty_rc2 = RECT {
                                left: col1_x + 20,
                                top: col_y + 72,
                                right: col1_x + col_w - 20,
                                bottom: col_y + 112,
                            };
                            let _ = DrawTextW(mem_dc, &mut empty_msg2, &mut empty_rc2, DT_WORDBREAK);
                        } else {
                            let item_h = (36.0 * l.font_scale) as i32;
                            let item_w = col_w - 36;
                            for (idx, session) in recent_sessions.iter().take(2).enumerate() {
                                let item_x = col1_x + 18;
                                let item_y = col_y + 46 + (idx as i32 * (item_h + 8));

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

                        // 4. Card 4: SECURITY & SERVER (Bottom Right)
                        draw_rounded_box(mem_dc, col2_x, col_y, col_w, col_h, PK_CARD, PK_CARD_BORDER, 12);

                        let tag4_w = (146.0 * l.font_scale) as i32;
                        let tag4_h = (22.0 * l.font_scale) as i32;
                        draw_rounded_box(mem_dc, col2_x + 18, col_y + 14, tag4_w, tag4_h, PK_INPUT_BG, PK_CARD_BORDER, 6);
                        let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
                        SetTextColor(mem_dc, PK_MUTED);
                        let mut tag4: Vec<u16> = "SECURITY & SERVER".encode_utf16().collect();
                        let mut tag4_rect = RECT { left: col2_x + 18, top: col_y + 14, right: col2_x + 18 + tag4_w, bottom: col_y + 14 + tag4_h };
                        let _ = DrawTextW(mem_dc, &mut tag4, &mut tag4_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                        let diag_box_x = col2_x + 18;
                        let diag_box_y = col_y + 44;
                        let diag_box_w = col_w - 36;
                        let diag_box_h = col_h - 54;
                        draw_rounded_box(mem_dc, diag_box_x, diag_box_y, diag_box_w, diag_box_h, PK_INPUT_BG, PK_CARD_BORDER, 8);

                        let _ = SelectObject(mem_dc, HGDIOBJ(font_console.0));

                        let srv_status = if is_online {
                            if ping > 0 {
                                format!("Istanbul (TR)  â€¢  {}ms", ping)
                            } else {
                                "Istanbul (TR)  â€¢  Online".to_string()
                            }
                        } else {
                            "Istanbul (TR)  â€¢  Offline".to_string()
                        };

                        let host_name = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Windows PC".into());

                        let diag_lines = [
                            ("Server Region:", srv_status),
                            ("Security Mode:", "End-to-End Encrypted".to_string()),
                            ("This Machine:", host_name),
                            ("Access Control:", "Interactive User Approval".to_string()),
                        ];

                        let line_h = (diag_box_h - 8) / 4;
                        for (i, (lbl, val)) in diag_lines.iter().enumerate() {
                            let row_y = diag_box_y + 4 + (i as i32 * line_h);

                            SetTextColor(mem_dc, PK_MUTED);
                            let mut lbl_txt: Vec<u16> = lbl.encode_utf16().collect();
                            let mut lbl_rc = RECT {
                                left: diag_box_x + 10,
                                top: row_y,
                                right: diag_box_x + 115,
                                bottom: row_y + line_h,
                            };
                            let _ = DrawTextW(mem_dc, &mut lbl_txt, &mut lbl_rc, DT_SINGLELINE | DT_VCENTER);

                            SetTextColor(mem_dc, PK_TEXT);
                            let mut val_txt: Vec<u16> = val.encode_utf16().collect();
                            let mut val_rc = RECT {
                                left: diag_box_x + 118,
                                top: row_y,
                                right: diag_box_x + diag_box_w - 8,
                                bottom: row_y + line_h,
                            };
                            let _ = DrawTextW(mem_dc, &mut val_txt, &mut val_rc, DT_SINGLELINE | DT_VCENTER);
                        }
                    }

                    let _ = BitBlt(hdc, 0, 0, w, h, Some(mem_dc), 0, 0, SRCCOPY);

                    let _ = SelectObject(mem_dc, old_font);
                    let _ = SelectObject(mem_dc, old_bmp);
                    let _ = DeleteObject(HGDIOBJ(mem_bmp.0));
                    let _ = DeleteObject(HGDIOBJ(mem_dc.0));
                    let _ = DeleteObject(HGDIOBJ(font_title.0));
                    let _ = DeleteObject(HGDIOBJ(font_sub.0));
                    let _ = DeleteObject(HGDIOBJ(font_badge.0));
                    let _ = DeleteObject(HGDIOBJ(font_id.0));
                    let _ = DeleteObject(HGDIOBJ(font_btn.0));
                    let _ = DeleteObject(HGDIOBJ(font_console.0));
                }

                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }

        WM_MOUSEMOVE => {
            unsafe {
                let x = (lparam.0 & 0xffff) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let w = rc.right - rc.left;
                let h = rc.bottom - rc.top;

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
                    if x >= l.copy_btn_x && x <= l.copy_btn_x + l.copy_btn_w && y >= l.copy_btn_y && y <= l.copy_btn_y + l.copy_btn_h {
                        1
                    } else if x >= l.conn_btn_x && x <= l.conn_btn_x + l.conn_btn_w && y >= l.conn_btn_y && y <= l.conn_btn_y + l.conn_btn_h {
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
            LRESULT(0)
        }

        WM_LBUTTONDOWN => {
            unsafe {
                let x = (lparam.0 & 0xffff) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let w = rc.right - rc.left;
                let h = rc.bottom - rc.top;

                // Handle Incoming Approval Modal clicks first if a request is pending
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
                    } else if x >= dec_x && x <= dec_x + btn_w && y >= btn_y && y <= btn_y + btn_h {
                        crate::p2p::respond_to_pending_approval(false);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    return LRESULT(0);
                }

                let l = get_responsive_layout(w, h);

                // 1. Copy Button Clicked
                if x >= l.copy_btn_x && x <= l.copy_btn_x + l.copy_btn_w && y >= l.copy_btn_y && y <= l.copy_btn_y + l.copy_btn_h {
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
                        append_status_log(&format!("âœ… Desky ID '{}' copied to Windows Clipboard!", id_to_copy));
                    }
                }

                // 2. Connect Button Clicked
                if x >= l.conn_btn_x && x <= l.conn_btn_x + l.conn_btn_w && y >= l.conn_btn_y && y <= l.conn_btn_y + l.conn_btn_h {
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
                        append_status_log("âš ï¸ Please enter a 9-digit remote Desky ID (e.g. 800 258 756)");
                        return LRESULT(0);
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
                        append_status_log("âš ï¸ That is THIS computer's own ID! Please enter the remote computer's ID.");
                        return LRESULT(0);
                    }

                    let target_display = if digits.len() == 9 {
                        format!("{} {} {}", &digits[0..3], &digits[3..6], &digits[6..9])
                    } else {
                        digits.clone()
                    };

                    info!("Initiating P2P session to remote Desky ID: {}", target_display);
                    crate::identity::save_recent_session(&target_display);
                    append_status_log(&format!("Requesting session with Remote Desky ID: {}...", target_display));

                    crate::p2p::start_client_session(
                        &target_display,
                        TARGET_FPS.load(Ordering::Relaxed),
                        CAPTURE_CURSOR.load(Ordering::Relaxed),
                    );

                    spawn_or_add_tab_viewport(&target_display);
                }

                // 3. Recent Sessions item clicked
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
                            // Clicked directly on "Connect" pill! Trigger connection immediately
                            let target_display = session.clone();
                            info!("Initiating P2P session to remote Desky ID: {}", target_display);
                            crate::identity::save_recent_session(&target_display);
                            append_status_log(&format!("Requesting session with Remote Desky ID: {}...", target_display));

                            crate::p2p::start_client_session(
                                &target_display,
                                TARGET_FPS.load(Ordering::Relaxed),
                                CAPTURE_CURSOR.load(Ordering::Relaxed),
                            );

                            spawn_or_add_tab_viewport(&target_display);
                        } else {
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        return LRESULT(0);
                    }
                }
            }
            LRESULT(0)
        }

        WM_APP_REDRAW => {
            unsafe {
                let has_pending = crate::p2p::get_pending_approval().is_some();
                if !HWND_EDIT_TARGET.0.is_null() && IsWindow(Some(HWND_EDIT_TARGET)).as_bool() {
                    let _ = ShowWindow(
                        HWND_EDIT_TARGET,
                        if has_pending { SW_HIDE } else { SW_SHOW },
                    );
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }

        WM_APP_SHOW_PROGRESS => {
            spawn_progress_window();
            LRESULT(0)
        }
        WM_APP_BRING_FRONT => {
            unsafe {
                let has_pending = crate::p2p::get_pending_approval().is_some();
                if !HWND_EDIT_TARGET.0.is_null() && IsWindow(Some(HWND_EDIT_TARGET)).as_bool() {
                    let _ = ShowWindow(
                        HWND_EDIT_TARGET,
                        if has_pending { SW_HIDE } else { SW_SHOW },
                    );
                }
                if IsIconic(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                }
                let _ = SetForegroundWindow(hwnd);
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }

        WM_CTLCOLOREDIT => {
            unsafe {
                let hdc = HDC(wparam.0 as *mut _);
                SetBkMode(hdc, OPAQUE);
                SetBkColor(hdc, PK_INPUT_BG);
                SetTextColor(hdc, PK_TEXT);
                LRESULT(EDIT_BG_BRUSH.0 as isize)
            }
        }

        WM_DESTROY => {
            unsafe {
                let _ = KillTimer(Some(hwnd), 1001);
                let old = CURRENT_EDIT_FONT.swap(0, Ordering::Relaxed);
                if old != 0 {
                    let _ = DeleteObject(HGDIOBJ(old as *mut _));
                }
                VIEWPORT_RUNNING.store(false, Ordering::SeqCst);
                crate::p2p::stop_client_session();
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
