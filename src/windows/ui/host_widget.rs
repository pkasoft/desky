//! Host Session Approval & Live Control Window (Desky Design Language)
//!
//! Provides a fully resizable, two-column layout adhering strictly to
//! pkasoft.com / Desky White Theme specifications:
//! - Off-white canvas (PK_BG) with crisp white cards (PK_CARD) & subtle line borders (PK_CARD_BORDER)
//! - Authentic Desky deep emerald (#143828) & signature green (#2d6a4f) branding and logos
//! - Fully resizable window with native edge-drag resizing, maximize/restore, and smooth titlebar dragging
//! - 4 active, real permission options with iconic Desky green vector icons
//! - Two-column horizontal architecture with identity/permissions on the left and tabbed live chat on the right

#![allow(unsafe_op_in_unsafe_fn)]
#![allow(unused_must_use)]

use std::sync::atomic::{AtomicI32, AtomicIsize, Ordering};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreatePen, CreateRoundRectRgn,
    CreateSolidBrush, DeleteObject, DrawTextW, Ellipse, EndPaint, FillRect, GetStockObject, InvalidateRect,
    LineTo, MoveToEx, RoundRect, SelectObject, SetBkColor, SetBkMode, SetTextColor, SetWindowRgn,
    UpdateWindow, DT_CENTER, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL,
    FW_SEMIBOLD, HDC, HGDIOBJ, NULL_BRUSH, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetParent, GetSystemMetrics,
    GetWindowRect, GetWindowTextLengthW, GetWindowTextW, IsWindow, IsZoomed, LoadCursorW,
    MoveWindow, RegisterClassExW, SendMessageW, SetCursor, SetForegroundWindow,
    SetTimer, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow, GWLP_WNDPROC, HMENU,
    HWND_TOPMOST, IDC_ARROW, IDC_HAND, MINMAXINFO, SM_CXSCREEN, SM_CYSCREEN,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SW_SHOW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_CHAR, WM_COMMAND, WM_CREATE, WM_CTLCOLOREDIT, WM_DESTROY,
    WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_NCHITTEST, WM_NCLBUTTONDBLCLK,
    WM_PAINT, WM_SETCURSOR, WM_SIZE, WM_TIMER, WNDCLASSEXW, WS_CHILD, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_TABSTOP, WS_THICKFRAME, WS_VISIBLE,
};

use super::theme::{
    create_segoe_font, draw_rounded_box, PK_BG, PK_BTN_HOVER, PK_CARD, PK_CARD_BORDER,
    PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_RED, PK_TEXT,
};

pub const DEFAULT_HOST_W: i32 = 720;
pub const DEFAULT_HOST_H: i32 = 440;
pub const MIN_HOST_W: i32 = 640;
pub const MIN_HOST_H: i32 = 400;

const ID_EDIT_HOST_CHAT: usize = 9981;
const ID_TIMER_WIDGET_TICK: usize = 4001;

static HWND_HOST_WIDGET: AtomicIsize = AtomicIsize::new(0);
static HWND_CHAT_EDIT: AtomicIsize = AtomicIsize::new(0);
static PREV_HOST_CHAT_WNDPROC: AtomicIsize = AtomicIsize::new(0);
static HOVER_BTN: AtomicI32 = AtomicI32::new(0);
// Hover IDs:
// 10 = Minimize, 11 = Maximize, 12 = Close
// 20 = Input Sovereignty, 21 = Mic, 22 = Clipboard, 23 = File Transfer
// 30 = Accept, 31 = Dismiss / Disconnect, 32 = Send

pub fn is_approval_active() -> bool {
    crate::p2p::get_pending_approval().is_some() && crate::p2p::get_active_host_session_info().is_none()
}

unsafe fn apply_widget_shape(hwnd: HWND, w: i32, h: i32) {
    if IsZoomed(hwnd).as_bool() {
        let _ = SetWindowRgn(hwnd, None, true);
        return;
    }
    let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, 12, 12);
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
                return LRESULT(0); // Suppress Windows bell sound on Enter
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
    let is_pending = is_approval_active();
    let is_connected = crate::p2p::get_active_host_session_info().is_some();

    if is_pending || is_connected {
        spawn_host_widget();
    } else {
        close_host_widget();
    }
}

pub fn close_host_widget() {
    let hwnd_raw = HWND_HOST_WIDGET.swap(0, Ordering::SeqCst);
    if hwnd_raw != 0 {
        unsafe {
            let hwnd = HWND(hwnd_raw as *mut _);
            if IsWindow(Some(hwnd)).as_bool() {
                let _ = DestroyWindow(hwnd);
            }
        }
    }
    HWND_CHAT_EDIT.store(0, Ordering::Relaxed);
    PREV_HOST_CHAT_WNDPROC.store(0, Ordering::Relaxed);
}

