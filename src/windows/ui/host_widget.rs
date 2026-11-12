//! Host-Side Floating Session Widget & Screen Sharing Indicator (Win32)
//!
//! Provides the controlled user with live awareness and complete control during remote sessions:
//! - Modern docked control panel above the Windows taskbar.
//! - Branded forest-emerald header with draggable grip, minimize, and quick session termination.
//! - Live session timer (`00:14:22`) and remote controller ID with emerald status badge.
//! - Padded, non-overlapping action buttons (Microphone toggle, In-Session Chat, and End Session).
//! - 3-state adaptive layout: Collapsed (32px), Standard Control Dock (104px), and Expanded Chat (350px).
//! - Non-intrusive full-screen click-through privacy border indicator.

#![allow(unsafe_op_in_unsafe_fn)]

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, Ordering};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreatePen, CreateRoundRectRgn,
    CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetStockObject, InvalidateRect,
    RoundRect, SelectObject, SetBkColor, SetBkMode, SetTextColor, SetWindowRgn, DT_CENTER, DT_LEFT,
    DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL, FW_SEMIBOLD, HDC, HGDIOBJ, NULL_BRUSH,
    PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos, GetParent, GetSystemMetrics,
    GetWindowRect, GetWindowTextLengthW, GetWindowTextW, IsWindow, LoadCursorW, MoveWindow,
    RegisterClassExW, SendMessageW, SetCursor, SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW, SetWindowTextW,
    ShowWindow, GWLP_WNDPROC, HMENU, HTCAPTION, IDC_ARROW, IDC_HAND, LWA_COLORKEY, SM_CXSCREEN, SM_CYSCREEN,
    SW_HIDE, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CHAR, WM_COMMAND, WM_CREATE, WM_CTLCOLOREDIT,
    WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_NCLBUTTONDOWN, WM_PAINT, WM_SETCURSOR, WM_TIMER,
    WNDCLASSEXW, WS_BORDER, WS_CHILD, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP, WS_TABSTOP, WS_VISIBLE,
};

use super::theme::{
    create_segoe_font, draw_rounded_box, PK_BTN_HOVER, PK_CARD, PK_CARD_BORDER,
    PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_RED, PK_TEXT,
};

pub const HOST_WIDGET_WIDTH: i32 = 330;
pub const HOST_WIDGET_MINI_H: i32 = 32;
pub const HOST_WIDGET_NORMAL_H: i32 = 104;
pub const HOST_WIDGET_EXPANDED_H: i32 = 350;

const ID_EDIT_HOST_CHAT: usize = 9981;
const ID_TIMER_WIDGET_TICK: usize = 4001;

static HWND_HOST_WIDGET: AtomicIsize = AtomicIsize::new(0);
static HWND_SCREEN_BORDER: AtomicIsize = AtomicIsize::new(0);
static HWND_CHAT_EDIT: AtomicIsize = AtomicIsize::new(0);
static PREV_HOST_CHAT_WNDPROC: AtomicIsize = AtomicIsize::new(0);
static CHAT_EXPANDED: AtomicBool = AtomicBool::new(false);
static COLLAPSED: AtomicBool = AtomicBool::new(false);
static HOVER_BTN: AtomicI32 = AtomicI32::new(0);
// 1 = Mic, 2 = Chat, 3 = End, 4 = Send, 10 = Minimize/Restore, 11 = Header Close

unsafe fn apply_widget_shape(hwnd: HWND, w: i32, h: i32) {
    let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, 10, 10);
    let _ = SetWindowRgn(hwnd, Some(rgn), true);
}

fn get_current_target_h() -> i32 {
    if COLLAPSED.load(Ordering::Relaxed) {
        HOST_WIDGET_MINI_H
    } else if CHAT_EXPANDED.load(Ordering::Relaxed) {
        HOST_WIDGET_EXPANDED_H
    } else {
        HOST_WIDGET_NORMAL_H
    }
}

unsafe fn update_widget_layout(hwnd: HWND) {
    let mut rc = RECT::default();
    let _ = GetWindowRect(hwnd, &mut rc);
    let cur_x = rc.left;
    let target_h = get_current_target_h();
    let target_y = (rc.bottom - target_h).max(10);

    let _ = MoveWindow(hwnd, cur_x, target_y, HOST_WIDGET_WIDTH, target_h, true);
    apply_widget_shape(hwnd, HOST_WIDGET_WIDTH, target_h);

    let edit_raw = HWND_CHAT_EDIT.load(Ordering::Relaxed);
    if edit_raw != 0 {
        let eh = HWND(edit_raw as *mut _);
        let show_edit = CHAT_EXPANDED.load(Ordering::Relaxed) && !COLLAPSED.load(Ordering::Relaxed);
        let _ = ShowWindow(eh, if show_edit { SW_SHOW } else { SW_HIDE });
        if show_edit {
            let _ = SetFocus(Some(eh));
        }
    }
    let _ = InvalidateRect(Some(hwnd), None, false);
}

