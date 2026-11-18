//! Host Session Window & Approval Controller (Win32)
//!
//! Provides the host workstation user with a dedicated, two-column session manager:
//! - Exact horizontal layout matching the familiar remote desktop host panel.
//! - Left column: Requester profile card, green identifier, permission grid, and Accept / Dismiss buttons.
//! - Right column: Chat & File Transfer tabs, session activity stream, live speech bubbles, and text input.
//! - Smooth native Win32 window dragging via WM_NCHITTEST with persistent positioning.
//! - Single persistent window lifecycle: upon clicking [Accept], the buttons seamlessly transition
//!   to [Disconnect], while preserving chat history, permission toggles, and duration timer.

#![allow(unsafe_op_in_unsafe_fn)]

use std::sync::atomic::{AtomicI32, AtomicIsize, Ordering};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreatePen, CreateRoundRectRgn,
    CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetStockObject, InvalidateRect,
    Polyline, RoundRect, SelectObject, SetBkColor, SetBkMode, SetTextColor, SetWindowRgn, UpdateWindow,
    DT_CENTER, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL, FW_SEMIBOLD, HDC, HGDIOBJ,
    NULL_BRUSH, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos, GetParent, GetSystemMetrics,
    GetWindowRect, GetWindowTextLengthW, GetWindowTextW, IsWindow, LoadCursorW,
    PostMessageW, RegisterClassExW, SendMessageW, SetCursor, SetForegroundWindow,
    SetTimer, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow, GWLP_WNDPROC, HMENU,
    HWND_TOPMOST, IDC_ARROW, IDC_HAND, SM_CXSCREEN, SM_CYSCREEN,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_MINIMIZE, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_CHAR, WM_COMMAND, WM_CREATE, WM_CTLCOLOREDIT, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_MOUSEMOVE, WM_NCHITTEST, WM_PAINT, WM_SETCURSOR, WM_TIMER, WNDCLASSEXW, WS_CHILD,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_TABSTOP, WS_VISIBLE,
};

use super::theme::{
    create_segoe_font, draw_rounded_box, PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT, PK_RED, PK_TEXT,
};

pub const HOST_WINDOW_W: i32 = 720;
pub const HOST_WINDOW_H: i32 = 440;

const ID_EDIT_HOST_CHAT: usize = 9981;
const ID_TIMER_WIDGET_TICK: usize = 4001;

static HWND_HOST_WIDGET: AtomicIsize = AtomicIsize::new(0);
static HWND_CHAT_EDIT: AtomicIsize = AtomicIsize::new(0);
static PREV_HOST_CHAT_WNDPROC: AtomicIsize = AtomicIsize::new(0);
static HOVER_BTN: AtomicI32 = AtomicI32::new(0);
// Hover IDs:
// 1 = Mic, 3 = Disconnect / End, 4 = Send, 5 = Input Sovereignty
// 10 = Minimize, 11 = Header Close
// 20 = Approval Accept, 21 = Approval Dismiss

pub fn is_approval_active() -> bool {
    crate::p2p::get_pending_approval().is_some() && crate::p2p::get_active_host_session_info().is_none()
}