pub fn spawn_host_widget() {
    let hwnd_raw = HWND_HOST_WIDGET.load(Ordering::SeqCst);
    if hwnd_raw != 0 {
        unsafe {
            let hwnd = HWND(hwnd_raw as *mut _);
            if IsWindow(Some(hwnd)).as_bool() {
                // Keep the exact user-dragged position and size across ticks
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
                return;
            }
        }
    }

    unsafe {
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let class_name = w!("DeskyHostSessionWindowClass");

        let mut wc = WNDCLASSEXW::default();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(host_widget_wndproc);
        wc.hInstance = HINSTANCE(instance.0);
        wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        wc.lpszClassName = class_name;
        let _ = RegisterClassExW(&wc);

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let x = (screen_w - DEFAULT_HOST_W) / 2;
        let y = (screen_h - DEFAULT_HOST_H) / 2;

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_TOPMOST.0 | WS_EX_TOOLWINDOW.0),
            class_name,
            w!("Desky | Session Management"),
            WINDOW_STYLE(WS_POPUP.0 | WS_THICKFRAME.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0 | WS_VISIBLE.0),
            x,
            y,
            DEFAULT_HOST_W,
            DEFAULT_HOST_H,
            None,
            None,
            Some(instance.into()),
            None,
        );

        if hwnd.is_err() || hwnd.as_ref().unwrap().0.is_null() {
            tracing::error!("Failed to create Desky Host Session window");
            return;
        }

        let hwnd = hwnd.unwrap();
        HWND_HOST_WIDGET.store(hwnd.0 as isize, Ordering::SeqCst);

        apply_widget_shape(hwnd, DEFAULT_HOST_W, DEFAULT_HOST_H);

        let _ = SetTimer(Some(hwnd), ID_TIMER_WIDGET_TICK, 1000, None);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = InvalidateRect(Some(hwnd), None, true);
        let _ = UpdateWindow(hwnd);
    }
}