unsafe extern "system" fn host_chat_edit_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let old_proc = PREV_HOST_CHAT_WNDPROC.load(Ordering::Relaxed);
    let old_wndproc = if old_proc != 0 {
        Some(std::mem::transmute::<
            isize,
            unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
        >(old_proc))
    } else {
        None
    };

    match msg {
        WM_KEYDOWN => {
            let key = wparam.0 as i32;
            if key == 0x0D {
                send_host_chat_input();
                let parent = GetParent(hwnd).unwrap_or_default();
                if !parent.0.is_null() {
                    let _ = InvalidateRect(Some(parent), None, false);
                }
                return LRESULT(0);
            }
        }
        WM_CHAR => {
            if wparam.0 == 0x0D {
                return LRESULT(0); // Suppress Windows bell on Enter
            }
        }
        _ => {}
    }

    if let Some(prev) = old_wndproc {
        prev(hwnd, msg, wparam, lparam)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

/// Synchronizes the visibility of the Host Floating Widget and Screen Border.
/// Automatically creates or destroys the window based on whether an approved client is connected.
pub fn sync_host_session_ui() {
    let session = crate::p2p::get_active_host_session_info();
    if session.is_some() {
        spawn_host_widget();
        spawn_screen_border();
    } else {
        close_host_widget();
        close_screen_border();
    }
}

pub fn spawn_host_widget() {
    let existing = HWND_HOST_WIDGET.load(Ordering::Relaxed);
    if existing != 0 {
        let hwnd = HWND(existing as *mut _);
        if unsafe { IsWindow(Some(hwnd)).as_bool() } {
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            return;
        }
    }

    unsafe {
        let h_module = GetModuleHandleW(None).unwrap_or_default();
        let h_instance: HINSTANCE = h_module.into();
        let class_name = w!("DeskyHostWidgetClass");

        let mut wc = WNDCLASSEXW::default();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(host_widget_wndproc);
        wc.hInstance = h_instance;
        wc.lpszClassName = class_name;
        wc.hbrBackground = CreateSolidBrush(PK_CARD);
        wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        let _ = RegisterClassExW(&wc);

        // Position at bottom-right of primary screen above taskbar
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let x = (screen_w - HOST_WIDGET_WIDTH - 20).max(10);
        let y = (screen_h - HOST_WIDGET_NORMAL_H - 52).max(10);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("Desky | Remote Session"),
            WS_POPUP | WS_VISIBLE,
            x,
            y,
            HOST_WIDGET_WIDTH,
            HOST_WIDGET_NORMAL_H,
            None,
            None,
            Some(h_instance),
            None,
        );

        if let Ok(h) = hwnd {
            HWND_HOST_WIDGET.store(h.0 as isize, Ordering::Relaxed);
            apply_widget_shape(h, HOST_WIDGET_WIDTH, HOST_WIDGET_NORMAL_H);
            let _ = SetTimer(Some(h), ID_TIMER_WIDGET_TICK, 500, None);
        }
    }
}

pub fn close_host_widget() {
    let existing = HWND_HOST_WIDGET.swap(0, Ordering::Relaxed);
    if existing != 0 {
        let hwnd = HWND(existing as *mut _);
        unsafe {
            if IsWindow(Some(hwnd)).as_bool() {
                let _ = DestroyWindow(hwnd);
            }
        }
    }
    HWND_CHAT_EDIT.store(0, Ordering::Relaxed);
    CHAT_EXPANDED.store(false, Ordering::Relaxed);
    COLLAPSED.store(false, Ordering::Relaxed);
}

pub fn spawn_screen_border() {
    let existing = HWND_SCREEN_BORDER.load(Ordering::Relaxed);
    if existing != 0 {
        let hwnd = HWND(existing as *mut _);
        if unsafe { IsWindow(Some(hwnd)).as_bool() } {
            return;
        }
    }

    unsafe {
        let h_module = GetModuleHandleW(None).unwrap_or_default();
        let h_instance: HINSTANCE = h_module.into();
        let class_name = w!("DeskyScreenBorderClass");

        let mut wc = WNDCLASSEXW::default();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(screen_border_wndproc);
        wc.hInstance = h_instance;
        wc.lpszClassName = class_name;
        wc.hbrBackground = CreateSolidBrush(COLORREF(0x00000000));
        wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        let _ = RegisterClassExW(&wc);

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_LAYERED,
            class_name,
            w!("Desky | Screen Border"),
            WS_POPUP | WS_VISIBLE,
            0,
            0,
            screen_w,
            screen_h,
            None,
            None,
            Some(h_instance),
            None,
        );

        if let Ok(h) = hwnd {
            HWND_SCREEN_BORDER.store(h.0 as isize, Ordering::Relaxed);
            // Black color key makes interior completely transparent and click-through
            let _ = SetLayeredWindowAttributes(h, COLORREF(0x00000000), 0, LWA_COLORKEY);
        }
    }
}

