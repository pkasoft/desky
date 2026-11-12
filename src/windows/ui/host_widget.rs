//! Host-Side Floating Session Widget & Screen Sharing Indicator (Win32)
//!
//! Provides the controlled user with live awareness and complete control during remote sessions:
//! - Always-on-top draggable floating session pill above the Windows taskbar.
//! - Live session timer (`00:14:22`) and remote controller ID.
//! - Expandable real-time two-way in-session chat with unread badges.
//! - One-click instant session termination ("Disconnect").
//! - Non-intrusive full-screen click-through privacy border indicator.

#![allow(unsafe_op_in_unsafe_fn)]

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, Ordering};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreatePen, CreateSolidBrush,
    DeleteObject, DrawTextW, EndPaint, FillRect, GetStockObject, InvalidateRect, Rectangle,
    SelectObject, SetBkColor, SetBkMode, SetTextColor, DT_CENTER, DT_LEFT, DT_SINGLELINE,
    DT_VCENTER, FW_BOLD, FW_NORMAL, FW_SEMIBOLD, HDC, HGDIOBJ, NULL_BRUSH, PAINTSTRUCT, PS_SOLID,
    SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos, GetParent, GetSystemMetrics,
    GetWindowRect, GetWindowTextLengthW, GetWindowTextW, IsWindow, LoadCursorW, MoveWindow,
    RegisterClassExW, SendMessageW, SetCursor, SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW, SetWindowTextW,
    ShowWindow, GWLP_WNDPROC, HMENU, HTCAPTION, IDC_ARROW, IDC_HAND, LWA_COLORKEY, SM_CXSCREEN, SM_CYSCREEN,
    SW_HIDE, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CHAR, WM_COMMAND, WM_CREATE, WM_CTLCOLOREDIT,
    WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_NCLBUTTONDOWN, WM_PAINT, WM_SETCURSOR, WM_TIMER,
    WNDCLASSEXW, WS_BORDER, WS_CHILD, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP, WS_TABSTOP, WS_VISIBLE,
};

use super::theme::{
    create_segoe_font, draw_rounded_box, PK_BTN_HOVER, PK_CARD, PK_CARD_BORDER,
    PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_RED, PK_TEXT,
};

pub const HOST_WIDGET_WIDTH: i32 = 440;
pub const HOST_WIDGET_PILL_H: i32 = 50;
pub const HOST_WIDGET_EXPANDED_H: i32 = 320;

const ID_EDIT_HOST_CHAT: usize = 9981;
const ID_TIMER_WIDGET_TICK: usize = 4001;