pub fn send_host_chat_input() {
    let edit_raw = HWND_CHAT_EDIT.load(Ordering::Relaxed);
    if edit_raw == 0 {
        return;
    }
    unsafe {
        let hwnd_edit = HWND(edit_raw as *mut _);
        let len = GetWindowTextLengthW(hwnd_edit);
        if len > 0 {
            let mut buf = vec![0u16; (len + 1) as usize];
            GetWindowTextW(hwnd_edit, &mut buf);
            let text = String::from_utf16_lossy(&buf[..len as usize]);
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                crate::p2p::send_chat_message(trimmed);
                let _ = SetWindowTextW(hwnd_edit, w!(""));
            }
        }
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
            let instance = GetModuleHandleW(None).unwrap_or_default();
            let col_left_w = 280;
            let edit_x = col_left_w + 16;
            let edit_y = DEFAULT_HOST_H - 48;
            let edit_w = DEFAULT_HOST_W - edit_x - 56;
            let edit_h = 32;

            let hwnd_edit = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                w!(""),
                WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | 0x0080), // ES_AUTOHSCROLL
                edit_x,
                edit_y,
                edit_w,
                edit_h,
                Some(hwnd),
                Some(HMENU(ID_EDIT_HOST_CHAT as *mut _)),
                Some(instance.into()),
                None,
            );

            if let Ok(edit) = hwnd_edit {
                HWND_CHAT_EDIT.store(edit.0 as isize, Ordering::Relaxed);
                let font = create_segoe_font(12, FW_NORMAL);
                let _ = SendMessageW(edit, 0x0030, Some(WPARAM(font.0 as usize)), Some(LPARAM(1))); // WM_SETFONT

                let prev = SetWindowLongPtrW(
                    edit,
                    GWLP_WNDPROC,
                    host_chat_edit_proc as *const () as usize as isize,
                );
                PREV_HOST_CHAT_WNDPROC.store(prev, Ordering::Relaxed);
            }
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let p = &mut *(lparam.0 as *mut MINMAXINFO);
            p.ptMinTrackSize.x = MIN_HOST_W;
            p.ptMinTrackSize.y = MIN_HOST_H;
            LRESULT(0)
        }
        WM_SIZE => {
            let w = (lparam.0 & 0xffff) as i32;
            let h = ((lparam.0 >> 16) & 0xffff) as i32;
            if w > 0 && h > 0 {
                apply_widget_shape(hwnd, w, h);

                let edit_raw = HWND_CHAT_EDIT.load(Ordering::Relaxed);
                if edit_raw != 0 {
                    let col_left_w = 280;
                    let input_x = col_left_w + 16;
                    let input_y = h - 48;
                    let input_w = (w - input_x - 56).max(100);
                    let input_h = 32;
                    let _ = MoveWindow(HWND(edit_raw as *mut _), input_x, input_y, input_w, input_h, true);
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_NCHITTEST => {
            let screen_x = (lparam.0 & 0xffff) as i16 as i32;
            let screen_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let mut rc = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rc);
            let border = 8;

            let is_zoomed = IsZoomed(hwnd).as_bool();
            if !is_zoomed {
                // Resize borders & corners
                let is_left = screen_x >= rc.left && screen_x < rc.left + border;
                let is_right = screen_x <= rc.right && screen_x > rc.right - border;
                let is_top = screen_y >= rc.top && screen_y < rc.top + border;
                let is_bottom = screen_y <= rc.bottom && screen_y > rc.bottom - border;

                if is_top && is_left { return LRESULT(13); } // HTTOPLEFT
                if is_top && is_right { return LRESULT(14); } // HTTOPRIGHT
                if is_bottom && is_left { return LRESULT(16); } // HTBOTTOMLEFT
                if is_bottom && is_right { return LRESULT(17); } // HTBOTTOMRIGHT
                if is_left { return LRESULT(10); } // HTLEFT
                if is_right { return LRESULT(11); } // HTRIGHT
                if is_top { return LRESULT(12); } // HTTOP
                if is_bottom { return LRESULT(15); } // HTBOTTOM
            }

            let client_y = screen_y - rc.top;
            let client_x = screen_x - rc.left;
            let win_w = rc.right - rc.left;

            if client_y <= 36 {
                // Keep client clicks for Window Controls: Minimize, Maximize, Close
                if client_x >= win_w - 110 {
                    return LRESULT(1); // HTCLIENT
                }
                // Everywhere else in the titlebar is draggable
                return LRESULT(2); // HTCAPTION
            }

            LRESULT(1) // HTCLIENT
        }
        WM_NCLBUTTONDBLCLK => {
            if wparam.0 == 2 { // HTCAPTION
                if IsZoomed(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                } else {
                    let _ = ShowWindow(hwnd, SW_MAXIMIZE);
                }
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CTLCOLOREDIT => {
            let hdc = HDC(wparam.0 as *mut _);
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, PK_TEXT);
            SetBkColor(hdc, PK_INPUT_BG);
            let brush = CreateSolidBrush(PK_INPUT_BG);
            LRESULT(brush.0 as isize)
        }
        WM_TIMER => {
            if wparam.0 == ID_TIMER_WIDGET_TICK {
                let is_pending = is_approval_active();
                let is_connected = crate::p2p::get_active_host_session_info().is_some();
                if !is_pending && !is_connected {
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

            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let w = rc.right - rc.left;
            let h = rc.bottom - rc.top;

            // 1. Top Window Controls (Title Bar y: 0..36)
            if client_y <= 36 {
                if client_x >= w - 36 {
                    // Close
                    close_host_widget();
                    return LRESULT(0);
                } else if client_x >= w - 72 {
                    // Maximize / Restore
                    if IsZoomed(hwnd).as_bool() {
                        let _ = ShowWindow(hwnd, SW_RESTORE);
                    } else {
                        let _ = ShowWindow(hwnd, SW_MAXIMIZE);
                    }
                    return LRESULT(0);
                } else if client_x >= w - 108 {
                    // Minimize
                    let _ = ShowWindow(hwnd, SW_MINIMIZE);
                    return LRESULT(0);
                }
            }

            // 2. Interactive Permission Toggles (Left Column y: 196..242)
            // 4 real options:
            // Btn 0 (20..76): Remote Input Sovereignty (Mouse & Keyboard)
            // Btn 1 (84..140): Audio / Voice Intercom
            // Btn 2 (148..204): Clipboard Sync
            // Btn 3 (212..268): File Transfer
            if client_y >= 196 && client_y <= 242 {
                if client_x >= 20 && client_x <= 76 {
                    let allowed = crate::p2p::toggle_host_remote_input();
                    tracing::info!("Host remote input sovereignty toggled: allowed={}", allowed);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                if client_x >= 84 && client_x <= 140 {
                    let is_on = crate::windows::audio::toggle_host_mic();
                    tracing::info!("Host microphone toggled: {}", is_on);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                if client_x >= 148 && client_x <= 204 {
                    let is_on = crate::p2p::toggle_host_clipboard();
                    tracing::info!("Host clipboard sync toggled: {}", is_on);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                if client_x >= 212 && client_x <= 268 {
                    let is_on = crate::p2p::toggle_host_file_transfer();
                    tracing::info!("Host file transfer toggled: {}", is_on);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
            }

            // 3. Primary Action Buttons (Left Column, y: h - 54..h - 14)
            let act_y = h - 54;
            let act_h = 40;
            if client_y >= act_y && client_y <= act_y + act_h {
                if is_pending {
                    // [ Accept ] Button (x: 20..140)
                    if client_x >= 20 && client_x <= 140 {
                        crate::p2p::respond_to_pending_approval(true);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        let edit_raw = HWND_CHAT_EDIT.load(Ordering::Relaxed);
                        if edit_raw != 0 {
                            let _ = SetFocus(Some(HWND(edit_raw as *mut _)));
                        }
                        return LRESULT(0);
                    }
                    // [ Dismiss ] Button (x: 148..268)
                    if client_x >= 148 && client_x <= 268 {
                        crate::p2p::respond_to_pending_approval(false);
                        close_host_widget();
                        return LRESULT(0);
                    }
                } else {
                    // [ Disconnect ] Button (x: 20..268)
                    if client_x >= 20 && client_x <= 268 {
                        crate::p2p::disconnect_host_session();
                        close_host_widget();
                        return LRESULT(0);
                    }
                }
            }

            // 4. Send Message Button (Right Column, x: w - 42..w - 10, y: h - 48..h - 14)
            let input_y = h - 48;
            if client_x >= w - 44 && client_x <= w - 10 && client_y >= input_y && client_y <= input_y + 34 {
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

            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let w = rc.right - rc.left;
            let h = rc.bottom - rc.top;

            let hover = if client_y <= 36 {
                if client_x >= w - 36 {
                    12 // Close
                } else if client_x >= w - 72 {
                    11 // Maximize
                } else if client_x >= w - 108 {
                    10 // Minimize
                } else {
                    0
                }
            } else if client_y >= 196 && client_y <= 242 {
                if client_x >= 20 && client_x <= 76 {
                    20 // Input sovereignty
                } else if client_x >= 84 && client_x <= 140 {
                    21 // Mic
                } else if client_x >= 148 && client_x <= 204 {
                    22 // Clipboard
                } else if client_x >= 212 && client_x <= 268 {
                    23 // File transfer
                } else {
                    0
                }
            } else if client_y >= h - 54 && client_y <= h - 14 {
                if is_pending {
                    if client_x >= 20 && client_x <= 140 {
                        30 // Accept
                    } else if client_x >= 148 && client_x <= 268 {
                        31 // Dismiss
                    } else {
                        0
                    }
                } else if client_x >= 20 && client_x <= 268 {
                    31 // Disconnect
                } else {
                    0
                }
            } else if client_x >= w - 44 && client_x <= w - 10 && client_y >= h - 48 && client_y <= h - 14 {
                32 // Send
            } else {
                0
            };

            let prev = HOVER_BTN.swap(hover, Ordering::Relaxed);
            if prev != hover {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let hover = HOVER_BTN.load(Ordering::Relaxed);
            if hover != 0 {
                let cursor = LoadCursorW(None, IDC_HAND).unwrap_or_default();
                SetCursor(Some(cursor));
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xffff) as usize;
            if id == ID_EDIT_HOST_CHAT {
                let code = ((wparam.0 >> 16) & 0xffff) as u16;
                if code == 0x0300 { // EN_CHANGE
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            HWND_HOST_WIDGET.store(0, Ordering::SeqCst);
            HWND_CHAT_EDIT.store(0, Ordering::Relaxed);
            PREV_HOST_CHAT_WNDPROC.store(0, Ordering::Relaxed);
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let w = rc.right - rc.left;
            let h = rc.bottom - rc.top;

            let hdc_mem = CreateCompatibleDC(Some(hdc));
            let hbm = CreateCompatibleBitmap(hdc, w, h);
            let old_bm = SelectObject(hdc_mem, HGDIOBJ(hbm.0));

            // Desky signature background canvas (PK_BG #f5f7f5)
            let bg_brush = CreateSolidBrush(PK_BG);
            FillRect(hdc_mem, &rc, bg_brush);
            let _ = DeleteObject(HGDIOBJ(bg_brush.0));

            // Outer border line (PK_CARD_BORDER #e2e7e3)
            let border_brush = CreateSolidBrush(PK_CARD_BORDER);
            let border_pen = CreatePen(PS_SOLID, 1, PK_CARD_BORDER);
            let old_pen = SelectObject(hdc_mem, HGDIOBJ(border_pen.0));
            let null_br = GetStockObject(NULL_BRUSH);
            let old_br = SelectObject(hdc_mem, null_br);
            RoundRect(hdc_mem, 0, 0, w, h, 12, 12);
            let _ = SelectObject(hdc_mem, old_br);
            let _ = SelectObject(hdc_mem, old_pen);
            let _ = DeleteObject(HGDIOBJ(border_pen.0));

            let hover = HOVER_BTN.load(Ordering::Relaxed);
            let is_pending = is_approval_active();
            let active_info = crate::p2p::get_active_host_session_info();
            let is_connected = active_info.is_some();

            let (peer_name, peer_id, session_time) = if let Some(req) = crate::p2p::get_pending_approval() {
                let name = req.client_key.clone();
                let id = req.client_id.clone();
                (name, id, String::new())
            } else if let Some((id, secs)) = active_info {
                let mm = secs / 60;
                let ss = secs % 60;
                (format!("Client {}", id), id, format!("{:02}:{:02}", mm, ss))
            } else {
                ("Remote Peer".to_string(), "000000000".to_string(), String::new())
            };

            // =========================================================================
            // 1. TOP TITLE BAR (y: 0..36)
            // =========================================================================
            let bar_rc = RECT { left: 0, top: 0, right: w, bottom: 36 };
            FillRect(hdc_mem, &bar_rc, border_brush);

            let bar_fill = RECT { left: 1, top: 1, right: w - 1, bottom: 35 };
            let bar_brush = CreateSolidBrush(PK_BG);
            FillRect(hdc_mem, &bar_fill, bar_brush);
            let _ = DeleteObject(HGDIOBJ(bar_brush.0));

            // 1.1 Authentic Desky Logo (Top-Left)
            crate::ui::draw_desky_logo(hdc_mem, 12, 7, 22);

            // 1.2 "Desky" Brand Text
            let font_brand = create_segoe_font(13, FW_BOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_brand.0));
            SetBkMode(hdc_mem, TRANSPARENT);
            SetTextColor(hdc_mem, PK_GREEN_DARK);
            let mut brand_txt: Vec<u16> = "Desky".encode_utf16().collect();
            let mut brand_rc = RECT { left: 38, top: 0, right: 100, bottom: 36 };
            DrawTextW(hdc_mem, &mut brand_txt, &mut brand_rc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

            // 1.3 Active Session Header Pill (Desky Emerald Pill)
            let tab_w = 170;
            let tab_h = 26;
            let tab_x = 104;
            let tab_y = 5;
            draw_rounded_box(hdc_mem, tab_x, tab_y, tab_w, tab_h, PK_CARD, PK_GREEN, 6);

            // Online status indicator dot inside tab
            let dot_brush = CreateSolidBrush(if is_connected { COLORREF(0x0080DE4A) } else { COLORREF(0x004F6A2D) });
            let old_b = SelectObject(hdc_mem, HGDIOBJ(dot_brush.0));
            Ellipse(hdc_mem, tab_x + 10, tab_y + 9, tab_x + 18, tab_y + 17);
            let _ = SelectObject(hdc_mem, old_b);
            let _ = DeleteObject(HGDIOBJ(dot_brush.0));

            let font_tab_title = create_segoe_font(11, FW_SEMIBOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_tab_title.0));
            SetTextColor(hdc_mem, PK_TEXT);
            let display_tab_name = if peer_name.len() > 14 { format!("{}...", &peer_name[..12]) } else { peer_name.clone() };
            let mut tab_txt: Vec<u16> = display_tab_name.encode_utf16().collect();
            let mut tab_rc = RECT { left: tab_x + 24, top: tab_y, right: tab_x + tab_w - 10, bottom: tab_y + tab_h };
            DrawTextW(hdc_mem, &mut tab_txt, &mut tab_rc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

            // 1.4 Window Controls (Minimize, Maximize/Restore, Close)
            // Minimize
            let min_rc = RECT { left: w - 108, top: 0, right: w - 72, bottom: 36 };
            if hover == 10 {
                let hover_br = CreateSolidBrush(PK_BTN_HOVER);
                FillRect(hdc_mem, &min_rc, hover_br);
                let _ = DeleteObject(HGDIOBJ(hover_br.0));
            }
            let pen_ctrl = CreatePen(PS_SOLID, 1, PK_TEXT);
            let old_cpen = SelectObject(hdc_mem, HGDIOBJ(pen_ctrl.0));
            MoveToEx(hdc_mem, w - 95, 20, None);
            LineTo(hdc_mem, w - 85, 20);

            // Maximize / Restore
            let max_rc = RECT { left: w - 72, top: 0, right: w - 36, bottom: 36 };
            if hover == 11 {
                let hover_br = CreateSolidBrush(PK_BTN_HOVER);
                FillRect(hdc_mem, &max_rc, hover_br);
                let _ = DeleteObject(HGDIOBJ(hover_br.0));
            }
            if IsZoomed(hwnd).as_bool() {
                // Restore icon (double square)
                RoundRect(hdc_mem, w - 57, 12, w - 47, 22, 2, 2);
                RoundRect(hdc_mem, w - 60, 15, w - 50, 25, 2, 2);
            } else {
                // Maximize icon (single square)
                RoundRect(hdc_mem, w - 59, 13, w - 49, 23, 2, 2);
            }

            // Close
            let close_rc = RECT { left: w - 36, top: 0, right: w, bottom: 36 };
            if hover == 12 {
                let close_br = CreateSolidBrush(COLORREF(0x003333E8)); // hover red
                FillRect(hdc_mem, &close_rc, close_br);
                let _ = DeleteObject(HGDIOBJ(close_br.0));
                let pen_white = CreatePen(PS_SOLID, 2, COLORREF(0x00FFFFFF));
                let _ = SelectObject(hdc_mem, HGDIOBJ(pen_white.0));
                MoveToEx(hdc_mem, w - 24, 13, None);
                LineTo(hdc_mem, w - 14, 23);
                MoveToEx(hdc_mem, w - 14, 13, None);
                LineTo(hdc_mem, w - 24, 23);
                let _ = SelectObject(hdc_mem, old_cpen);
                let _ = DeleteObject(HGDIOBJ(pen_white.0));
            } else {
                MoveToEx(hdc_mem, w - 24, 13, None);
                LineTo(hdc_mem, w - 14, 23);
                MoveToEx(hdc_mem, w - 14, 13, None);
                LineTo(hdc_mem, w - 24, 23);
                let _ = SelectObject(hdc_mem, old_cpen);
            }
            let _ = DeleteObject(HGDIOBJ(pen_ctrl.0));

            // =========================================================================
            // 2. LEFT COLUMN (Identity & Real Host Permissions)
            // =========================================================================
            let col_left_w = 280;
            let left_rc = RECT { left: 0, top: 36, right: col_left_w, bottom: h };
            let card_brush = CreateSolidBrush(PK_CARD);
            FillRect(hdc_mem, &left_rc, card_brush);

            // Dividing line between left and right column
            let div_line = RECT { left: col_left_w, top: 36, right: col_left_w + 1, bottom: h };
            FillRect(hdc_mem, &div_line, border_brush);

            // 2.1 Authentic Desky Emblem Avatar (x: 20, y: 52, size: 84)
            crate::ui::draw_desky_logo(hdc_mem, 20, 52, 84);

            // 2.2 Remote Peer Identity Header (Right of Avatar: x: 116)
            let font_name = create_segoe_font(14, FW_BOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_name.0));
            SetTextColor(hdc_mem, PK_GREEN_DARK);
            let mut name_txt: Vec<u16> = peer_name.encode_utf16().collect();
            let mut name_rc = RECT { left: 116, top: 54, right: col_left_w - 12, bottom: 76 };
            DrawTextW(hdc_mem, &mut name_txt, &mut name_rc, DT_LEFT | DT_SINGLELINE);

            // Remote Peer ID in parentheses
            let font_id = create_segoe_font(11, FW_SEMIBOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_id.0));
            SetTextColor(hdc_mem, PK_MUTED);
            let clean_id = crate::p2p::normalize_id(&peer_id);
            let mut id_txt: Vec<u16> = format!("({})", clean_id).encode_utf16().collect();
            let mut id_rc = RECT { left: 116, top: 76, right: col_left_w - 12, bottom: 96 };
            DrawTextW(hdc_mem, &mut id_txt, &mut id_rc, DT_LEFT | DT_SINGLELINE);

            // Status prompt or active duration
            let font_desc = create_segoe_font(10, FW_NORMAL);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_desc.0));
            SetTextColor(hdc_mem, PK_TEXT);
            let desc_str = if is_pending {
                "would like to view\nyour desk.".to_string()
            } else {
                format!("Connected active\nDuration: {}", session_time)
            };
            let mut desc_txt: Vec<u16> = desc_str.encode_utf16().collect();
            let mut desc_rc = RECT { left: 116, top: 96, right: col_left_w - 12, bottom: 136 };
            DrawTextW(hdc_mem, &mut desc_txt, &mut desc_rc, DT_LEFT);

            // 2.3 Security Badges: Desky Green Lock & Verified Shield
            crate::ui::draw_lock(hdc_mem, 22, 146, 14, PK_GREEN, 2);
            crate::ui::draw_check(hdc_mem, 42, 147, 13, PK_GREEN, 2);
            let font_sec = create_segoe_font(9, FW_NORMAL);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_sec.0));
            SetTextColor(hdc_mem, PK_MUTED);
            let mut sec_txt: Vec<u16> = "End-to-End Encrypted".encode_utf16().collect();
            let mut sec_rc = RECT { left: 60, top: 145, right: col_left_w - 12, bottom: 162 };
            DrawTextW(hdc_mem, &mut sec_txt, &mut sec_rc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

            // 2.4 Permissions Header
            let font_perm_hdr = create_segoe_font(11, FW_SEMIBOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_perm_hdr.0));
            SetTextColor(hdc_mem, PK_MUTED);
            let mut perm_txt: Vec<u16> = "Permissions".encode_utf16().collect();
            let mut perm_rc = RECT { left: 20, top: 172, right: col_left_w - 20, bottom: 190 };
            DrawTextW(hdc_mem, &mut perm_txt, &mut perm_rc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

            // 2.5 Active Permission Buttons in Iconic Desky Green (y: 196..242)
            // 4 actual functional options:
            // Btn 0: Input Sovereignty (Keyboard & Mouse)
            // Btn 1: Voice / Microphone
            // Btn 2: Clipboard Sync
            // Btn 3: File Transfer
            let btn_y = 196;
            let btn_w = 56;
            let btn_h = 46;
            let gap = 8;

            let input_allowed = crate::p2p::is_host_remote_input_allowed();
            let mic_allowed = crate::windows::audio::is_host_mic_active();
            let clip_allowed = crate::p2p::is_host_clipboard_allowed();
            let file_allowed = crate::p2p::is_host_file_transfer_allowed();

            // Helper for drawing an option button
            let draw_perm_btn = |hdc: HDC, x: i32, is_allowed: bool, is_hover: bool| {
                let bg = if is_hover {
                    PK_BTN_HOVER
                } else if is_allowed {
                    PK_GREEN_TINT
                } else {
                    PK_CARD
                };
                let border = if is_allowed { PK_GREEN } else { PK_CARD_BORDER };
                draw_rounded_box(hdc, x, btn_y, btn_w, btn_h, bg, border, 8);
            };

            // Option 0: Keyboard & Mouse
            let b0_x = 20;
            draw_perm_btn(hdc_mem, b0_x, input_allowed, hover == 20);
            crate::ui::draw_keyboard_mouse(
                hdc_mem,
                b0_x + 16,
                btn_y + 11,
                24,
                if input_allowed { PK_GREEN } else { PK_MUTED },
                2,
            );

            // Option 1: Audio / Mic
            let b1_x = b0_x + btn_w + gap;
            draw_perm_btn(hdc_mem, b1_x, mic_allowed, hover == 21);
            if mic_allowed {
                crate::ui::draw_mic(hdc_mem, b1_x + 17, btn_y + 11, 22, PK_GREEN, 2);
            } else {
                crate::ui::draw_mic_off(hdc_mem, b1_x + 17, btn_y + 11, 22, PK_MUTED, 2);
            }

            // Option 2: Clipboard Sync
            let b2_x = b1_x + btn_w + gap;
            draw_perm_btn(hdc_mem, b2_x, clip_allowed, hover == 22);
            crate::ui::draw_copy(hdc_mem, b2_x + 17, btn_y + 11, 22, if clip_allowed { PK_GREEN } else { PK_MUTED }, 2);

            // Option 3: File Transfer
            let b3_x = b2_x + btn_w + gap;
            draw_perm_btn(hdc_mem, b3_x, file_allowed, hover == 23);
            crate::ui::draw_folder(hdc_mem, b3_x + 17, btn_y + 11, 22, if file_allowed { PK_GREEN } else { PK_MUTED }, 2);

            // 2.6 Bottom Action Area (Pinned to bottom: act_y = h - 54, act_h = 40)
            let act_y = h - 54;
            let act_h = 40;
            let font_action = create_segoe_font(13, FW_BOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_action.0));

            if is_pending {
                // [ Accept ] Button (x: 20..140) in Iconic Desky Green
                let acc_bg = if hover == 30 { PK_GREEN_DARK } else { PK_GREEN };
                draw_rounded_box(hdc_mem, 20, act_y, 120, act_h, acc_bg, PK_GREEN_DARK, 8);
                SetTextColor(hdc_mem, COLORREF(0x00FFFFFF));
                let mut acc_txt: Vec<u16> = "Accept".encode_utf16().collect();
                let mut acc_rc = RECT { left: 20, top: act_y, right: 140, bottom: act_y + act_h };
                DrawTextW(hdc_mem, &mut acc_txt, &mut acc_rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

                // [ Dismiss ] Button (x: 148..268) in Red
                let dis_bg = if hover == 31 { COLORREF(0x002424A8) } else { PK_RED };
                draw_rounded_box(hdc_mem, 148, act_y, 120, act_h, dis_bg, dis_bg, 8);
                SetTextColor(hdc_mem, COLORREF(0x00FFFFFF));
                let mut dis_txt: Vec<u16> = "Dismiss".encode_utf16().collect();
                let mut dis_rc = RECT { left: 148, top: act_y, right: 268, bottom: act_y + act_h };
                DrawTextW(hdc_mem, &mut dis_txt, &mut dis_rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            } else {
                // [ Disconnect ] Button across whole column width (x: 20..268)
                let disc_bg = if hover == 31 { COLORREF(0x002424A8) } else { PK_RED };
                draw_rounded_box(hdc_mem, 20, act_y, 248, act_h, disc_bg, disc_bg, 8);
                SetTextColor(hdc_mem, COLORREF(0x00FFFFFF));
                let mut disc_txt: Vec<u16> = "Disconnect".encode_utf16().collect();
                let mut disc_rc = RECT { left: 20, top: act_y, right: 268, bottom: act_y + act_h };
                DrawTextW(hdc_mem, &mut disc_txt, &mut disc_rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            }

            // =========================================================================
            // 3. RIGHT COLUMN (Tabbed Session Log & Real-Time Chat)
            // =========================================================================
            let right_rc = RECT { left: col_left_w + 1, top: 36, right: w, bottom: h };
            FillRect(hdc_mem, &right_rc, card_brush);

            // 3.1 Tabs Bar (y: 36..68)
            let tab_bar_rc = RECT { left: col_left_w + 1, top: 36, right: w, bottom: 68 };
            let tab_bar_br = CreateSolidBrush(PK_BG);
            FillRect(hdc_mem, &tab_bar_rc, tab_bar_br);
            let _ = DeleteObject(HGDIOBJ(tab_bar_br.0));

            // Tab bottom border
            let tab_div = RECT { left: col_left_w + 1, top: 67, right: w, bottom: 68 };
            FillRect(hdc_mem, &tab_div, border_brush);

            // Tab 1: "Chat" (Active)
            let tab_chat_rc = RECT { left: col_left_w + 1, top: 36, right: col_left_w + 130, bottom: 67 };
            FillRect(hdc_mem, &tab_chat_rc, card_brush);
            // Green indicator line on active tab
            let active_line = RECT { left: col_left_w + 1, top: 36, right: col_left_w + 130, bottom: 38 };
            let green_br = CreateSolidBrush(PK_GREEN);
            FillRect(hdc_mem, &active_line, green_br);
            let _ = DeleteObject(HGDIOBJ(green_br.0));

            let font_tab = create_segoe_font(11, FW_SEMIBOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_tab.0));
            SetTextColor(hdc_mem, PK_GREEN_DARK);
            let mut chat_tab_txt: Vec<u16> = "Chat".encode_utf16().collect();
            let mut ctab_rc = RECT { left: col_left_w + 1, top: 38, right: col_left_w + 130, bottom: 67 };
            DrawTextW(hdc_mem, &mut chat_tab_txt, &mut ctab_rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

            // Tab 2: "File Transfer" (Inactive)
            SetTextColor(hdc_mem, PK_MUTED);
            let mut ft_tab_txt: Vec<u16> = "File Transfer".encode_utf16().collect();
            let mut fttab_rc = RECT { left: col_left_w + 130, top: 36, right: col_left_w + 260, bottom: 67 };
            DrawTextW(hdc_mem, &mut ft_tab_txt, &mut fttab_rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

            // 3.2 Header: "Open Chat Log"
            let font_log_hdr = create_segoe_font(11, FW_SEMIBOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_log_hdr.0));
            SetTextColor(hdc_mem, PK_GREEN_DARK);
            let mut ocl_txt: Vec<u16> = "Open Chat Log".encode_utf16().collect();
            let mut ocl_rc = RECT { left: col_left_w + 16, top: 76, right: w - 16, bottom: 94 };
            DrawTextW(hdc_mem, &mut ocl_txt, &mut ocl_rc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

            // Subtle underline
            let pen_link = CreatePen(PS_SOLID, 1, PK_GREEN);
            let old_lpen = SelectObject(hdc_mem, HGDIOBJ(pen_link.0));
            MoveToEx(hdc_mem, col_left_w + 16, 94, None);
            LineTo(hdc_mem, col_left_w + 105, 94);
            let _ = SelectObject(hdc_mem, old_lpen);
            let _ = DeleteObject(HGDIOBJ(pen_link.0));

            // 3.3 Event Banner: Monitor Icon + Session Request Received Event
            let font_evt_hdr = create_segoe_font(11, FW_BOLD);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_evt_hdr.0));
            SetTextColor(hdc_mem, PK_TEXT);
            let mut p_name_txt: Vec<u16> = peer_name.encode_utf16().collect();
            let mut p_name_rc = RECT { left: col_left_w + 16, top: 104, right: w - 80, bottom: 122 };
            DrawTextW(hdc_mem, &mut p_name_txt, &mut p_name_rc, DT_LEFT | DT_SINGLELINE);

            let font_time = create_segoe_font(9, FW_NORMAL);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_time.0));
            SetTextColor(hdc_mem, PK_MUTED);
            let now_time = crate::windows::ui::launcher::get_local_time_string();
            let short_time = if now_time.len() >= 5 { &now_time[..5] } else { "10:24" };
            let mut time_txt: Vec<u16> = short_time.encode_utf16().collect();
            let mut time_rc = RECT { left: w - 80, top: 104, right: w - 16, bottom: 122 };
            DrawTextW(hdc_mem, &mut time_txt, &mut time_rc, DT_RIGHT | DT_SINGLELINE);

            // Monitor Icon + "Session request received"
            crate::ui::draw_monitor(hdc_mem, col_left_w + 16, 126, 18, PK_GREEN, 2);
            let font_evt = create_segoe_font(10, FW_NORMAL);
            let _ = SelectObject(hdc_mem, HGDIOBJ(font_evt.0));
            SetTextColor(hdc_mem, PK_TEXT);
            let evt_label = if is_pending { "Session request received" } else { "Session established & active" };
            let mut srr_txt: Vec<u16> = evt_label.encode_utf16().collect();
            let mut srr_rc = RECT { left: col_left_w + 40, top: 126, right: w - 16, bottom: 144 };
            DrawTextW(hdc_mem, &mut srr_txt, &mut srr_rc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

            // 3.4 Dynamic Real-Time Chat Stream (y: 154..h - 56)
            let msgs = crate::p2p::get_chat_messages();
            let mut cur_y = 154;
            let font_bubble = create_segoe_font(10, FW_NORMAL);
            let font_author = create_segoe_font(9, FW_SEMIBOLD);

            for msg in msgs.iter().rev().take(6).collect::<Vec<_>>().into_iter().rev() {
                if cur_y + 36 > h - 56 {
                    break;
                }
                let is_me = msg.is_me;
                let bubble_w = ((w - col_left_w - 60) * 7 / 10).min(320);
                let bubble_x = if is_me { w - bubble_w - 18 } else { col_left_w + 18 };

                let _ = SelectObject(hdc_mem, HGDIOBJ(font_author.0));
                SetTextColor(hdc_mem, PK_MUTED);
                let mut auth_txt: Vec<u16> = msg.sender.encode_utf16().collect();
                let mut auth_rc = RECT { left: bubble_x + 4, top: cur_y, right: bubble_x + bubble_w, bottom: cur_y + 14 };
                DrawTextW(hdc_mem, &mut auth_txt, &mut auth_rc, DT_LEFT | DT_SINGLELINE);

                let bubble_bg = if is_me { PK_GREEN_TINT } else { PK_INPUT_BG };
                let bubble_border = if is_me { PK_GREEN } else { PK_CARD_BORDER };
                draw_rounded_box(hdc_mem, bubble_x, cur_y + 14, bubble_w, 24, bubble_bg, bubble_border, 6);

                let _ = SelectObject(hdc_mem, HGDIOBJ(font_bubble.0));
                SetTextColor(hdc_mem, PK_TEXT);
                let mut body_txt: Vec<u16> = msg.text.encode_utf16().collect();
                let mut body_rc = RECT { left: bubble_x + 8, top: cur_y + 14, right: bubble_x + bubble_w - 8, bottom: cur_y + 38 };
                DrawTextW(hdc_mem, &mut body_txt, &mut body_rc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

                cur_y += 42;
            }

            // 3.5 Bottom Chat Input Area (y: h - 48..h - 14)
            let input_y = h - 48;
            let input_x = col_left_w + 16;
            let input_w = w - input_x - 56;
            let input_h = 34;

            // Box outline for edit control
            draw_rounded_box(hdc_mem, input_x - 2, input_y - 1, input_w + 4, input_h, PK_INPUT_BG, PK_CARD_BORDER, 6);

            // Placeholder "Message" if edit control is empty
            let edit_raw = HWND_CHAT_EDIT.load(Ordering::Relaxed);
            let is_empty = if edit_raw != 0 {
                GetWindowTextLengthW(HWND(edit_raw as *mut _)) == 0
            } else {
                true
            };

            if is_empty {
                let font_ph = create_segoe_font(11, FW_NORMAL);
                let _ = SelectObject(hdc_mem, HGDIOBJ(font_ph.0));
                SetTextColor(hdc_mem, COLORREF(0x0094A3B8));
                let mut ph_txt: Vec<u16> = "Message".encode_utf16().collect();
                let mut ph_rc = RECT { left: input_x + 6, top: input_y, right: input_x + input_w - 10, bottom: input_y + input_h };
                DrawTextW(hdc_mem, &mut ph_txt, &mut ph_rc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
            }

            // Send Button (Desky Green with white paper airplane / arrow)
            let send_x = w - 44;
            let send_y = input_y;
            let send_size = 34;
            let send_bg = if hover == 32 { PK_GREEN_DARK } else { PK_GREEN };
            draw_rounded_box(hdc_mem, send_x, send_y, send_size, send_size, send_bg, send_bg, 8);

            // White Arrow vector (➤)
            let pen_arrow = CreatePen(PS_SOLID, 2, COLORREF(0x00FFFFFF));
            let old_ap = SelectObject(hdc_mem, HGDIOBJ(pen_arrow.0));
            MoveToEx(hdc_mem, send_x + 11, send_y + 11, None);
            LineTo(hdc_mem, send_x + 23, send_y + 17);
            LineTo(hdc_mem, send_x + 11, send_y + 23);
            LineTo(hdc_mem, send_x + 14, send_y + 17);
            LineTo(hdc_mem, send_x + 11, send_y + 11);
            let _ = SelectObject(hdc_mem, old_ap);
            let _ = DeleteObject(HGDIOBJ(pen_arrow.0));

            // Clean up GDI objects
            let _ = DeleteObject(HGDIOBJ(card_brush.0));
            let _ = DeleteObject(HGDIOBJ(border_brush.0));

            let _ = BitBlt(hdc, 0, 0, w, h, Some(hdc_mem), 0, 0, SRCCOPY);
            let _ = SelectObject(hdc_mem, old_bm);
            let _ = DeleteObject(HGDIOBJ(hbm.0));
            let _ = windows::Win32::Graphics::Gdi::DeleteDC(hdc_mem);

            EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
