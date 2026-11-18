//! Host Session Window & Approval Controller (Win32)
//!
//! Provides the host workstation user with a dedicated, unified session manager:
//! - Standalone topmost window that immediately surfaces when an incoming connection request arrives.
//! - Clear requester profile identification, connection status, and permission indicators.
//! - Single persistent window lifecycle: upon clicking [Accept], the window seamlessly transitions
//!   into the active session controller with live duration ticker, in-session chat, and [Disconnect].
//! - Remote Input Sovereignty: instant toggle between remote mouse/keyboard control and View-Only mode.
//! - Integrated In-Session Chat stream with real-time speech bubbles and typing input.
//! - Host microphone toggle and one-click session termination.

#![allow(unsafe_op_in_unsafe_fn)]

use std::sync::atomic::{AtomicI32, AtomicIsize, Ordering};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreatePen, CreateRoundRectRgn,
    CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetStockObject, InvalidateRect,
    RoundRect, SelectObject, SetBkColor, SetBkMode, SetTextColor, SetWindowRgn, UpdateWindow,
    DT_CENTER, DT_LEFT, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL, FW_SEMIBOLD, HDC, HGDIOBJ,
    NULL_BRUSH, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos, GetParent, GetSystemMetrics,
    GetWindowTextLengthW, GetWindowTextW, IsWindow, LoadCursorW,
    PostMessageW, RegisterClassExW, SendMessageW, SetCursor, SetForegroundWindow,
    SetTimer, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow, GWLP_WNDPROC, HMENU, HTCAPTION,
    HWND_TOPMOST, IDC_ARROW, IDC_HAND, SM_CXSCREEN, SM_CYSCREEN,
    SWP_SHOWWINDOW, SW_MINIMIZE, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CHAR, WM_COMMAND, WM_CREATE,
    WM_CTLCOLOREDIT, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEMOVE,
    WM_NCLBUTTONDOWN, WM_PAINT, WM_SETCURSOR, WM_TIMER, WNDCLASSEXW, WS_BORDER, WS_CHILD,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_TABSTOP, WS_VISIBLE,
};

use super::theme::{
    create_segoe_font, draw_rounded_box, PK_BTN_HOVER, PK_CARD, PK_CARD_BORDER,
    PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_RED, PK_TEXT,
};

pub const HOST_WINDOW_W: i32 = 380;
pub const HOST_WINDOW_H: i32 = 524;

const ID_EDIT_HOST_CHAT: usize = 9981;
const ID_TIMER_WIDGET_TICK: usize = 4001;

static HWND_HOST_WIDGET: AtomicIsize = AtomicIsize::new(0);
static HWND_CHAT_EDIT: AtomicIsize = AtomicIsize::new(0);
static PREV_HOST_CHAT_WNDPROC: AtomicIsize = AtomicIsize::new(0);
static HOVER_BTN: AtomicI32 = AtomicI32::new(0);
// Hover button IDs:
// 1 = Mic, 3 = Disconnect / End, 4 = Send, 5 = Input Sovereignty
// 10 = Minimize, 11 = Header Close
// 20 = Approval Accept, 21 = Approval Dismiss

pub fn is_approval_active() -> bool {
    crate::p2p::get_pending_approval().is_some() && crate::p2p::get_active_host_session_info().is_none()
}

unsafe fn apply_widget_shape(hwnd: HWND, w: i32, h: i32) {
    let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, 14, 14);
    let _ = SetWindowRgn(hwnd, Some(rgn), true);
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
                return LRESULT(0); // Suppress Windows default beep on Enter
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