static HWND_HOST_WIDGET: AtomicIsize = AtomicIsize::new(0);
static HWND_SCREEN_BORDER: AtomicIsize = AtomicIsize::new(0);
static HWND_CHAT_EDIT: AtomicIsize = AtomicIsize::new(0);
static PREV_HOST_CHAT_WNDPROC: AtomicIsize = AtomicIsize::new(0);
static CHAT_EXPANDED: AtomicBool = AtomicBool::new(false);
static HOVER_BTN: AtomicI32 = AtomicI32::new(0); // 1 = Chat, 2 = Disconnect, 3 = Send, 4 = Mic

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
        let x = (screen_w - HOST_WIDGET_WIDTH - 24).max(10);
        let y = (screen_h - HOST_WIDGET_PILL_H - 64).max(10);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("Desky | Remote Session Active"),
            WS_POPUP | WS_VISIBLE,
            x,
            y,
            HOST_WIDGET_WIDTH,
            HOST_WIDGET_PILL_H,
            None,
            None,
            Some(h_instance),
            None,
        );

        if let Ok(h) = hwnd {
            HWND_HOST_WIDGET.store(h.0 as isize, Ordering::Relaxed);
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
                12,
                278,
                HOST_WIDGET_WIDTH - 84,
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

            let mic_btn_x = HOST_WIDGET_WIDTH - 228;
            let mic_btn_w = 68;
            let chat_btn_x = HOST_WIDGET_WIDTH - 152;
            let chat_btn_w = 68;
            let disc_btn_x = HOST_WIDGET_WIDTH - 76;
            let disc_btn_w = 64;

            // 0. Mic Toggle Button Click
            if client_x >= mic_btn_x
                && client_x <= mic_btn_x + mic_btn_w
                && client_y >= 10
                && client_y <= 40
            {
                let is_on = crate::windows::audio::toggle_host_mic();
                tracing::info!("Host microphone toggled: {}", is_on);
                let _ = InvalidateRect(Some(hwnd), None, false);
                return LRESULT(0);
            }

            // 1. Chat Toggle Button Click
            if client_x >= chat_btn_x
                && client_x <= chat_btn_x + chat_btn_w
                && client_y >= 10
                && client_y <= 40
            {
                let expanded = !CHAT_EXPANDED.load(Ordering::Relaxed);
                CHAT_EXPANDED.store(expanded, Ordering::Relaxed);
                crate::p2p::mark_chat_read();

                let mut rc = RECT::default();
                let _ = GetWindowRect(hwnd, &mut rc);
                let cur_x = rc.left;
                let cur_y = rc.top;

                let (_target_h, edit_show) = if expanded {
                    // Expand upwards if near bottom of screen
                    let target_y = (cur_y - (HOST_WIDGET_EXPANDED_H - HOST_WIDGET_PILL_H)).max(10);
                    let _ = MoveWindow(hwnd, cur_x, target_y, HOST_WIDGET_WIDTH, HOST_WIDGET_EXPANDED_H, true);
                    (HOST_WIDGET_EXPANDED_H, SW_SHOW)
                } else {
                    let target_y = cur_y + (HOST_WIDGET_EXPANDED_H - HOST_WIDGET_PILL_H);
                    let _ = MoveWindow(hwnd, cur_x, target_y, HOST_WIDGET_WIDTH, HOST_WIDGET_PILL_H, true);
                    (HOST_WIDGET_PILL_H, SW_HIDE)
                };

                let edit_raw = HWND_CHAT_EDIT.load(Ordering::Relaxed);
                if edit_raw != 0 {
                    let eh = HWND(edit_raw as *mut _);
                    let _ = ShowWindow(eh, edit_show);
                    if expanded {
                        let _ = SetFocus(Some(eh));
                    }
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                return LRESULT(0);
            }

            // 2. Disconnect Button Click
            if client_x >= disc_btn_x
                && client_x <= disc_btn_x + disc_btn_w
                && client_y >= 10
                && client_y <= 40
            {
                crate::p2p::disconnect_host_session();
                close_host_widget();
                close_screen_border();
                return LRESULT(0);
            }

            // 3. Send Message Button (When chat is expanded)
            if CHAT_EXPANDED.load(Ordering::Relaxed) {
                let send_btn_x = HOST_WIDGET_WIDTH - 64;
                let send_btn_y = 277;
                let send_btn_w = 52;
                let send_btn_h = 28;

                if client_x >= send_btn_x
                    && client_x <= send_btn_x + send_btn_w
                    && client_y >= send_btn_y
                    && client_y <= send_btn_y + send_btn_h
                {
                    send_host_chat_input();
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
            }

            // 4. Header Bar Draggable
            if client_y <= HOST_WIDGET_PILL_H {
                let _ = ReleaseCapture();
                let _ = SendMessageW(hwnd, WM_NCLBUTTONDOWN, Some(WPARAM(HTCAPTION as usize)), Some(LPARAM(0)));
                return LRESULT(0);
            }

            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            let mic_btn_x = HOST_WIDGET_WIDTH - 228;
            let mic_btn_w = 68;
            let chat_btn_x = HOST_WIDGET_WIDTH - 152;
            let chat_btn_w = 68;
            let disc_btn_x = HOST_WIDGET_WIDTH - 76;
            let disc_btn_w = 64;

            let prev_hover = HOVER_BTN.load(Ordering::Relaxed);
            let hover = if client_x >= mic_btn_x
                && client_x <= mic_btn_x + mic_btn_w
                && client_y >= 10
                && client_y <= 40
            {
                4
            } else if client_x >= chat_btn_x
                && client_x <= chat_btn_x + chat_btn_w
                && client_y >= 10
                && client_y <= 40
            {
                1
            } else if client_x >= disc_btn_x
                && client_x <= disc_btn_x + disc_btn_w
                && client_y >= 10
                && client_y <= 40
            {
                2
            } else if CHAT_EXPANDED.load(Ordering::Relaxed)
                && client_x >= HOST_WIDGET_WIDTH - 64
                && client_x <= HOST_WIDGET_WIDTH - 12
                && client_y >= 277
                && client_y <= 305
            {
                3
            } else {
                0
            };

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

            // Background card & border
            let bg_brush = CreateSolidBrush(PK_CARD);
            FillRect(hdc_mem, &rc, bg_brush);
            let _ = DeleteObject(HGDIOBJ(bg_brush.0));

            let border_pen = CreatePen(PS_SOLID, 1, PK_CARD_BORDER);
            let old_pen = SelectObject(hdc_mem, HGDIOBJ(border_pen.0));
            let old_brush = SelectObject(hdc_mem, HGDIOBJ(GetStockObject(NULL_BRUSH).0));
            let _ = Rectangle(hdc_mem, 0, 0, w, h);
            let _ = SelectObject(hdc_mem, old_brush);
            let _ = SelectObject(hdc_mem, old_pen);
            let _ = DeleteObject(HGDIOBJ(border_pen.0));

            // Render Header contents (Pill)
            let session_info = crate::p2p::get_active_host_session_info();
            let (client_id, elapsed_secs) = session_info.unwrap_or_else(|| ("Client".to_string(), 0));

            // 1. Live Emerald Pulse Dot
            let dot_pen = CreatePen(PS_SOLID, 1, PK_GREEN_DARK);
            let dot_brush = CreateSolidBrush(PK_GREEN);
            let old_dp = SelectObject(hdc_mem, HGDIOBJ(dot_pen.0));
            let old_db = SelectObject(hdc_mem, HGDIOBJ(dot_brush.0));
            let _ = windows::Win32::Graphics::Gdi::Ellipse(hdc_mem, 14, 18, 26, 30);
            let _ = SelectObject(hdc_mem, old_dp);
            let _ = SelectObject(hdc_mem, old_db);
            let _ = DeleteObject(HGDIOBJ(dot_pen.0));
            let _ = DeleteObject(HGDIOBJ(dot_brush.0));

            // 2. Titles: "Controlled by" and Session Duration
            SetBkMode(hdc_mem, TRANSPARENT);
            let font_title = create_segoe_font(12, FW_BOLD);
            let old_font = SelectObject(hdc_mem, HGDIOBJ(font_title.0));
            SetTextColor(hdc_mem, PK_TEXT);

            let title_text = format!("Controlled by: {}", client_id);
            let mut wide_title: Vec<u16> = title_text.encode_utf16().collect();
            let mut title_rc = RECT {
                left: 32,
                top: 8,
                right: HOST_WIDGET_WIDTH - 232,
                bottom: 26,
            };
            DrawTextW(hdc_mem, &mut wide_title, &mut title_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            let font_sub = create_segoe_font(11, FW_SEMIBOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_sub.0));
            SetTextColor(hdc_mem, PK_GREEN_DARK);

            let timer_str = format!("⏱ Session: {}", format_duration(elapsed_secs));
            let mut wide_timer: Vec<u16> = timer_str.encode_utf16().collect();
            let mut timer_rc = RECT {
                left: 32,
                top: 26,
                right: HOST_WIDGET_WIDTH - 232,
                bottom: 44,
            };
            DrawTextW(hdc_mem, &mut wide_timer, &mut timer_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            // 2.5 Mic Button
            let hover = HOVER_BTN.load(Ordering::Relaxed);
            let is_mic = crate::windows::audio::is_host_mic_active();
            let mic_btn_x = HOST_WIDGET_WIDTH - 228;
            let mic_btn_y = 10;
            let mic_btn_w = 68;
            let mic_btn_h = 30;

            let (m_bg, m_border, m_fg) = if is_mic {
                (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
            } else if hover == 4 {
                (PK_BTN_HOVER, PK_CARD_BORDER, PK_TEXT)
            } else {
                (PK_CARD, PK_CARD_BORDER, PK_MUTED)
            };
            draw_rounded_box(hdc_mem, mic_btn_x, mic_btn_y, mic_btn_w, mic_btn_h, m_bg, m_border, 6);
            if is_mic {
                crate::ui::draw_mic(hdc_mem, mic_btn_x + 8, mic_btn_y + 7, 16, m_fg, 2);
            } else {
                crate::ui::draw_mic_off(hdc_mem, mic_btn_x + 8, mic_btn_y + 7, 16, m_fg, 2);
            }

            let mic_label = if is_mic { "Mic ON" } else { "Mic" };
            let mut wide_mic: Vec<u16> = mic_label.encode_utf16().collect();
            let mut mic_rc = RECT {
                left: mic_btn_x + 24,
                top: mic_btn_y,
                right: mic_btn_x + mic_btn_w,
                bottom: mic_btn_y + mic_btn_h,
            };
            let font_btn = create_segoe_font(11, FW_SEMIBOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_btn.0));
            SetTextColor(hdc_mem, m_fg);
            DrawTextW(hdc_mem, &mut wide_mic, &mut mic_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

            // 3. Chat Button
            let chat_expanded = CHAT_EXPANDED.load(Ordering::Relaxed);
            let unread = crate::p2p::get_unread_count();

            let chat_btn_x = HOST_WIDGET_WIDTH - 152;
            let chat_btn_y = 10;
            let chat_btn_w = 68;
            let chat_btn_h = 30;

            let (c_bg, c_border, c_fg) = if chat_expanded || unread > 0 {
                (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
            } else if hover == 1 {
                (PK_BTN_HOVER, PK_CARD_BORDER, PK_TEXT)
            } else {
                (PK_CARD, PK_CARD_BORDER, PK_MUTED)
            };
            draw_rounded_box(hdc_mem, chat_btn_x, chat_btn_y, chat_btn_w, chat_btn_h, c_bg, c_border, 6);
            crate::ui::draw_message_square(hdc_mem, chat_btn_x + 8, chat_btn_y + 7, 16, c_fg, 2);

            let chat_label = if unread > 0 {
                format!("Chat ({})", unread)
            } else {
                "Chat".to_string()
            };
            let mut wide_chat: Vec<u16> = chat_label.encode_utf16().collect();
            let mut chat_rc = RECT {
                left: chat_btn_x + 26,
                top: chat_btn_y,
                right: chat_btn_x + chat_btn_w,
                bottom: chat_btn_y + chat_btn_h,
            };
            let font_btn = create_segoe_font(11, FW_SEMIBOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_btn.0));
            SetTextColor(hdc_mem, c_fg);
            DrawTextW(hdc_mem, &mut wide_chat, &mut chat_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

            // 4. Disconnect Button
            let disc_btn_x = HOST_WIDGET_WIDTH - 76;
            let disc_btn_y = 10;
            let disc_btn_w = 64;
            let disc_btn_h = 30;

            let (d_bg, d_border, d_fg) = if hover == 2 {
                (PK_RED, PK_RED, COLORREF(0x00FFFFFF))
            } else {
                (COLORREF(0x00EEF0FF), PK_RED, PK_RED)
            };
            draw_rounded_box(hdc_mem, disc_btn_x, disc_btn_y, disc_btn_w, disc_btn_h, d_bg, d_border, 6);
            crate::ui::draw_x(hdc_mem, disc_btn_x + 7, disc_btn_y + 8, 14, d_fg, 2);

            let mut wide_disc: Vec<u16> = "End".encode_utf16().collect();
            let mut disc_rc = RECT {
                left: disc_btn_x + 22,
                top: disc_btn_y,
                right: disc_btn_x + disc_btn_w - 4,
                bottom: disc_btn_y + disc_btn_h,
            };
            SetTextColor(hdc_mem, d_fg);
            DrawTextW(hdc_mem, &mut wide_disc, &mut disc_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

            // 5. Render Chat Box (if expanded)
            if chat_expanded {
                let div_pen = CreatePen(PS_SOLID, 1, PK_CARD_BORDER);
                let _ = SelectObject(hdc_mem, HGDIOBJ(div_pen.0));
                let _ = windows::Win32::Graphics::Gdi::MoveToEx(hdc_mem, 0, HOST_WIDGET_PILL_H, None);
                let _ = windows::Win32::Graphics::Gdi::LineTo(hdc_mem, w, HOST_WIDGET_PILL_H);
                let _ = DeleteObject(HGDIOBJ(div_pen.0));

                let chat_rc_bg = RECT {
                    left: 1,
                    top: HOST_WIDGET_PILL_H + 1,
                    right: w - 1,
                    bottom: 270,
                };
                let chat_brush = CreateSolidBrush(PK_INPUT_BG);
                FillRect(hdc_mem, &chat_rc_bg, chat_brush);
                let _ = DeleteObject(HGDIOBJ(chat_brush.0));

                // Draw last messages
                let msgs = crate::p2p::get_chat_messages();
                let start_idx = msgs.len().saturating_sub(6);
                let mut line_y = HOST_WIDGET_PILL_H + 10;

                let font_msg = create_segoe_font(11, FW_NORMAL);
                let font_msg_bold = create_segoe_font(11, FW_BOLD);

                if msgs.is_empty() {
                    let mut wide_empty: Vec<u16> = "No messages yet. Send a message below.".encode_utf16().collect();
                    let mut empty_rc = RECT {
                        left: 14,
                        top: 130,
                        right: w - 14,
                        bottom: 160,
                    };
                    let _ = SelectObject(hdc_mem, HGDIOBJ(font_msg.0));
                    SetTextColor(hdc_mem, PK_MUTED);
                    DrawTextW(hdc_mem, &mut wide_empty, &mut empty_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
                } else {
                    for m in &msgs[start_idx..] {
                        let sender_str = if m.is_me { "You:" } else { "Remote:" };
                        let sender_color = if m.is_me { PK_GREEN_DARK } else { PK_TEXT };

                        let _ = SelectObject(hdc_mem, HGDIOBJ(font_msg_bold.0));
                        SetTextColor(hdc_mem, sender_color);

                        let mut wide_sender: Vec<u16> = sender_str.encode_utf16().collect();
                        let mut sender_rc = RECT {
                            left: 14,
                            top: line_y,
                            right: 76,
                            bottom: line_y + 18,
                        };
                        DrawTextW(hdc_mem, &mut wide_sender, &mut sender_rc, DT_LEFT | DT_SINGLELINE);

                        let _ = SelectObject(hdc_mem, HGDIOBJ(font_msg.0));
                        SetTextColor(hdc_mem, PK_TEXT);

                        let text_str = format!("{} [{}]", m.text, m.timestamp_str);
                        let mut wide_text: Vec<u16> = text_str.encode_utf16().collect();
                        let mut text_rc = RECT {
                            left: 78,
                            top: line_y,
                            right: w - 14,
                            bottom: line_y + 18,
                        };
                        DrawTextW(hdc_mem, &mut wide_text, &mut text_rc, DT_LEFT | DT_SINGLELINE | windows::Win32::Graphics::Gdi::DT_END_ELLIPSIS);

                        line_y += 34;
                    }
                }

                let _ = DeleteObject(HGDIOBJ(font_msg.0));
                let _ = DeleteObject(HGDIOBJ(font_msg_bold.0));

                // Send Button
                let send_btn_x = w - 64;
                let send_btn_y = 277;
                let send_btn_w = 52;
                let send_btn_h = 29;

                let (s_bg, s_border, s_fg) = if hover == 3 {
                    (PK_GREEN_DARK, PK_GREEN_DARK, COLORREF(0x00FFFFFF))
                } else {
                    (PK_GREEN, PK_GREEN, COLORREF(0x00FFFFFF))
                };
                draw_rounded_box(hdc_mem, send_btn_x, send_btn_y, send_btn_w, send_btn_h, s_bg, s_border, 6);

                let font_send = create_segoe_font(11, FW_BOLD);
                let _ = SelectObject(hdc_mem, HGDIOBJ(font_send.0));
                SetTextColor(hdc_mem, s_fg);

                let mut wide_send: Vec<u16> = "Send".encode_utf16().collect();
                let mut send_rc = RECT {
                    left: send_btn_x,
                    top: send_btn_y,
                    right: send_btn_x + send_btn_w,
                    bottom: send_btn_y + send_btn_h,
                };
                DrawTextW(hdc_mem, &mut wide_send, &mut send_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
                let _ = DeleteObject(HGDIOBJ(font_send.0));
            }

            let _ = BitBlt(hdc, 0, 0, w, h, Some(hdc_mem), 0, 0, SRCCOPY);

            let _ = SelectObject(hdc_mem, old_font);
            let _ = DeleteObject(HGDIOBJ(font_title.0));
            let _ = DeleteObject(HGDIOBJ(font_sub.0));
            let _ = DeleteObject(HGDIOBJ(font_btn.0));

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

            // Fill black (which is made transparent via LWA_COLORKEY)
            let black_brush = CreateSolidBrush(COLORREF(0x00000000));
            FillRect(hdc, &rc, black_brush);
            let _ = DeleteObject(HGDIOBJ(black_brush.0));

            // Draw 3px emerald border along the edges of the screen
            let border_thickness = 3;
            let border_brush = CreateSolidBrush(PK_GREEN);

            let top_rc = RECT { left: 0, top: 0, right: w, bottom: border_thickness };
            let bottom_rc = RECT { left: 0, top: h - border_thickness, right: w, bottom: h };
            let left_rc = RECT { left: 0, top: 0, right: border_thickness, bottom: h };
            let right_rc = RECT { left: w - border_thickness, top: 0, right: w, bottom: h };

            FillRect(hdc, &top_rc, border_brush);
            FillRect(hdc, &bottom_rc, border_brush);
            FillRect(hdc, &left_rc, border_brush);
            FillRect(hdc, &right_rc, border_brush);

            let _ = DeleteObject(HGDIOBJ(border_brush.0));
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DESTROY => {
            HWND_SCREEN_BORDER.store(0, Ordering::Relaxed);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