pub fn close_screen_border() {
    let existing = HWND_SCREEN_BORDER.swap(0, Ordering::Relaxed);
    if existing != 0 {
        let hwnd = HWND(existing as *mut _);
        unsafe {
            if IsWindow(Some(hwnd)).as_bool() {
                let _ = DestroyWindow(hwnd);
            }
        }
    }
}

fn format_duration(seconds: u64) -> String {
    let hrs = seconds / 3600;
    let mins = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hrs > 0 {
        format!("{:02}:{:02}:{:02}", hrs, mins, secs)
    } else {
        format!("{:02}:{:02}", mins, secs)
    }
}

unsafe extern "system" fn host_widget_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let h_module = GetModuleHandleW(None).unwrap_or_default();
            let h_instance: HINSTANCE = h_module.into();

            let edit_hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("EDIT"),
                w!(""),
                WINDOW_STYLE(
                    WS_CHILD.0 | WS_BORDER.0 | WS_TABSTOP.0 | 0x0080, // ES_AUTOHSCROLL
                ),
                10,
                308,
                244,
                28,
                Some(hwnd),
                Some(HMENU(ID_EDIT_HOST_CHAT as *mut _)),
                Some(h_instance),
                None,
            );

            if let Ok(eh) = edit_hwnd {
                let font = create_segoe_font(12, FW_NORMAL);
                let _ = SendMessageW(eh, windows::Win32::UI::WindowsAndMessaging::WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
                let old = SetWindowLongPtrW(eh, GWLP_WNDPROC, host_chat_edit_proc as *const () as usize as isize);
                if old != 0 {
                    PREV_HOST_CHAT_WNDPROC.store(old, Ordering::Relaxed);
                }
                HWND_CHAT_EDIT.store(eh.0 as isize, Ordering::Relaxed);
                let _ = ShowWindow(eh, SW_HIDE);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xffff) as usize;
            if id == ID_EDIT_HOST_CHAT {
                let notif = ((wparam.0 >> 16) & 0xffff) as u16;
                if notif == 0x0300 {
                    // EN_CHANGE
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_CTLCOLOREDIT => {
            let hdc = HDC(wparam.0 as *mut _);
            SetBkMode(hdc, windows::Win32::Graphics::Gdi::OPAQUE);
            SetBkColor(hdc, PK_INPUT_BG);
            SetTextColor(hdc, PK_TEXT);
            let brush = CreateSolidBrush(PK_INPUT_BG);
            LRESULT(brush.0 as isize)
        }
        WM_TIMER => {
            let id = wparam.0 as usize;
            if id == ID_TIMER_WIDGET_TICK {
                if crate::p2p::get_active_host_session_info().is_none() {
                    sync_host_session_ui();
                } else {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let is_collapsed = COLLAPSED.load(Ordering::Relaxed);
            let is_chat = CHAT_EXPANDED.load(Ordering::Relaxed);

            // Collapsed state handling
            if is_collapsed {
                if client_x >= 298 && client_x <= 324 && client_y >= 4 && client_y <= 28 {
                    COLLAPSED.store(false, Ordering::Relaxed);
                    update_widget_layout(hwnd);
                    return LRESULT(0);
                }
                // Allow dragging mini bar
                let _ = ReleaseCapture();
                let _ = SendMessageW(hwnd, WM_NCLBUTTONDOWN, Some(WPARAM(HTCAPTION as usize)), Some(LPARAM(0)));
                return LRESULT(0);
            }

            // 1. Header controls (y <= 28)
            if client_y <= 28 {
                if client_x >= 278 && client_x <= 300 {
                    // Minimize to compact mini bar
                    COLLAPSED.store(true, Ordering::Relaxed);
                    update_widget_layout(hwnd);
                    return LRESULT(0);
                } else if client_x >= 302 && client_x <= 324 {
                    // Close / Disconnect
                    crate::p2p::disconnect_host_session();
                    close_host_widget();
                    close_screen_border();
                    return LRESULT(0);
                }
                // Header bar draggable
                let _ = ReleaseCapture();
                let _ = SendMessageW(hwnd, WM_NCLBUTTONDOWN, Some(WPARAM(HTCAPTION as usize)), Some(LPARAM(0)));
                return LRESULT(0);
            }

            // 2. Action buttons (y >= 66 && y <= 96)
            if client_y >= 66 && client_y <= 96 {
                if client_x >= 10 && client_x <= 106 {
                    // Mic toggle
                    let is_on = crate::windows::audio::toggle_host_mic();
                    tracing::info!("Host microphone toggled: {}", is_on);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                } else if client_x >= 114 && client_x <= 210 {
                    // Chat toggle
                    let next = !is_chat;
                    CHAT_EXPANDED.store(next, Ordering::Relaxed);
                    crate::p2p::mark_chat_read();
                    update_widget_layout(hwnd);
                    return LRESULT(0);
                } else if client_x >= 218 && client_x <= 320 {
                    // End Session
                    crate::p2p::disconnect_host_session();
                    close_host_widget();
                    close_screen_border();
                    return LRESULT(0);
                }
            }

            // 3. Send message button (when chat is expanded)
            if is_chat && client_x >= 262 && client_x <= 320 && client_y >= 306 && client_y <= 338 {
                send_host_chat_input();
                let _ = InvalidateRect(Some(hwnd), None, false);
                return LRESULT(0);
            }

            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let is_collapsed = COLLAPSED.load(Ordering::Relaxed);
            let is_chat = CHAT_EXPANDED.load(Ordering::Relaxed);

            let hover = if is_collapsed {
                if client_x >= 298 && client_x <= 324 && client_y >= 4 && client_y <= 28 {
                    10 // Restore
                } else {
                    0
                }
            } else {
                if client_y >= 4 && client_y <= 24 {
                    if client_x >= 278 && client_x <= 300 {
                        10 // Minimize
                    } else if client_x >= 302 && client_x <= 324 {
                        11 // Header Close
                    } else {
                        0
                    }
                } else if client_y >= 66 && client_y <= 96 {
                    if client_x >= 10 && client_x <= 106 {
                        1 // Mic
                    } else if client_x >= 114 && client_x <= 210 {
                        2 // Chat
                    } else if client_x >= 218 && client_x <= 320 {
                        3 // End Session
                    } else {
                        0
                    }
                } else if is_chat && client_x >= 262 && client_x <= 320 && client_y >= 306 && client_y <= 338 {
                    4 // Send Button
                } else {
                    0
                }
            };

            let prev_hover = HOVER_BTN.load(Ordering::Relaxed);
            if prev_hover != hover {
                HOVER_BTN.store(hover, Ordering::Relaxed);
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);

            let hover = HOVER_BTN.load(Ordering::Relaxed);
            let cursor_name = if hover > 0 { IDC_HAND } else { IDC_ARROW };
            let c = LoadCursorW(None, cursor_name).unwrap_or_default();
            SetCursor(Some(c));
            LRESULT(1)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let w = rc.right - rc.left;
            let h = rc.bottom - rc.top;

            let hdc_mem = CreateCompatibleDC(Some(hdc));
            let hbm_mem = CreateCompatibleBitmap(hdc, w, h);
            let old_bmp = SelectObject(hdc_mem, HGDIOBJ(hbm_mem.0));

            let hover = HOVER_BTN.load(Ordering::Relaxed);
            let is_collapsed = COLLAPSED.load(Ordering::Relaxed);
            let chat_expanded = CHAT_EXPANDED.load(Ordering::Relaxed);
            let unread = crate::p2p::get_unread_count();

            let session_info = crate::p2p::get_active_host_session_info();
            let (raw_client_id, elapsed_secs) = session_info.unwrap_or_else(|| ("Remote Peer".to_string(), 0));
            let client_id = crate::p2p::state::format_id_display(&raw_client_id);
            let timer_str = format!("⏱ {}", format_duration(elapsed_secs));

            let pine_header_bg = COLORREF(0x001B4332); // Signature Dark Pine Header
            let pine_mint_fg = COLORREF(0x00B7E4C7);   // Soft emerald mint text

            if is_collapsed {
                // Mini Docked Bar (32px high)
                let bg_brush = CreateSolidBrush(pine_header_bg);
                FillRect(hdc_mem, &rc, bg_brush);
                let _ = DeleteObject(HGDIOBJ(bg_brush.0));

                let dot_pen = CreatePen(PS_SOLID, 1, PK_GREEN_DARK);
                let dot_brush = CreateSolidBrush(PK_GREEN);
                let old_dp = SelectObject(hdc_mem, HGDIOBJ(dot_pen.0));
                let old_db = SelectObject(hdc_mem, HGDIOBJ(dot_brush.0));
                let _ = windows::Win32::Graphics::Gdi::Ellipse(hdc_mem, 12, 11, 20, 19);
                let _ = SelectObject(hdc_mem, old_dp);
                let _ = SelectObject(hdc_mem, old_db);
                let _ = DeleteObject(HGDIOBJ(dot_pen.0));
                let _ = DeleteObject(HGDIOBJ(dot_brush.0));

                SetBkMode(hdc_mem, TRANSPARENT);
                let font_bold = create_segoe_font(11, FW_BOLD);
                let old_f = SelectObject(hdc_mem, HGDIOBJ(font_bold.0));
                SetTextColor(hdc_mem, COLORREF(0x00FFFFFF));

                let mut brand_txt: Vec<u16> = "DESKY".encode_utf16().collect();
                let mut brand_rc = RECT { left: 26, top: 0, right: 75, bottom: 32 };
                DrawTextW(hdc_mem, &mut brand_txt, &mut brand_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

                let font_regular = create_segoe_font(11, FW_SEMIBOLD);
                let _ = SelectObject(hdc_mem, HGDIOBJ(font_regular.0));
                SetTextColor(hdc_mem, pine_mint_fg);

                let summary = format!("{} | {}", client_id, timer_str);
                let mut sum_txt: Vec<u16> = summary.encode_utf16().collect();
                let mut sum_rc = RECT { left: 80, top: 0, right: 290, bottom: 32 };
                DrawTextW(hdc_mem, &mut sum_txt, &mut sum_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

                // Restore Button (▲)
                let r_bg = if hover == 10 { COLORREF(0x00325C47) } else { pine_header_bg };
                draw_rounded_box(hdc_mem, 298, 5, 22, 22, r_bg, COLORREF(0x004F6A2D), 4);
                let mut arr_txt: Vec<u16> = "▲".encode_utf16().collect();
                let mut arr_rc = RECT { left: 298, top: 5, right: 320, bottom: 27 };
                SetTextColor(hdc_mem, COLORREF(0x00FFFFFF));
                DrawTextW(hdc_mem, &mut arr_txt, &mut arr_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                let _ = SelectObject(hdc_mem, old_f);
                let _ = DeleteObject(HGDIOBJ(font_bold.0));
                let _ = DeleteObject(HGDIOBJ(font_regular.0));
            } else {
                // Card Background & Border
                let bg_brush = CreateSolidBrush(PK_CARD);
                FillRect(hdc_mem, &rc, bg_brush);
                let _ = DeleteObject(HGDIOBJ(bg_brush.0));

                let border_pen = CreatePen(PS_SOLID, 1, PK_CARD_BORDER);
                let old_pen = SelectObject(hdc_mem, HGDIOBJ(border_pen.0));
                let old_brush = SelectObject(hdc_mem, HGDIOBJ(GetStockObject(NULL_BRUSH).0));
                let _ = RoundRect(hdc_mem, 0, 0, w, h, 10, 10);
                let _ = SelectObject(hdc_mem, old_brush);
                let _ = SelectObject(hdc_mem, old_pen);
                let _ = DeleteObject(HGDIOBJ(border_pen.0));

                // 1. Branded Header Bar (0..28)
                let header_rc = RECT { left: 0, top: 0, right: w, bottom: 28 };
                let h_brush = CreateSolidBrush(pine_header_bg);
                FillRect(hdc_mem, &header_rc, h_brush);
                let _ = DeleteObject(HGDIOBJ(h_brush.0));

                SetBkMode(hdc_mem, TRANSPARENT);
                let font_title = create_segoe_font(11, FW_BOLD);
                let old_font = SelectObject(hdc_mem, HGDIOBJ(font_title.0));
                SetTextColor(hdc_mem, COLORREF(0x00FFFFFF));

                let mut title_txt: Vec<u16> = "DESKY".encode_utf16().collect();
                let mut title_rc = RECT { left: 12, top: 0, right: 60, bottom: 28 };
                DrawTextW(hdc_mem, &mut title_txt, &mut title_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

                let font_sub = create_segoe_font(11, FW_NORMAL);
                let _ = SelectObject(hdc_mem, HGDIOBJ(font_sub.0));
                SetTextColor(hdc_mem, pine_mint_fg);
                let mut sub_txt: Vec<u16> = " | Remote Session".encode_utf16().collect();
                let mut sub_rc = RECT { left: 56, top: 0, right: 260, bottom: 28 };
                DrawTextW(hdc_mem, &mut sub_txt, &mut sub_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

                // Minimize Button (—)
                let m_bg = if hover == 10 { COLORREF(0x00325C47) } else { pine_header_bg };
                draw_rounded_box(hdc_mem, 278, 4, 22, 20, m_bg, COLORREF(0x004F6A2D), 4);
                let m_pen = CreatePen(PS_SOLID, 2, COLORREF(0x00FFFFFF));
                let old_mp = SelectObject(hdc_mem, HGDIOBJ(m_pen.0));
                let _ = windows::Win32::Graphics::Gdi::MoveToEx(hdc_mem, 283, 14, None);
                let _ = windows::Win32::Graphics::Gdi::LineTo(hdc_mem, 295, 14);
                let _ = SelectObject(hdc_mem, old_mp);
                let _ = DeleteObject(HGDIOBJ(m_pen.0));

                // Header Close Button (✕)
                let c_bg = if hover == 11 { PK_RED } else { pine_header_bg };
                draw_rounded_box(hdc_mem, 302, 4, 22, 20, c_bg, if hover == 11 { PK_RED } else { COLORREF(0x004F6A2D) }, 4);
                crate::ui::draw_x(hdc_mem, 307, 8, 12, COLORREF(0x00FFFFFF), 2);

                // Divider line below header
                let div_pen = CreatePen(PS_SOLID, 1, PK_CARD_BORDER);
                let old_dp = SelectObject(hdc_mem, HGDIOBJ(div_pen.0));
                let _ = windows::Win32::Graphics::Gdi::MoveToEx(hdc_mem, 0, 28, None);
                let _ = windows::Win32::Graphics::Gdi::LineTo(hdc_mem, w, 28);
                let _ = SelectObject(hdc_mem, old_dp);
                let _ = DeleteObject(HGDIOBJ(div_pen.0));

                // 2. Status Row (29..63)
                // Pulse indicator dot
                let dot_pen = CreatePen(PS_SOLID, 1, PK_GREEN_DARK);
                let dot_brush = CreateSolidBrush(PK_GREEN);
                let old_p = SelectObject(hdc_mem, HGDIOBJ(dot_pen.0));
                let old_b = SelectObject(hdc_mem, HGDIOBJ(dot_brush.0));
                let _ = windows::Win32::Graphics::Gdi::Ellipse(hdc_mem, 12, 41, 20, 49);
                let _ = SelectObject(hdc_mem, old_p);
                let _ = SelectObject(hdc_mem, old_b);
                let _ = DeleteObject(HGDIOBJ(dot_pen.0));
                let _ = DeleteObject(HGDIOBJ(dot_brush.0));

                let font_label = create_segoe_font(11, FW_NORMAL);
                let _ = SelectObject(hdc_mem, HGDIOBJ(font_label.0));
                SetTextColor(hdc_mem, PK_MUTED);
                let mut ctrl_txt: Vec<u16> = "Controlled by: ".encode_utf16().collect();
                let mut ctrl_rc = RECT { left: 26, top: 32, right: 104, bottom: 58 };
                DrawTextW(hdc_mem, &mut ctrl_txt, &mut ctrl_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

                let font_id = create_segoe_font(11, FW_BOLD);
                let _ = SelectObject(hdc_mem, HGDIOBJ(font_id.0));
                SetTextColor(hdc_mem, PK_TEXT);
                let mut id_txt: Vec<u16> = client_id.encode_utf16().collect();
                let mut id_rc = RECT { left: 106, top: 32, right: 230, bottom: 58 };
                DrawTextW(hdc_mem, &mut id_txt, &mut id_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

                // Session Duration Pill
                draw_rounded_box(hdc_mem, 234, 34, 86, 22, PK_GREEN_TINT, PK_CARD_BORDER, 4);
                let font_timer = create_segoe_font(11, FW_SEMIBOLD);
                let _ = SelectObject(hdc_mem, HGDIOBJ(font_timer.0));
                SetTextColor(hdc_mem, PK_GREEN_DARK);
                let mut timer_txt: Vec<u16> = timer_str.encode_utf16().collect();
                let mut timer_rc = RECT { left: 234, top: 34, right: 320, bottom: 56 };
                DrawTextW(hdc_mem, &mut timer_txt, &mut timer_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                // Divider line above buttons
                let div2_pen = CreatePen(PS_SOLID, 1, COLORREF(0x00F0F2F0));
                let old_d2 = SelectObject(hdc_mem, HGDIOBJ(div2_pen.0));
                let _ = windows::Win32::Graphics::Gdi::MoveToEx(hdc_mem, 10, 62, None);
                let _ = windows::Win32::Graphics::Gdi::LineTo(hdc_mem, w - 10, 62);
                let _ = SelectObject(hdc_mem, old_d2);
                let _ = DeleteObject(HGDIOBJ(div2_pen.0));

                // 3. Action Toolbar Row (66..96)
                let is_mic = crate::windows::audio::is_host_mic_active();
                let font_btn = create_segoe_font(11, FW_SEMIBOLD);
                let _ = SelectObject(hdc_mem, HGDIOBJ(font_btn.0));

                // 3.1 Mic Button [10..106]
                let (m_bg, m_border, m_fg) = if is_mic {
                    (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
                } else if hover == 1 {
                    (PK_BTN_HOVER, PK_CARD_BORDER, PK_TEXT)
                } else {
                    (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
                };
                draw_rounded_box(hdc_mem, 10, 67, 96, 28, m_bg, m_border, 6);
                if is_mic {
                    crate::ui::draw_mic(hdc_mem, 18, 73, 14, m_fg, 2);
                } else {
                    crate::ui::draw_mic_off(hdc_mem, 18, 73, 14, m_fg, 2);
                }
                let mic_label = if is_mic { "Mic: ON" } else { "Mic: OFF" };
                let mut wide_mic: Vec<u16> = mic_label.encode_utf16().collect();
                let mut mic_rc = RECT { left: 34, top: 67, right: 104, bottom: 95 };
                SetTextColor(hdc_mem, m_fg);
                DrawTextW(hdc_mem, &mut wide_mic, &mut mic_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                // 3.2 Chat Button [114..210]
                let (c_bg, c_border, c_fg) = if chat_expanded || unread > 0 {
                    (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
                } else if hover == 2 {
                    (PK_BTN_HOVER, PK_CARD_BORDER, PK_TEXT)
                } else {
                    (PK_INPUT_BG, PK_CARD_BORDER, PK_TEXT)
                };
                draw_rounded_box(hdc_mem, 114, 67, 96, 28, c_bg, c_border, 6);
                crate::ui::draw_message_square(hdc_mem, 122, 73, 14, c_fg, 2);
                let chat_label = if unread > 0 {
                    format!("Chat ({})", unread)
                } else {
                    "Chat".to_string()
                };
                let mut wide_chat: Vec<u16> = chat_label.encode_utf16().collect();
                let mut chat_rc = RECT { left: 138, top: 67, right: 208, bottom: 95 };
                SetTextColor(hdc_mem, c_fg);
                DrawTextW(hdc_mem, &mut wide_chat, &mut chat_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                // 3.3 End Session Button [218..320]
                let (d_bg, d_border, d_fg) = if hover == 3 {
                    (PK_RED, PK_RED, COLORREF(0x00FFFFFF))
                } else {
                    (COLORREF(0x00EEF0FF), PK_RED, PK_RED)
                };
                draw_rounded_box(hdc_mem, 218, 67, 102, 28, d_bg, d_border, 6);
                crate::ui::draw_x(hdc_mem, 226, 75, 12, d_fg, 2);
                let mut wide_disc: Vec<u16> = "End Session".encode_utf16().collect();
                let mut disc_rc = RECT { left: 240, top: 67, right: 318, bottom: 95 };
                SetTextColor(hdc_mem, d_fg);
                DrawTextW(hdc_mem, &mut wide_disc, &mut disc_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                // 4. Expandable In-Session Chat (100..350)
                if chat_expanded {
                    let div3_pen = CreatePen(PS_SOLID, 1, PK_CARD_BORDER);
                    let old_d3 = SelectObject(hdc_mem, HGDIOBJ(div3_pen.0));
                    let _ = windows::Win32::Graphics::Gdi::MoveToEx(hdc_mem, 0, 101, None);
                    let _ = windows::Win32::Graphics::Gdi::LineTo(hdc_mem, w, 101);
                    let _ = SelectObject(hdc_mem, old_d3);
                    let _ = DeleteObject(HGDIOBJ(div3_pen.0));

                    let chat_bg_rc = RECT { left: 1, top: 102, right: w - 1, bottom: 300 };
                    let chat_brush = CreateSolidBrush(PK_INPUT_BG);
                    FillRect(hdc_mem, &chat_bg_rc, chat_brush);
                    let _ = DeleteObject(HGDIOBJ(chat_brush.0));

                    let msgs = crate::p2p::get_chat_messages();
                    let start_idx = msgs.len().saturating_sub(5);
                    let mut line_y = 108;

                    let font_msg = create_segoe_font(11, FW_NORMAL);
                    let font_msg_bold = create_segoe_font(10, FW_BOLD);

                    if msgs.is_empty() {
                        let mut wide_empty: Vec<u16> = "No messages yet. Send a message below.".encode_utf16().collect();
                        let mut empty_rc = RECT { left: 14, top: 160, right: w - 14, bottom: 190 };
                        let _ = SelectObject(hdc_mem, HGDIOBJ(font_msg.0));
                        SetTextColor(hdc_mem, PK_MUTED);
                        DrawTextW(hdc_mem, &mut wide_empty, &mut empty_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
                    } else {
                        for m in &msgs[start_idx..] {
                            if m.is_me {
                                // Host speech bubble (aligned right)
                                draw_rounded_box(hdc_mem, w - 210, line_y, 198, 32, PK_GREEN_TINT, PK_GREEN, 6);
                                let _ = SelectObject(hdc_mem, HGDIOBJ(font_msg_bold.0));
                                SetTextColor(hdc_mem, PK_GREEN_DARK);
                                let header = format!("You • {}", m.timestamp_str);
                                let mut wide_hdr: Vec<u16> = header.encode_utf16().collect();
                                let mut hdr_rc = RECT { left: w - 204, top: line_y + 2, right: w - 16, bottom: line_y + 16 };
                                DrawTextW(hdc_mem, &mut wide_hdr, &mut hdr_rc, DT_LEFT | DT_SINGLELINE);

                                let _ = SelectObject(hdc_mem, HGDIOBJ(font_msg.0));
                                SetTextColor(hdc_mem, PK_TEXT);
                                let mut wide_txt: Vec<u16> = m.text.encode_utf16().collect();
                                let mut txt_rc = RECT { left: w - 204, top: line_y + 16, right: w - 16, bottom: line_y + 30 };
                                DrawTextW(hdc_mem, &mut wide_txt, &mut txt_rc, DT_LEFT | DT_SINGLELINE | windows::Win32::Graphics::Gdi::DT_END_ELLIPSIS);
                            } else {
                                // Technician speech bubble (aligned left)
                                draw_rounded_box(hdc_mem, 12, line_y, 198, 32, PK_CARD, PK_CARD_BORDER, 6);
                                let _ = SelectObject(hdc_mem, HGDIOBJ(font_msg_bold.0));
                                SetTextColor(hdc_mem, PK_MUTED);
                                let header = format!("Technician • {}", m.timestamp_str);
                                let mut wide_hdr: Vec<u16> = header.encode_utf16().collect();
                                let mut hdr_rc = RECT { left: 18, top: line_y + 2, right: 206, bottom: line_y + 16 };
                                DrawTextW(hdc_mem, &mut wide_hdr, &mut hdr_rc, DT_LEFT | DT_SINGLELINE);

                                let _ = SelectObject(hdc_mem, HGDIOBJ(font_msg.0));
                                SetTextColor(hdc_mem, PK_TEXT);
                                let mut wide_txt: Vec<u16> = m.text.encode_utf16().collect();
                                let mut txt_rc = RECT { left: 18, top: line_y + 16, right: 206, bottom: line_y + 30 };
                                DrawTextW(hdc_mem, &mut wide_txt, &mut txt_rc, DT_LEFT | DT_SINGLELINE | windows::Win32::Graphics::Gdi::DT_END_ELLIPSIS);
                            }
                            line_y += 38;
                        }
                    }

                    let _ = DeleteObject(HGDIOBJ(font_msg.0));
                    let _ = DeleteObject(HGDIOBJ(font_msg_bold.0));

                    // Send Button
                    let send_btn_x = 264;
                    let send_btn_y = 308;
                    let send_btn_w = 56;
                    let send_btn_h = 28;

                    let (s_bg, s_border, s_fg) = if hover == 4 {
                        (PK_GREEN_DARK, PK_GREEN_DARK, COLORREF(0x00FFFFFF))
                    } else {
                        (PK_GREEN, PK_GREEN, COLORREF(0x00FFFFFF))
                    };
                    draw_rounded_box(hdc_mem, send_btn_x, send_btn_y, send_btn_w, send_btn_h, s_bg, s_border, 6);

                    let font_send = create_segoe_font(11, FW_BOLD);
                    let _ = SelectObject(hdc_mem, HGDIOBJ(font_send.0));
                    SetTextColor(hdc_mem, s_fg);
                    let mut wide_send: Vec<u16> = "Send".encode_utf16().collect();
                    let mut send_rc = RECT { left: send_btn_x, top: send_btn_y, right: send_btn_x + send_btn_w, bottom: send_btn_y + send_btn_h };
                    DrawTextW(hdc_mem, &mut wide_send, &mut send_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
                    let _ = DeleteObject(HGDIOBJ(font_send.0));
                }

                let _ = SelectObject(hdc_mem, old_font);
                let _ = DeleteObject(HGDIOBJ(font_title.0));
                let _ = DeleteObject(HGDIOBJ(font_sub.0));
                let _ = DeleteObject(HGDIOBJ(font_label.0));
                let _ = DeleteObject(HGDIOBJ(font_id.0));
                let _ = DeleteObject(HGDIOBJ(font_timer.0));
                let _ = DeleteObject(HGDIOBJ(font_btn.0));
            }

            let _ = BitBlt(hdc, 0, 0, w, h, Some(hdc_mem), 0, 0, SRCCOPY);

            let _ = SelectObject(hdc_mem, old_bmp);
            let _ = DeleteObject(HGDIOBJ(hbm_mem.0));
            let _ = windows::Win32::Graphics::Gdi::DeleteDC(hdc_mem);

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DESTROY => {
            HWND_HOST_WIDGET.store(0, Ordering::Relaxed);
            HWND_CHAT_EDIT.store(0, Ordering::Relaxed);
            CHAT_EXPANDED.store(false, Ordering::Relaxed);
            COLLAPSED.store(false, Ordering::Relaxed);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn send_host_chat_input() {
    let edit_raw = HWND_CHAT_EDIT.load(Ordering::Relaxed);
    if edit_raw != 0 {
        let eh = HWND(edit_raw as *mut _);
        unsafe {
            let len = GetWindowTextLengthW(eh);
            if len > 0 {
                let mut buf = vec![0u16; (len + 1) as usize];
                let _ = GetWindowTextW(eh, &mut buf);
                let text = String::from_utf16_lossy(&buf[..len as usize]);
                if !text.trim().is_empty() {
                    crate::p2p::send_chat_message(&text);
                    let _ = SetWindowTextW(eh, w!(""));
                }
            }
        }
    }
}

unsafe extern "system" fn screen_border_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let w = rc.right - rc.left;
            let h = rc.bottom - rc.top;

            let border_brush = CreateSolidBrush(PK_GREEN);
            let thickness = 4;

            // Top border
            let top_rc = RECT { left: 0, top: 0, right: w, bottom: thickness };
            FillRect(hdc, &top_rc, border_brush);

            // Bottom border
            let btm_rc = RECT { left: 0, top: h - thickness, right: w, bottom: h };
            FillRect(hdc, &btm_rc, border_brush);

            // Left border
            let left_rc = RECT { left: 0, top: 0, right: thickness, bottom: h };
            FillRect(hdc, &left_rc, border_brush);

            // Right border
            let right_rc = RECT { left: w - thickness, top: 0, right: w, bottom: h };
            FillRect(hdc, &right_rc, border_brush);

            let _ = DeleteObject(HGDIOBJ(border_brush.0));
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_DESTROY => {
            HWND_SCREEN_BORDER.store(0, Ordering::Relaxed);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