/// Synchronizes the visibility and state of the Host Session Window.
/// Guarantees execution strictly on the Win32 UI thread.
pub fn sync_host_session_ui() {
    let ui_tid = super::launcher::UI_THREAD_ID.load(Ordering::Relaxed);
    let cur_tid = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    if ui_tid != 0 && cur_tid != ui_tid {
        // Forward to the Win32 UI thread via message queue to ensure thread safety
        unsafe {
            let main_hwnd = super::launcher::HWND_MAIN_WINDOW;
            if !main_hwnd.0.is_null() && IsWindow(Some(main_hwnd)).as_bool() {
                let _ = PostMessageW(
                    Some(main_hwnd),
                    super::launcher::WM_APP_SYNC_HOST_UI,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
        return;
    }

    let session = crate::p2p::get_active_host_session_info();
    let pending = crate::p2p::get_pending_approval();

    if session.is_some() || pending.is_some() {
        spawn_host_widget();
    } else {
        close_host_widget();
    }
}

pub fn spawn_host_widget() {
    let ui_tid = super::launcher::UI_THREAD_ID.load(Ordering::Relaxed);
    let cur_tid = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    if ui_tid != 0 && cur_tid != ui_tid {
        unsafe {
            let main_hwnd = super::launcher::HWND_MAIN_WINDOW;
            if !main_hwnd.0.is_null() && IsWindow(Some(main_hwnd)).as_bool() {
                let _ = PostMessageW(
                    Some(main_hwnd),
                    super::launcher::WM_APP_SYNC_HOST_UI,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
        return;
    }

    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let x = (screen_w - HOST_WINDOW_W) / 2;
    let y = (screen_h - HOST_WINDOW_H) / 2;

    let existing = HWND_HOST_WIDGET.load(Ordering::Relaxed);
    if existing != 0 {
        let hwnd = HWND(existing as *mut _);
        if unsafe { IsWindow(Some(hwnd)).as_bool() } {
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    x,
                    y,
                    HOST_WINDOW_W,
                    HOST_WINDOW_H,
                    SWP_SHOWWINDOW,
                );
                let _ = SetForegroundWindow(hwnd);
                let _ = InvalidateRect(Some(hwnd), None, false);
                let _ = UpdateWindow(hwnd);
            }
            return;
        }
    }

    unsafe {
        let h_module = GetModuleHandleW(None).unwrap_or_default();
        let h_instance: HINSTANCE = h_module.into();
        let class_name = w!("DeskyHostSessionWindowClass");

        let mut wc = WNDCLASSEXW::default();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(host_widget_wndproc);
        wc.hInstance = h_instance;
        wc.lpszClassName = class_name;
        wc.hbrBackground = CreateSolidBrush(PK_CARD);
        wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        let _ = RegisterClassExW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("Desky | Host Session"),
            WS_POPUP,
            x,
            y,
            HOST_WINDOW_W,
            HOST_WINDOW_H,
            None,
            None,
            Some(h_instance),
            None,
        );

        if let Ok(h) = hwnd {
            HWND_HOST_WIDGET.store(h.0 as isize, Ordering::Relaxed);
            apply_widget_shape(h, HOST_WINDOW_W, HOST_WINDOW_H);
            let _ = ShowWindow(h, SW_SHOW);
            let _ = SetWindowPos(
                h,
                Some(HWND_TOPMOST),
                x,
                y,
                HOST_WINDOW_W,
                HOST_WINDOW_H,
                SWP_SHOWWINDOW,
            );
            let _ = SetForegroundWindow(h);
            let _ = SetTimer(Some(h), ID_TIMER_WIDGET_TICK, 400, None);
            let _ = InvalidateRect(Some(h), None, false);
            let _ = UpdateWindow(h);
        }
    }
}

pub fn close_host_widget() {
    let ui_tid = super::launcher::UI_THREAD_ID.load(Ordering::Relaxed);
    let cur_tid = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    if ui_tid != 0 && cur_tid != ui_tid {
        unsafe {
            let main_hwnd = super::launcher::HWND_MAIN_WINDOW;
            if !main_hwnd.0.is_null() && IsWindow(Some(main_hwnd)).as_bool() {
                let _ = PostMessageW(
                    Some(main_hwnd),
                    super::launcher::WM_APP_SYNC_HOST_UI,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
        return;
    }

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
}

// Deprecated stubs preserved for module compatibility
#[allow(dead_code)]
pub fn spawn_screen_border() {}
#[allow(dead_code)]
pub fn close_screen_border() {}

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
                    WS_CHILD.0 | WS_BORDER.0 | WS_TABSTOP.0 | 0x0080 | WS_VISIBLE.0, // ES_AUTOHSCROLL | WS_VISIBLE
                ),
                14,
                476,
                284,
                32,
                Some(hwnd),
                Some(HMENU(ID_EDIT_HOST_CHAT as *mut _)),
                Some(h_instance),
                None,
            );

            if let Ok(eh) = edit_hwnd {
                let font = create_segoe_font(12, FW_NORMAL);
                let _ = SendMessageW(
                    eh,
                    windows::Win32::UI::WindowsAndMessaging::WM_SETFONT,
                    Some(WPARAM(font.0 as usize)),
                    Some(LPARAM(1)),
                );
                let old = SetWindowLongPtrW(
                    eh,
                    GWLP_WNDPROC,
                    host_chat_edit_proc as *const () as usize as isize,
                );
                if old != 0 {
                    PREV_HOST_CHAT_WNDPROC.store(old, Ordering::Relaxed);
                }
                HWND_CHAT_EDIT.store(eh.0 as isize, Ordering::Relaxed);
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
                if crate::p2p::get_active_host_session_info().is_none()
                    && crate::p2p::get_pending_approval().is_none()
                {
                    close_host_widget();
                } else {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let is_pending = is_approval_active();

            // 1. Header controls (y <= 34)
            if client_y <= 34 {
                if client_x >= 318 && client_x <= 342 {
                    // Minimize
                    let _ = ShowWindow(hwnd, SW_MINIMIZE);
                    return LRESULT(0);
                } else if client_x >= 346 && client_x <= 372 {
                    // Close / Dismiss / End Session
                    if is_pending {
                        crate::p2p::respond_to_pending_approval(false);
                    } else {
                        crate::p2p::disconnect_host_session();
                    }
                    close_host_widget();
                    return LRESULT(0);
                }
                // Header drag
                let _ = ReleaseCapture();
                let _ = SendMessageW(
                    hwnd,
                    WM_NCLBUTTONDOWN,
                    Some(WPARAM(HTCAPTION as usize)),
                    Some(LPARAM(0)),
                );
                return LRESULT(0);
            }

            // 2. Permission Toggles (y: 134..172)
            if client_y >= 134 && client_y <= 172 {
                // Mouse & Keyboard (Remote Input Sovereignty): x: 102..186
                if client_x >= 102 && client_x <= 186 {
                    let allowed = crate::p2p::toggle_host_remote_input();
                    tracing::info!("Host remote input sovereignty toggled: allowed={}", allowed);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                // Mic toggle: x: 190..270
                if client_x >= 190 && client_x <= 270 {
                    let is_on = crate::windows::audio::toggle_host_mic();
                    tracing::info!("Host microphone toggled: {}", is_on);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
            }

            // 3. Primary Action Buttons (y: 178..224)
            if client_y >= 178 && client_y <= 224 {
                if is_pending {
                    // [ Accept ] Button (x: 14..186)
                    if client_x >= 14 && client_x <= 186 {
                        crate::p2p::respond_to_pending_approval(true);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    // [ Dismiss ] Button (x: 194..366)
                    if client_x >= 194 && client_x <= 366 {
                        crate::p2p::respond_to_pending_approval(false);
                        close_host_widget();
                        return LRESULT(0);
                    }
                } else {
                    // [ Disconnect Session ] Button (x: 14..366)
                    if client_x >= 14 && client_x <= 366 {
                        crate::p2p::disconnect_host_session();
                        close_host_widget();
                        return LRESULT(0);
                    }
                }
            }

            // 4. Send Message Button (x: 304..366, y: 476..508)
            if client_x >= 304 && client_x <= 366 && client_y >= 476 && client_y <= 508 {
                send_host_chat_input();
                let _ = InvalidateRect(Some(hwnd), None, false);
                return LRESULT(0);
            }

            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let is_pending = is_approval_active();

            let hover = if client_y <= 34 {
                if client_x >= 318 && client_x <= 342 {
                    10 // Minimize
                } else if client_x >= 346 && client_x <= 372 {
                    11 // Header Close
                } else {
                    0
                }
            } else if client_y >= 134 && client_y <= 172 {
                if client_x >= 102 && client_x <= 186 {
                    5 // Input Sovereignty toggle
                } else if client_x >= 190 && client_x <= 270 {
                    1 // Mic toggle
                } else {
                    0
                }
            } else if client_y >= 178 && client_y <= 224 {
                if is_pending {
                    if client_x >= 14 && client_x <= 186 {
                        20 // Accept
                    } else if client_x >= 194 && client_x <= 366 {
                        21 // Dismiss
                    } else {
                        0
                    }
                } else if client_x >= 14 && client_x <= 366 {
                    3 // Disconnect Session
                } else {
                    0
                }
            } else if client_x >= 304 && client_x <= 366 && client_y >= 476 && client_y <= 508 {
                4 // Send Button
            } else {
                0
            };

            let prev_hover = HOVER_BTN.swap(hover, Ordering::Relaxed);
            if prev_hover != hover {
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
            let is_pending = is_approval_active();
            let pine_header_bg = COLORREF(0x001B4332); // Signature Dark Pine Header
            let pine_mint_fg = COLORREF(0x00B7E4C7);   // Soft emerald mint text

            // Background & Border
            let bg_brush = CreateSolidBrush(PK_CARD);
            FillRect(hdc_mem, &rc, bg_brush);
            let _ = DeleteObject(HGDIOBJ(bg_brush.0));

            let border_pen = CreatePen(PS_SOLID, 1, PK_CARD_BORDER);
            let old_pen = SelectObject(hdc_mem, HGDIOBJ(border_pen.0));
            let old_brush = SelectObject(hdc_mem, HGDIOBJ(GetStockObject(NULL_BRUSH).0));
            let _ = RoundRect(hdc_mem, 0, 0, w, h, 14, 14);
            let _ = SelectObject(hdc_mem, old_brush);
            let _ = SelectObject(hdc_mem, old_pen);
            let _ = DeleteObject(HGDIOBJ(border_pen.0));

            // 1. Header Bar (0..34)
            let header_rc = RECT { left: 0, top: 0, right: w, bottom: 34 };
            let h_brush = CreateSolidBrush(pine_header_bg);
            FillRect(hdc_mem, &header_rc, h_brush);
            let _ = DeleteObject(HGDIOBJ(h_brush.0));

            SetBkMode(hdc_mem, TRANSPARENT);
            let font_title = create_segoe_font(11, FW_BOLD);
            let old_font = SelectObject(hdc_mem, HGDIOBJ(font_title.0));
            SetTextColor(hdc_mem, COLORREF(0x00FFFFFF));

            let mut title_txt: Vec<u16> = "DESKY".encode_utf16().collect();
            let mut title_rc = RECT { left: 14, top: 0, right: 62, bottom: 34 };
            DrawTextW(hdc_mem, &mut title_txt, &mut title_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            let font_sub = create_segoe_font(11, FW_NORMAL);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_sub.0));
            SetTextColor(hdc_mem, pine_mint_fg);
            let sub_title = if is_pending { " | Connection Request" } else { " | Active Session" };
            let mut sub_txt: Vec<u16> = sub_title.encode_utf16().collect();
            let mut sub_rc = RECT { left: 62, top: 0, right: 310, bottom: 34 };
            DrawTextW(hdc_mem, &mut sub_txt, &mut sub_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            // Minimize Button (—)
            let m_bg = if hover == 10 { COLORREF(0x00325C47) } else { pine_header_bg };
            draw_rounded_box(hdc_mem, 318, 6, 24, 22, m_bg, COLORREF(0x004F6A2D), 4);
            let m_pen = CreatePen(PS_SOLID, 2, COLORREF(0x00FFFFFF));
            let old_mp = SelectObject(hdc_mem, HGDIOBJ(m_pen.0));
            let _ = windows::Win32::Graphics::Gdi::MoveToEx(hdc_mem, 323, 17, None);
            let _ = windows::Win32::Graphics::Gdi::LineTo(hdc_mem, 337, 17);
            let _ = SelectObject(hdc_mem, old_mp);
            let _ = DeleteObject(HGDIOBJ(m_pen.0));

            // Header Close Button (✕)
            let c_bg = if hover == 11 { PK_RED } else { pine_header_bg };
            draw_rounded_box(hdc_mem, 346, 6, 24, 22, c_bg, if hover == 11 { PK_RED } else { COLORREF(0x004F6A2D) }, 4);
            crate::ui::draw_x(hdc_mem, 352, 11, 12, COLORREF(0x00FFFFFF), 2);

            // Divider below header
            let div_pen = CreatePen(PS_SOLID, 1, PK_CARD_BORDER);
            let old_dp = SelectObject(hdc_mem, HGDIOBJ(div_pen.0));
            let _ = windows::Win32::Graphics::Gdi::MoveToEx(hdc_mem, 0, 34, None);
            let _ = windows::Win32::Graphics::Gdi::LineTo(hdc_mem, w, 34);
            let _ = SelectObject(hdc_mem, old_dp);
            let _ = DeleteObject(HGDIOBJ(div_pen.0));

            // 2. Identity & Avatar Area (36..130)
            let session_info = crate::p2p::get_active_host_session_info();
            let pending_info = crate::p2p::get_pending_approval();

            let (raw_id, elapsed_secs) = if let Some((id, secs)) = session_info {
                (id, secs)
            } else if let Some(p) = pending_info {
                (p.client_id, 0)
            } else {
                ("Remote Peer".to_string(), 0)
            };
            let client_id = crate::p2p::state::format_id_display(&raw_id);

            // Circular Avatar at (170..210, 42..82)
            let dot_pen = CreatePen(PS_SOLID, 1, PK_GREEN);
            let dot_brush = CreateSolidBrush(PK_GREEN_TINT);
            let old_p = SelectObject(hdc_mem, HGDIOBJ(dot_pen.0));
            let old_b = SelectObject(hdc_mem, HGDIOBJ(dot_brush.0));
            let _ = windows::Win32::Graphics::Gdi::Ellipse(hdc_mem, 170, 42, 210, 82);
            let _ = SelectObject(hdc_mem, old_p);
            let _ = SelectObject(hdc_mem, old_b);
            let _ = DeleteObject(HGDIOBJ(dot_pen.0));
            let _ = DeleteObject(HGDIOBJ(dot_brush.0));
            crate::ui::draw_monitor(hdc_mem, 179, 51, 22, PK_GREEN_DARK, 2);

            // Remote ID in bold
            let font_id = create_segoe_font(16, FW_BOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_id.0));
            SetTextColor(hdc_mem, PK_TEXT);
            let mut id_txt: Vec<u16> = client_id.encode_utf16().collect();
            let mut id_rc = RECT { left: 14, top: 88, right: w - 14, bottom: 110 };
            DrawTextW(hdc_mem, &mut id_txt, &mut id_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

            // Status prompt line
            let font_status = create_segoe_font(10, FW_SEMIBOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_status.0));
            if is_pending {
                SetTextColor(hdc_mem, PK_MUTED);
                let mut p_txt: Vec<u16> = "would like to view and control your desktop".encode_utf16().collect();
                let mut p_rc = RECT { left: 14, top: 110, right: w - 14, bottom: 130 };
                DrawTextW(hdc_mem, &mut p_txt, &mut p_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
            } else {
                let status_msg = format!("● Session Active • ⏱ {}", format_duration(elapsed_secs));
                SetTextColor(hdc_mem, PK_GREEN_DARK);
                let mut p_txt: Vec<u16> = status_msg.encode_utf16().collect();
                let mut p_rc = RECT { left: 14, top: 110, right: w - 14, bottom: 130 };
                DrawTextW(hdc_mem, &mut p_txt, &mut p_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
            }

            // 3. Permissions Strip (134..172)
            draw_rounded_box(hdc_mem, 14, 134, 352, 38, PK_INPUT_BG, PK_CARD_BORDER, 6);
            let pill_font = create_segoe_font(10, FW_SEMIBOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(pill_font.0));

            // 3.1 Screen (Always Active)
            draw_rounded_box(hdc_mem, 18, 139, 80, 28, PK_GREEN_TINT, PK_GREEN, 5);
            crate::ui::draw_monitor(hdc_mem, 23, 146, 14, PK_GREEN_DARK, 2);
            SetTextColor(hdc_mem, PK_GREEN_DARK);
            let mut p1: Vec<u16> = "Screen".encode_utf16().collect();
            let mut p1_rc = RECT { left: 39, top: 139, right: 96, bottom: 167 };
            DrawTextW(hdc_mem, &mut p1, &mut p1_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

            // 3.2 Mouse & Keyboard Control (Input Sovereignty Toggle)
            let is_input_allowed = crate::p2p::is_host_remote_input_allowed();
            let (in_bg, in_border, in_fg, in_label) = if is_input_allowed {
                (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK, "Control")
            } else {
                (COLORREF(0x00E0E0FF), PK_RED, PK_RED, "View-Only")
            };
            draw_rounded_box(hdc_mem, 102, 139, 84, 28, in_bg, in_border, 5);
            if is_input_allowed {
                crate::ui::draw_check(hdc_mem, 106, 146, 14, in_fg, 2);
            } else {
                crate::ui::draw_lock(hdc_mem, 106, 146, 14, in_fg, 2);
            }
            SetTextColor(hdc_mem, in_fg);
            let mut p2: Vec<u16> = in_label.encode_utf16().collect();
            let mut p2_rc = RECT { left: 122, top: 139, right: 184, bottom: 167 };
            DrawTextW(hdc_mem, &mut p2, &mut p2_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

            // 3.3 Mic (Toggleable)
            let is_mic = crate::windows::audio::is_host_mic_active();
            let (m_bg, m_border, m_fg, m_label) = if is_mic {
                (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK, "Mic: ON")
            } else if hover == 1 {
                (PK_BTN_HOVER, PK_CARD_BORDER, PK_TEXT, "Mic: OFF")
            } else {
                (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED, "Mic: OFF")
            };
            draw_rounded_box(hdc_mem, 190, 139, 80, 28, m_bg, m_border, 5);
            if is_mic {
                crate::ui::draw_mic(hdc_mem, 194, 146, 14, m_fg, 2);
            } else {
                crate::ui::draw_mic_off(hdc_mem, 194, 146, 14, m_fg, 2);
            }
            SetTextColor(hdc_mem, m_fg);
            let mut p3: Vec<u16> = m_label.encode_utf16().collect();
            let mut p3_rc = RECT { left: 210, top: 139, right: 268, bottom: 167 };
            DrawTextW(hdc_mem, &mut p3, &mut p3_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

            // 3.4 Clipboard (Always active)
            draw_rounded_box(hdc_mem, 274, 139, 88, 28, PK_GREEN_TINT, PK_GREEN, 5);
            crate::ui::draw_copy(hdc_mem, 278, 146, 14, PK_GREEN_DARK, 2);
            SetTextColor(hdc_mem, PK_GREEN_DARK);
            let mut p4: Vec<u16> = "Clipboard".encode_utf16().collect();
            let mut p4_rc = RECT { left: 294, top: 139, right: 360, bottom: 167 };
            DrawTextW(hdc_mem, &mut p4, &mut p4_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

            // 4. Primary Action Area (178..224)
            let font_action = create_segoe_font(12, FW_BOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_action.0));
            SetTextColor(hdc_mem, COLORREF(0x00FFFFFF));

            if is_pending {
                // Two Large Action Buttons: [ Accept ] & [ Dismiss ]
                let acc_x = 14;
                let btn_w = 172;
                let btn_h = 44;
                let btn_y = 178;
                let dis_x = 194;

                // [ Accept ] Button
                let acc_bg = if hover == 20 { PK_GREEN_DARK } else { PK_GREEN };
                draw_rounded_box(hdc_mem, acc_x, btn_y, btn_w, btn_h, acc_bg, acc_bg, 8);
                crate::ui::draw_check(hdc_mem, acc_x + 42, btn_y + 14, 16, COLORREF(0x00FFFFFF), 2);
                let mut acc_txt: Vec<u16> = "Accept".encode_utf16().collect();
                let mut acc_rc = RECT { left: acc_x + 62, top: btn_y, right: acc_x + btn_w, bottom: btn_y + btn_h };
                DrawTextW(hdc_mem, &mut acc_txt, &mut acc_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                // [ Dismiss ] Button
                let dis_bg = if hover == 21 { COLORREF(0x00B83A3A) } else { PK_RED };
                draw_rounded_box(hdc_mem, dis_x, btn_y, btn_w, btn_h, dis_bg, dis_bg, 8);
                crate::ui::draw_x(hdc_mem, dis_x + 42, btn_y + 14, 16, COLORREF(0x00FFFFFF), 2);
                let mut dis_txt: Vec<u16> = "Dismiss".encode_utf16().collect();
                let mut dis_rc = RECT { left: dis_x + 62, top: btn_y, right: dis_x + btn_w, bottom: btn_y + btn_h };
                DrawTextW(hdc_mem, &mut dis_txt, &mut dis_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
            } else {
                // Single Full-Width [ Disconnect Session ] Button
                let disc_x = 14;
                let disc_y = 178;
                let disc_w = 352;
                let disc_h = 42;
                let disc_bg = if hover == 3 { COLORREF(0x00B83A3A) } else { PK_RED };

                draw_rounded_box(hdc_mem, disc_x, disc_y, disc_w, disc_h, disc_bg, disc_bg, 8);
                crate::ui::draw_x(hdc_mem, disc_x + 94, disc_y + 13, 16, COLORREF(0x00FFFFFF), 2);
                let mut disc_txt: Vec<u16> = "Disconnect Session".encode_utf16().collect();
                let mut disc_rc = RECT { left: disc_x + 116, top: disc_y, right: disc_x + disc_w - 60, bottom: disc_y + disc_h };
                DrawTextW(hdc_mem, &mut disc_txt, &mut disc_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
            }

            // 5. In-Session Chat Area (230..516)
            let font_chat_label = create_segoe_font(9, FW_BOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_chat_label.0));
            SetTextColor(hdc_mem, PK_MUTED);
            let mut cl_txt: Vec<u16> = "IN-SESSION CHAT".encode_utf16().collect();
            let mut cl_rc = RECT { left: 16, top: 230, right: w - 16, bottom: 244 };
            DrawTextW(hdc_mem, &mut cl_txt, &mut cl_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
            let _ = DeleteObject(HGDIOBJ(font_chat_label.0));

            // Chat Message Box Background
            let chat_box_x = 14;
            let chat_box_y = 246;
            let chat_box_w = 352;
            let chat_box_h = 222;
            draw_rounded_box(hdc_mem, chat_box_x, chat_box_y, chat_box_w, chat_box_h, PK_INPUT_BG, PK_CARD_BORDER, 6);

            let msgs = crate::p2p::get_chat_messages();
            let start_idx = msgs.len().saturating_sub(5);
            let mut line_y = chat_box_y + 8;

            let font_msg = create_segoe_font(10, FW_NORMAL);
            let font_msg_bold = create_segoe_font(9, FW_BOLD);

            if msgs.is_empty() {
                let mut wide_empty: Vec<u16> = "No messages yet. Send a message to the technician below.".encode_utf16().collect();
                let mut empty_rc = RECT { left: chat_box_x + 12, top: chat_box_y + 85, right: chat_box_x + chat_box_w - 12, bottom: chat_box_y + 115 };
                let _ = SelectObject(hdc_mem, HGDIOBJ(font_msg.0));
                SetTextColor(hdc_mem, PK_MUTED);
                DrawTextW(hdc_mem, &mut wide_empty, &mut empty_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
            } else {
                for m in &msgs[start_idx..] {
                    if m.is_me {
                        // Host speech bubble (aligned right)
                        let bubble_w = 214;
                        let bubble_x = chat_box_x + chat_box_w - bubble_w - 8;
                        draw_rounded_box(hdc_mem, bubble_x, line_y, bubble_w, 34, PK_GREEN_TINT, PK_GREEN, 6);

                        let _ = SelectObject(hdc_mem, HGDIOBJ(font_msg_bold.0));
                        SetTextColor(hdc_mem, PK_GREEN_DARK);
                        let header = format!("You • {}", m.timestamp_str);
                        let mut wide_hdr: Vec<u16> = header.encode_utf16().collect();
                        let mut hdr_rc = RECT { left: bubble_x + 8, top: line_y + 3, right: bubble_x + bubble_w - 8, bottom: line_y + 17 };
                        DrawTextW(hdc_mem, &mut wide_hdr, &mut hdr_rc, DT_LEFT | DT_SINGLELINE);

                        let _ = SelectObject(hdc_mem, HGDIOBJ(font_msg.0));
                        SetTextColor(hdc_mem, PK_TEXT);
                        let mut wide_txt: Vec<u16> = m.text.encode_utf16().collect();
                        let mut txt_rc = RECT { left: bubble_x + 8, top: line_y + 17, right: bubble_x + bubble_w - 8, bottom: line_y + 31 };
                        DrawTextW(hdc_mem, &mut wide_txt, &mut txt_rc, DT_LEFT | DT_SINGLELINE | windows::Win32::Graphics::Gdi::DT_END_ELLIPSIS);
                    } else {
                        // Technician speech bubble (aligned left)
                        let bubble_w = 214;
                        let bubble_x = chat_box_x + 8;
                        draw_rounded_box(hdc_mem, bubble_x, line_y, bubble_w, 34, PK_CARD, PK_CARD_BORDER, 6);

                        let _ = SelectObject(hdc_mem, HGDIOBJ(font_msg_bold.0));
                        SetTextColor(hdc_mem, PK_MUTED);
                        let header = format!("Technician • {}", m.timestamp_str);
                        let mut wide_hdr: Vec<u16> = header.encode_utf16().collect();
                        let mut hdr_rc = RECT { left: bubble_x + 8, top: line_y + 3, right: bubble_x + bubble_w - 8, bottom: line_y + 17 };
                        DrawTextW(hdc_mem, &mut wide_hdr, &mut hdr_rc, DT_LEFT | DT_SINGLELINE);

                        let _ = SelectObject(hdc_mem, HGDIOBJ(font_msg.0));
                        SetTextColor(hdc_mem, PK_TEXT);
                        let mut wide_txt: Vec<u16> = m.text.encode_utf16().collect();
                        let mut txt_rc = RECT { left: bubble_x + 8, top: line_y + 17, right: bubble_x + bubble_w - 8, bottom: line_y + 31 };
                        DrawTextW(hdc_mem, &mut wide_txt, &mut txt_rc, DT_LEFT | DT_SINGLELINE | windows::Win32::Graphics::Gdi::DT_END_ELLIPSIS);
                    }
                    line_y += 40;
                }
            }
            let _ = DeleteObject(HGDIOBJ(font_msg.0));
            let _ = DeleteObject(HGDIOBJ(font_msg_bold.0));

            // Send Button (x: 304, y: 476, w: 62, h: 32)
            let send_btn_x = 304;
            let send_btn_y = 476;
            let send_btn_w = 62;
            let send_btn_h = 32;
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

            let _ = SelectObject(hdc_mem, old_font);
            let _ = DeleteObject(HGDIOBJ(font_title.0));
            let _ = DeleteObject(HGDIOBJ(font_sub.0));
            let _ = DeleteObject(HGDIOBJ(font_id.0));
            let _ = DeleteObject(HGDIOBJ(font_status.0));
            let _ = DeleteObject(HGDIOBJ(pill_font.0));
            let _ = DeleteObject(HGDIOBJ(font_action.0));

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