unsafe fn apply_widget_shape(hwnd: HWND, w: i32, h: i32) {
    let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, 10, 10);
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

    let existing = HWND_HOST_WIDGET.load(Ordering::Relaxed);
    if existing != 0 {
        let hwnd = HWND(existing as *mut _);
        if unsafe { IsWindow(Some(hwnd)).as_bool() } {
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOW);
                // Keep the window wherever the user moved it (SWP_NOMOVE)
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                );
                let _ = InvalidateRect(Some(hwnd), None, false);
                let _ = UpdateWindow(hwnd);
            }
            return;
        }
    }

    // Initial creation: center on primary screen
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let x = (screen_w - HOST_WINDOW_W) / 2;
    let y = (screen_h - HOST_WINDOW_H) / 2;

    unsafe {
        let h_module = GetModuleHandleW(None).unwrap_or_default();
        let h_instance: HINSTANCE = h_module.into();
        let class_name = w!("DeskyHostSessionWindowClass");

        let mut wc = WNDCLASSEXW::default();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(host_widget_wndproc);
        wc.hInstance = h_instance;
        wc.lpszClassName = class_name;
        wc.hbrBackground = CreateSolidBrush(COLORREF(0x00FFFFFF));
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
        WM_NCHITTEST => {
            let pt_x = (lparam.0 & 0xffff) as i16 as i32;
            let pt_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let mut rc = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rc);
            let client_x = pt_x - rc.left;
            let client_y = pt_y - rc.top;

            // Top titlebar (y <= 36): rightmost buttons (minimize, close) receive clicks
            if client_y >= 0 && client_y <= 36 {
                if client_x >= rc.right - rc.left - 90 {
                    return LRESULT(1); // HTCLIENT
                }
                return LRESULT(2); // HTCAPTION -> Smooth native Win32 window movement
            }
            LRESULT(1) // HTCLIENT for rest of body
        }
        WM_CREATE => {
            let h_module = GetModuleHandleW(None).unwrap_or_default();
            let h_instance: HINSTANCE = h_module.into();

            // Right-column chat edit control positioned in the bottom input bar
            let edit_hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("EDIT"),
                w!(""),
                WINDOW_STYLE(
                    WS_CHILD.0 | WS_TABSTOP.0 | 0x0080 | WS_VISIBLE.0, // ES_AUTOHSCROLL | WS_VISIBLE
                ),
                304,
                398,
                362,
                28,
                Some(hwnd),
                Some(HMENU(ID_EDIT_HOST_CHAT as *mut _)),
                Some(h_instance),
                None,
            );

            if let Ok(eh) = edit_hwnd {
                let font = create_segoe_font(11, FW_NORMAL);
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
            SetBkColor(hdc, COLORREF(0x00FFFFFF));
            SetTextColor(hdc, COLORREF(0x001E293B));
            let brush = CreateSolidBrush(COLORREF(0x00FFFFFF));
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

            // 1. Title bar buttons (y <= 36)
            if client_y <= 36 {
                if client_x >= 630 && client_x <= 660 {
                    // Minimize
                    let _ = ShowWindow(hwnd, SW_MINIMIZE);
                    return LRESULT(0);
                } else if client_x >= 686 && client_x <= 716 {
                    // Close / Dismiss / End Session
                    if is_pending {
                        crate::p2p::respond_to_pending_approval(false);
                    } else {
                        crate::p2p::disconnect_host_session();
                    }
                    close_host_widget();
                    return LRESULT(0);
                }
            }

            // 2. Interactive Permission Toggles (Left Column, Row 1 y: 196..238)
            if client_y >= 196 && client_y <= 238 {
                // Button 1: Keyboard & Mouse Control (x: 20..68)
                if client_x >= 20 && client_x <= 68 {
                    let allowed = crate::p2p::toggle_host_remote_input();
                    tracing::info!("Host remote input sovereignty toggled: allowed={}", allowed);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                // Button 4: Audio / Mic (x: 176..224)
                if client_x >= 176 && client_x <= 224 {
                    let is_on = crate::windows::audio::toggle_host_mic();
                    tracing::info!("Host microphone toggled: {}", is_on);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
            }

            // 3. Primary Action Buttons (Left Column, y: 374..422)
            if client_y >= 374 && client_y <= 422 {
                if is_pending {
                    // [ Accept ] Button (x: 16..144)
                    if client_x >= 16 && client_x <= 144 {
                        crate::p2p::respond_to_pending_approval(true);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        let edit_raw = HWND_CHAT_EDIT.load(Ordering::Relaxed);
                        if edit_raw != 0 {
                            let _ = SetFocus(Some(HWND(edit_raw as *mut _)));
                        }
                        return LRESULT(0);
                    }
                    // [ Dismiss ] Button (x: 148..276)
                    if client_x >= 148 && client_x <= 276 {
                        crate::p2p::respond_to_pending_approval(false);
                        close_host_widget();
                        return LRESULT(0);
                    }
                } else {
                    // [ Disconnect ] Button (x: 16..276)
                    if client_x >= 16 && client_x <= 276 {
                        crate::p2p::disconnect_host_session();
                        close_host_widget();
                        return LRESULT(0);
                    }
                }
            }

            // 4. Send Message Button (Right Column, x: 674..712, y: 396..430)
            if client_x >= 674 && client_x <= 712 && client_y >= 396 && client_y <= 430 {
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

            let hover = if client_y <= 36 {
                if client_x >= 630 && client_x <= 660 {
                    10 // Minimize
                } else if client_x >= 686 && client_x <= 716 {
                    11 // Close
                } else {
                    0
                }
            } else if client_y >= 196 && client_y <= 238 {
                if client_x >= 20 && client_x <= 68 {
                    5 // Input Control toggle
                } else if client_x >= 176 && client_x <= 224 {
                    1 // Mic toggle
                } else {
                    0
                }
            } else if client_y >= 374 && client_y <= 422 {
                if is_pending {
                    if client_x >= 16 && client_x <= 144 {
                        20 // Accept
                    } else if client_x >= 148 && client_x <= 276 {
                        21 // Dismiss
                    } else {
                        0
                    }
                } else if client_x >= 16 && client_x <= 276 {
                    3 // Disconnect
                } else {
                    0
                }
            } else if client_x >= 674 && client_x <= 712 && client_y >= 396 && client_y <= 430 {
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

            // Colors
            let clr_bg = COLORREF(0x00FFFFFF);             // Clean White
            let clr_bar_bg = COLORREF(0x00D9DDE2);         // Soft Neutral Header
            let clr_tab_active = COLORREF(0x00FFFFFF);     // White active tab
            let clr_border = COLORREF(0x00E2E8F0);         // Crisp separator line
            let clr_btn_blue = COLORREF(0x00C6864F);       // Slate Blue for permissions (#4F86C6)
            let clr_btn_blue_hover = COLORREF(0x00D69A65); // Lighter blue hover
            let clr_chat_tab = COLORREF(0x00B07B4A);       // Active Chat tab (#4A7BB0)
            let clr_file_tab = COLORREF(0x00BCC5CF);       // Inactive File tab (#CFC5BC)
            let clr_green_peer = COLORREF(0x0016A34A);     // Pure Emerald Green Peer Name
            let clr_avatar_red = COLORREF(0x002424D9);     // Signature Scarlet Red Avatar Box (#D92424)
            let clr_accept_green = COLORREF(0x0045A728);   // Signature Accept Green (#28A745)
            let clr_dismiss_red = COLORREF(0x002626DC);    // Signature Dismiss Red (#DC2626)

            // Base Background & Outer Border
            let bg_brush = CreateSolidBrush(clr_bg);
            FillRect(hdc_mem, &rc, bg_brush);
            let _ = DeleteObject(HGDIOBJ(bg_brush.0));

            let border_pen = CreatePen(PS_SOLID, 1, clr_border);
            let old_pen = SelectObject(hdc_mem, HGDIOBJ(border_pen.0));
            let old_brush = SelectObject(hdc_mem, HGDIOBJ(GetStockObject(NULL_BRUSH).0));
            let _ = RoundRect(hdc_mem, 0, 0, w, h, 10, 10);
            let _ = SelectObject(hdc_mem, old_brush);
            let _ = SelectObject(hdc_mem, old_pen);
            let _ = DeleteObject(HGDIOBJ(border_pen.0));

            // ==============================================================
            // 1. TOP TITLE / TAB BAR (y: 0..36)
            // ==============================================================
            let titlebar_rc = RECT { left: 0, top: 0, right: w, bottom: 36 };
            let tb_brush = CreateSolidBrush(clr_bar_bg);
            FillRect(hdc_mem, &titlebar_rc, tb_brush);
            let _ = DeleteObject(HGDIOBJ(tb_brush.0));

            SetBkMode(hdc_mem, TRANSPARENT);

            // Tab 1: Logo + "Desky" (0..106)
            let font_tab = create_segoe_font(11, FW_SEMIBOLD);
            let old_font = SelectObject(hdc_mem, HGDIOBJ(font_tab.0));
            crate::ui::draw_zap(hdc_mem, 10, 10, 16, clr_avatar_red, 2);
            SetTextColor(hdc_mem, COLORREF(0x001E293B));
            let mut brand_txt: Vec<u16> = "Desky".encode_utf16().collect();
            let mut brand_rc = RECT { left: 32, top: 0, right: 90, bottom: 36 };
            DrawTextW(hdc_mem, &mut brand_txt, &mut brand_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            // Tab 2: Active Peer Tab (x: 106..260)
            let tab2_rc = RECT { left: 106, top: 4, right: 260, bottom: 36 };
            let t2_brush = CreateSolidBrush(clr_tab_active);
            FillRect(hdc_mem, &tab2_rc, t2_brush);
            let _ = DeleteObject(HGDIOBJ(t2_brush.0));

            crate::ui::draw_zap(hdc_mem, 114, 11, 14, clr_avatar_red, 2);

            let session_info = crate::p2p::get_active_host_session_info();
            let pending_info = crate::p2p::get_pending_approval();
            let (raw_id, elapsed_secs) = if let Some((id, secs)) = session_info {
                (id, secs)
            } else if let Some(p) = pending_info {
                (p.client_id, 0)
            } else {
                ("Remote Peer".to_string(), 0)
            };
            let disp_id = crate::p2p::state::format_id_display(&raw_id);

            SetTextColor(hdc_mem, COLORREF(0x001E293B));
            let mut tab2_txt: Vec<u16> = disp_id.encode_utf16().collect();
            let mut tab2_rc_txt = RECT { left: 134, top: 4, right: 236, bottom: 36 };
            DrawTextW(hdc_mem, &mut tab2_txt, &mut tab2_rc_txt, DT_LEFT | DT_SINGLELINE | DT_VCENTER | windows::Win32::Graphics::Gdi::DT_END_ELLIPSIS);
            crate::ui::draw_x(hdc_mem, 242, 14, 10, COLORREF(0x0064748B), 2);

            // Title bar Window Controls (Right: Minimize & Close)
            // Minimize (—)
            let m_pen = CreatePen(PS_SOLID, 2, if hover == 10 { COLORREF(0x000F172A) } else { COLORREF(0x00475569) });
            let old_mp = SelectObject(hdc_mem, HGDIOBJ(m_pen.0));
            let _ = windows::Win32::Graphics::Gdi::MoveToEx(hdc_mem, 638, 20, None);
            let _ = windows::Win32::Graphics::Gdi::LineTo(hdc_mem, 652, 20);
            let _ = SelectObject(hdc_mem, old_mp);
            let _ = DeleteObject(HGDIOBJ(m_pen.0));

            // Maximize Square (□)
            let sq_pen = CreatePen(PS_SOLID, 1, COLORREF(0x00475569));
            let old_sp = SelectObject(hdc_mem, HGDIOBJ(sq_pen.0));
            let _ = RoundRect(hdc_mem, 664, 13, 676, 25, 2, 2);
            let _ = SelectObject(hdc_mem, old_sp);
            let _ = DeleteObject(HGDIOBJ(sq_pen.0));

            // Close (✕)
            if hover == 11 {
                let c_rc = RECT { left: 686, top: 0, right: 720, bottom: 36 };
                let c_brush = CreateSolidBrush(PK_RED);
                FillRect(hdc_mem, &c_rc, c_brush);
                let _ = DeleteObject(HGDIOBJ(c_brush.0));
                crate::ui::draw_x(hdc_mem, 698, 12, 12, COLORREF(0x00FFFFFF), 2);
            } else {
                crate::ui::draw_x(hdc_mem, 698, 12, 12, COLORREF(0x00475569), 2);
            }

            // Divider separating Left & Right Columns (x: 290)
            let col_pen = CreatePen(PS_SOLID, 1, clr_border);
            let old_cp = SelectObject(hdc_mem, HGDIOBJ(col_pen.0));
            let _ = windows::Win32::Graphics::Gdi::MoveToEx(hdc_mem, 290, 36, None);
            let _ = windows::Win32::Graphics::Gdi::LineTo(hdc_mem, 290, h);
            let _ = SelectObject(hdc_mem, old_cp);
            let _ = DeleteObject(HGDIOBJ(col_pen.0));

            // ==============================================================
            // 2. LEFT COLUMN (Width: 290px, x: 0..290)
            // ==============================================================

            // 2.1 Square Avatar Box (x: 20, y: 52, w: 86, h: 86)
            draw_rounded_box(hdc_mem, 20, 52, 86, 86, clr_avatar_red, clr_avatar_red, 4);

            // White Avatar Silhouette (Head circle + Chevron body)
            let white_pen = CreatePen(PS_SOLID, 1, COLORREF(0x00FFFFFF));
            let white_brush = CreateSolidBrush(COLORREF(0x00FFFFFF));
            let old_wp = SelectObject(hdc_mem, HGDIOBJ(white_pen.0));
            let old_wb = SelectObject(hdc_mem, HGDIOBJ(white_brush.0));
            let _ = windows::Win32::Graphics::Gdi::Ellipse(hdc_mem, 52, 64, 74, 86);
            let _ = SelectObject(hdc_mem, old_wp);
            let _ = SelectObject(hdc_mem, old_wb);
            let _ = DeleteObject(HGDIOBJ(white_pen.0));
            let _ = DeleteObject(HGDIOBJ(white_brush.0));

            // Chevron V-shape body
            let thick_pen = CreatePen(PS_SOLID, 5, COLORREF(0x00FFFFFF));
            let old_tp = SelectObject(hdc_mem, HGDIOBJ(thick_pen.0));
            let chev_pts = [
                POINT { x: 42, y: 94 },
                POINT { x: 63, y: 115 },
                POINT { x: 84, y: 94 },
            ];
            let _ = Polyline(hdc_mem, &chev_pts);
            let _ = SelectObject(hdc_mem, old_tp);
            let _ = DeleteObject(HGDIOBJ(thick_pen.0));

            // 2.2 Peer Identification Text
            let font_peer_name = create_segoe_font(15, FW_BOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_peer_name.0));
            SetTextColor(hdc_mem, clr_green_peer);
            let mut pn_txt: Vec<u16> = disp_id.encode_utf16().collect();
            let mut pn_rc = RECT { left: 118, top: 52, right: 284, bottom: 74 };
            DrawTextW(hdc_mem, &mut pn_txt, &mut pn_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            let font_peer_id = create_segoe_font(10, FW_SEMIBOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_peer_id.0));
            SetTextColor(hdc_mem, clr_green_peer);
            let id_formatted = format!("({})", raw_id);
            let mut pid_txt: Vec<u16> = id_formatted.encode_utf16().collect();
            let mut pid_rc = RECT { left: 118, top: 74, right: 284, bottom: 94 };
            DrawTextW(hdc_mem, &mut pid_txt, &mut pid_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            let font_subtitle = create_segoe_font(10, FW_NORMAL);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_subtitle.0));
            if is_pending {
                SetTextColor(hdc_mem, COLORREF(0x00475569));
                let mut sub1_txt: Vec<u16> = "would like to view your".encode_utf16().collect();
                let mut sub1_rc = RECT { left: 118, top: 96, right: 284, bottom: 114 };
                DrawTextW(hdc_mem, &mut sub1_txt, &mut sub1_rc, DT_LEFT | DT_SINGLELINE);

                let mut sub2_txt: Vec<u16> = "desk.".encode_utf16().collect();
                let mut sub2_rc = RECT { left: 118, top: 114, right: 284, bottom: 132 };
                DrawTextW(hdc_mem, &mut sub2_txt, &mut sub2_rc, DT_LEFT | DT_SINGLELINE);
            } else {
                SetTextColor(hdc_mem, clr_green_peer);
                let duration_str = format!("Session Active\n⏱ {}", format_duration(elapsed_secs));
                let mut dur_txt: Vec<u16> = duration_str.encode_utf16().collect();
                let mut dur_rc = RECT { left: 118, top: 96, right: 284, bottom: 134 };
                DrawTextW(hdc_mem, &mut dur_txt, &mut dur_rc, DT_LEFT);
            }

            // Security Badges (Below avatar: Green Lock & Green Checkmark Shield)
            crate::ui::draw_lock(hdc_mem, 22, 146, 14, clr_green_peer, 2);
            draw_rounded_box(hdc_mem, 40, 145, 16, 16, clr_green_peer, clr_green_peer, 3);
            crate::ui::draw_check(hdc_mem, 42, 147, 12, COLORREF(0x00FFFFFF), 2);

            // 2.3 "Permissions" Label
            let font_perm = create_segoe_font(11, FW_SEMIBOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_perm.0));
            SetTextColor(hdc_mem, COLORREF(0x0064748B));
            let mut p_lbl: Vec<u16> = "Permissions".encode_utf16().collect();
            let mut p_rc = RECT { left: 20, top: 172, right: 200, bottom: 192 };
            DrawTextW(hdc_mem, &mut p_lbl, &mut p_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            // 2.4 Permissions Matrix (2 rows of Solid Slate Blue buttons)
            let is_input_allowed = crate::p2p::is_host_remote_input_allowed();
            let is_mic_on = crate::windows::audio::is_host_mic_active();

            // Row 1 (y: 196..238)
            // 1. Keyboard & Mouse (Input Sovereignty - Clickable)
            let btn1_bg = if !is_input_allowed {
                clr_dismiss_red
            } else if hover == 5 {
                clr_btn_blue_hover
            } else {
                clr_btn_blue
            };
            draw_rounded_box(hdc_mem, 20, 196, 48, 42, btn1_bg, btn1_bg, 4);
            if is_input_allowed {
                crate::ui::draw_terminal(hdc_mem, 32, 207, 24, COLORREF(0x00FFFFFF), 2);
            } else {
                crate::ui::draw_lock(hdc_mem, 34, 207, 20, COLORREF(0x00FFFFFF), 2);
            }

            // 2. Clipboard
            draw_rounded_box(hdc_mem, 74, 196, 48, 42, clr_btn_blue, clr_btn_blue, 4);
            crate::ui::draw_copy(hdc_mem, 88, 207, 20, COLORREF(0x00FFFFFF), 2);

            // 3. File Transfer
            draw_rounded_box(hdc_mem, 128, 196, 48, 42, clr_btn_blue, clr_btn_blue, 4);
            crate::ui::draw_folder(hdc_mem, 142, 207, 20, COLORREF(0x00FFFFFF), 2);

            // 4. Sound / Mic (Clickable)
            let btn4_bg = if hover == 1 { clr_btn_blue_hover } else { clr_btn_blue };
            draw_rounded_box(hdc_mem, 182, 196, 48, 42, btn4_bg, btn4_bg, 4);
            if is_mic_on {
                crate::ui::draw_volume(hdc_mem, 196, 207, 20, COLORREF(0x00FFFFFF), 2);
            } else {
                crate::ui::draw_volume_x(hdc_mem, 196, 207, 20, COLORREF(0x00FFFFFF), 2);
            }

            // 5. Screen Resolution
            draw_rounded_box(hdc_mem, 236, 196, 48, 42, clr_btn_blue, clr_btn_blue, 4);
            crate::ui::draw_monitor(hdc_mem, 249, 207, 22, COLORREF(0x00FFFFFF), 2);

            // Row 2 (y: 246..288)
            // 6. Lock
            draw_rounded_box(hdc_mem, 20, 246, 48, 42, clr_btn_blue, clr_btn_blue, 4);
            crate::ui::draw_lock(hdc_mem, 34, 257, 20, COLORREF(0x00FFFFFF), 2);

            // 7. Video Recording
            draw_rounded_box(hdc_mem, 74, 246, 48, 42, clr_btn_blue, clr_btn_blue, 4);
            crate::ui::draw_record(hdc_mem, 88, 257, 20, COLORREF(0x00FFFFFF), 2, false);

            // 8. Whiteboard / Pen
            draw_rounded_box(hdc_mem, 128, 246, 48, 42, clr_btn_blue, clr_btn_blue, 4);
            crate::ui::draw_pen(hdc_mem, 142, 257, 20, COLORREF(0x00FFFFFF), 2);

            // 2.5 Bottom Action Area (y: 374..422)
            let font_action = create_segoe_font(13, FW_BOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_action.0));
            SetTextColor(hdc_mem, COLORREF(0x00FFFFFF));

            if is_pending {
                // [ Accept ] Button (x: 16..144, w=128, h=48)
                let acc_bg = if hover == 20 { COLORREF(0x001E8238) } else { clr_accept_green };
                draw_rounded_box(hdc_mem, 16, 374, 128, 48, acc_bg, acc_bg, 4);
                // Outer highlight border matching screenshot
                let acc_border_pen = CreatePen(PS_SOLID, 2, COLORREF(0x002E7D32));
                let old_abp = SelectObject(hdc_mem, HGDIOBJ(acc_border_pen.0));
                let old_abb = SelectObject(hdc_mem, HGDIOBJ(GetStockObject(NULL_BRUSH).0));
                let _ = RoundRect(hdc_mem, 14, 372, 146, 424, 6, 6);
                let _ = SelectObject(hdc_mem, old_abb);
                let _ = SelectObject(hdc_mem, old_abp);
                let _ = DeleteObject(HGDIOBJ(acc_border_pen.0));

                let mut acc_txt: Vec<u16> = "Accept".encode_utf16().collect();
                let mut acc_rc = RECT { left: 16, top: 374, right: 144, bottom: 422 };
                DrawTextW(hdc_mem, &mut acc_txt, &mut acc_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                // [ Dismiss ] Button (x: 152..276, w=124, h=48)
                let dis_bg = if hover == 21 { COLORREF(0x00B81D1D) } else { clr_dismiss_red };
                draw_rounded_box(hdc_mem, 152, 374, 124, 48, dis_bg, dis_bg, 4);
                let mut dis_txt: Vec<u16> = "Dismiss".encode_utf16().collect();
                let mut dis_rc = RECT { left: 152, top: 374, right: 276, bottom: 422 };
                DrawTextW(hdc_mem, &mut dis_txt, &mut dis_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
            } else {
                // Active Session: Single Full-Width [ Disconnect ] Button
                let disc_bg = if hover == 3 { COLORREF(0x00B81D1D) } else { clr_dismiss_red };
                draw_rounded_box(hdc_mem, 16, 374, 260, 48, disc_bg, disc_bg, 4);
                let mut disc_txt: Vec<u16> = "Disconnect".encode_utf16().collect();
                let mut disc_rc = RECT { left: 16, top: 374, right: 276, bottom: 422 };
                DrawTextW(hdc_mem, &mut disc_txt, &mut disc_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
            }

            // ==============================================================
            // 3. RIGHT COLUMN: CHAT & FILE TRANSFER (x: 290..720)
            // ==============================================================

            // 3.1 Tabs (y: 36..70)
            // Chat Tab (Active, width 190)
            let chat_tab_rc = RECT { left: 290, top: 36, right: 480, bottom: 70 };
            let ct_brush = CreateSolidBrush(clr_chat_tab);
            FillRect(hdc_mem, &chat_tab_rc, ct_brush);
            let _ = DeleteObject(HGDIOBJ(ct_brush.0));

            let font_tabs = create_segoe_font(11, FW_BOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_tabs.0));
            SetTextColor(hdc_mem, COLORREF(0x00FFFFFF));
            let mut ct_txt: Vec<u16> = "Chat".encode_utf16().collect();
            let mut ct_rc = RECT { left: 290, top: 36, right: 480, bottom: 70 };
            DrawTextW(hdc_mem, &mut ct_txt, &mut ct_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

            // File Transfer Tab (Inactive, width 240)
            let ft_tab_rc = RECT { left: 480, top: 36, right: w, bottom: 70 };
            let ft_brush = CreateSolidBrush(clr_file_tab);
            FillRect(hdc_mem, &ft_tab_rc, ft_brush);
            let _ = DeleteObject(HGDIOBJ(ft_brush.0));

            let mut ft_txt: Vec<u16> = "File Transfer".encode_utf16().collect();
            let mut ft_rc = RECT { left: 480, top: 36, right: w, bottom: 70 };
            DrawTextW(hdc_mem, &mut ft_txt, &mut ft_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

            // 3.2 "Open Chat Log" Subheader (y: 74..98)
            let font_log = create_segoe_font(10, FW_BOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_log.0));
            SetTextColor(hdc_mem, COLORREF(0x000F172A));
            let mut log_txt: Vec<u16> = "Open Chat Log".encode_utf16().collect();
            let mut log_rc = RECT { left: 302, top: 76, right: 450, bottom: 96 };
            DrawTextW(hdc_mem, &mut log_txt, &mut log_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

            // Underline for Open Chat Log
            let under_pen = CreatePen(PS_SOLID, 1, COLORREF(0x000F172A));
            let old_up = SelectObject(hdc_mem, HGDIOBJ(under_pen.0));
            let _ = windows::Win32::Graphics::Gdi::MoveToEx(hdc_mem, 302, 95, None);
            let _ = windows::Win32::Graphics::Gdi::LineTo(hdc_mem, 396, 95);
            let _ = SelectObject(hdc_mem, old_up);
            let _ = DeleteObject(HGDIOBJ(under_pen.0));

            // Horizontal divider below Open Chat Log (y: 100)
            let sep_pen = CreatePen(PS_SOLID, 1, clr_border);
            let old_sp2 = SelectObject(hdc_mem, HGDIOBJ(sep_pen.0));
            let _ = windows::Win32::Graphics::Gdi::MoveToEx(hdc_mem, 290, 100, None);
            let _ = windows::Win32::Graphics::Gdi::LineTo(hdc_mem, w, 100);
            let _ = SelectObject(hdc_mem, old_sp2);
            let _ = DeleteObject(HGDIOBJ(sep_pen.0));

            // 3.3 Message Stream Area (y: 102..388)
            // Initial Session Event: [Peer Name] ... [Timestamp] \n [Monitor] Session request received
            let font_evt_header = create_segoe_font(10, FW_BOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_evt_header.0));
            SetTextColor(hdc_mem, COLORREF(0x00334155));
            let mut evth_txt: Vec<u16> = disp_id.encode_utf16().collect();
            let mut evth_rc = RECT { left: 304, top: 112, right: 550, bottom: 128 };
            DrawTextW(hdc_mem, &mut evth_txt, &mut evth_rc, DT_LEFT | DT_SINGLELINE);

            let font_time = create_segoe_font(9, FW_NORMAL);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_time.0));
            SetTextColor(hdc_mem, COLORREF(0x0064748B));
            let mut time_txt: Vec<u16> = "10:24".encode_utf16().collect();
            let mut time_rc = RECT { left: w - 70, top: 112, right: w - 16, bottom: 128 };
            DrawTextW(hdc_mem, &mut time_txt, &mut time_rc, DT_RIGHT | DT_SINGLELINE);

            // Monitor Icon + "Session request received"
            crate::ui::draw_monitor(hdc_mem, 304, 132, 16, COLORREF(0x0064748B), 2);
            let font_evt = create_segoe_font(10, FW_NORMAL);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_evt.0));
            SetTextColor(hdc_mem, COLORREF(0x000F172A));
            let mut srr_txt: Vec<u16> = "Session request received".encode_utf16().collect();
            let mut srr_rc = RECT { left: 326, top: 130, right: 600, bottom: 148 };
            DrawTextW(hdc_mem, &mut srr_txt, &mut srr_rc, DT_LEFT | DT_SINGLELINE);

            // Live Chat Messages List
            let msgs = crate::p2p::get_chat_messages();
            let start_idx = msgs.len().saturating_sub(4);
            let mut line_y = 158;

            let font_msg = create_segoe_font(10, FW_NORMAL);
            let font_msg_bold = create_segoe_font(9, FW_BOLD);

            for m in &msgs[start_idx..] {
                if m.is_me {
                    // Host message bubble on right
                    let bubble_w = 260;
                    let bubble_x = w - bubble_w - 18;
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
                    // Remote message bubble on left
                    let bubble_w = 260;
                    let bubble_x = 304;
                    draw_rounded_box(hdc_mem, bubble_x, line_y, bubble_w, 34, COLORREF(0x00F8FAFC), clr_border, 6);

                    let _ = SelectObject(hdc_mem, HGDIOBJ(font_msg_bold.0));
                    SetTextColor(hdc_mem, COLORREF(0x0064748B));
                    let header = format!("{} • {}", disp_id, m.timestamp_str);
                    let mut wide_hdr: Vec<u16> = header.encode_utf16().collect();
                    let mut hdr_rc = RECT { left: bubble_x + 8, top: line_y + 3, right: bubble_x + bubble_w - 8, bottom: line_y + 17 };
                    DrawTextW(hdc_mem, &mut wide_hdr, &mut hdr_rc, DT_LEFT | DT_SINGLELINE);

                    let _ = SelectObject(hdc_mem, HGDIOBJ(font_msg.0));
                    SetTextColor(hdc_mem, COLORREF(0x000F172A));
                    let mut wide_txt: Vec<u16> = m.text.encode_utf16().collect();
                    let mut txt_rc = RECT { left: bubble_x + 8, top: line_y + 17, right: bubble_x + bubble_w - 8, bottom: line_y + 31 };
                    DrawTextW(hdc_mem, &mut wide_txt, &mut txt_rc, DT_LEFT | DT_SINGLELINE | windows::Win32::Graphics::Gdi::DT_END_ELLIPSIS);
                }
                line_y += 38;
            }

            let _ = DeleteObject(HGDIOBJ(font_msg.0));
            let _ = DeleteObject(HGDIOBJ(font_msg_bold.0));
            let _ = DeleteObject(HGDIOBJ(font_evt_header.0));
            let _ = DeleteObject(HGDIOBJ(font_time.0));
            let _ = DeleteObject(HGDIOBJ(font_evt.0));

            // 3.4 Bottom Input Row (y: 390..438)
            // Divider above input row
            let bot_pen = CreatePen(PS_SOLID, 1, clr_border);
            let old_bp = SelectObject(hdc_mem, HGDIOBJ(bot_pen.0));
            let _ = windows::Win32::Graphics::Gdi::MoveToEx(hdc_mem, 290, 390, None);
            let _ = windows::Win32::Graphics::Gdi::LineTo(hdc_mem, w, 390);
            let _ = SelectObject(hdc_mem, old_bp);
            let _ = DeleteObject(HGDIOBJ(bot_pen.0));

            // If edit box is empty, show placeholder "Message"
            let edit_raw = HWND_CHAT_EDIT.load(Ordering::Relaxed);
            let edit_len = if edit_raw != 0 {
                GetWindowTextLengthW(HWND(edit_raw as *mut _))
            } else {
                0
            };
            if edit_len == 0 {
                let font_place = create_segoe_font(11, FW_NORMAL);
                let _ = SelectObject(hdc_mem, HGDIOBJ(font_place.0));
                SetTextColor(hdc_mem, COLORREF(0x0094A3B8));
                let mut p_txt: Vec<u16> = "Message".encode_utf16().collect();
                let mut p_rc = RECT { left: 308, top: 398, right: 450, bottom: 426 };
                DrawTextW(hdc_mem, &mut p_txt, &mut p_rc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
                let _ = DeleteObject(HGDIOBJ(font_place.0));
            }

            // Send Arrow Button at x: 674, y: 396, w: 38, h: 32
            let send_btn_x = 674;
            let send_btn_y = 396;
            let send_btn_w = 38;
            let send_btn_h = 32;
            let s_bg = if hover == 4 { COLORREF(0x00F1F5F9) } else { COLORREF(0x00FFFFFF) };
            draw_rounded_box(hdc_mem, send_btn_x, send_btn_y, send_btn_w, send_btn_h, s_bg, s_bg, 4);

            // Paper airplane / Send arrow icon (pointed right)
            let arrow_color = if hover == 4 { clr_accept_green } else { COLORREF(0x00334155) };
            let arrow_pen = CreatePen(PS_SOLID, 2, arrow_color);
            let arrow_brush = CreateSolidBrush(arrow_color);
            let old_ap2 = SelectObject(hdc_mem, HGDIOBJ(arrow_pen.0));
            let old_ab2 = SelectObject(hdc_mem, HGDIOBJ(arrow_brush.0));
            let arrow_pts = [
                POINT { x: send_btn_x + 8, y: send_btn_y + 8 },
                POINT { x: send_btn_x + 28, y: send_btn_y + 16 },
                POINT { x: send_btn_x + 8, y: send_btn_y + 24 },
                POINT { x: send_btn_x + 14, y: send_btn_y + 16 },
            ];
            let _ = windows::Win32::Graphics::Gdi::Polygon(hdc_mem, &arrow_pts);
            let _ = SelectObject(hdc_mem, old_ab2);
            let _ = SelectObject(hdc_mem, old_ap2);
            let _ = DeleteObject(HGDIOBJ(arrow_pen.0));
            let _ = DeleteObject(HGDIOBJ(arrow_brush.0));

            // Clean up fonts
            let _ = SelectObject(hdc_mem, old_font);
            let _ = DeleteObject(HGDIOBJ(font_tab.0));
            let _ = DeleteObject(HGDIOBJ(font_peer_name.0));
            let _ = DeleteObject(HGDIOBJ(font_peer_id.0));
            let _ = DeleteObject(HGDIOBJ(font_subtitle.0));
            let _ = DeleteObject(HGDIOBJ(font_perm.0));
            let _ = DeleteObject(HGDIOBJ(font_action.0));
            let _ = DeleteObject(HGDIOBJ(font_tabs.0));
            let _ = DeleteObject(HGDIOBJ(font_log.0));

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
