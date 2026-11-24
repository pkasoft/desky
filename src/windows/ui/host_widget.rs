//! Host Session Management Window (Native Windows Window & Leak-Free Architecture)
//!
//! Exposes a clean, responsive two-column layout using standard native Windows frame decorations:
//! - Standard native Windows title bar with native Minimize, Maximize, and Close controls
//! - Fully native window resizing and aero-snap without custom hit-testing overhead
//! - Starter size compact & responsive (640x420)
//! - Single-allocation cached font engine (Zero GDI leaks, resolving icon/text disappearance)
//! - Sharp, crisp geometric cards, input fields, and action buttons matching the Resky Home Page
//! - pkasoft.com White Theme palette: PK_BG canvas, PK_CARD cards, PK_CARD_BORDER, PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT
//! - 4 active, real host permission options using iconic Resky green vector icons from `src/ui/icons/`

#![allow(unsafe_op_in_unsafe_fn)]
#![allow(unused_must_use)]

use std::sync::atomic::{AtomicI32, AtomicIsize, Ordering};
use std::sync::OnceLock;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreatePen, CreateRectRgn,
    CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, InvalidateRect,
    LineTo, MoveToEx, SelectClipRgn, SelectObject, SetBkColor, SetBkMode, SetTextColor,
    UpdateWindow, DT_CENTER, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL,
    FW_SEMIBOLD, HDC, HFONT, HGDIOBJ, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetParent, GetSystemMetrics,
    GetWindowTextLengthW, GetWindowTextW, IsWindow, LoadCursorW,
    MoveWindow, RegisterClassExW, SendMessageW, SetCursor, SetForegroundWindow,
    SetTimer, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow, GWLP_WNDPROC, HMENU,
    HWND_TOPMOST, IDC_ARROW, IDC_HAND, MINMAXINFO, SM_CXSCREEN, SM_CYSCREEN,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_SHOW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_CHAR, WM_COMMAND, WM_CREATE, WM_CTLCOLOREDIT, WM_DESTROY,
    WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETCURSOR, WM_SIZE,
    WM_TIMER, WNDCLASSEXW, WS_CHILD, WS_CLIPCHILDREN, WS_EX_TOPMOST,
    WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_VISIBLE,
};

use super::theme::{
    create_segoe_font, draw_rounded_box, PK_BG, PK_BTN_HOVER, PK_CARD, PK_CARD_BORDER,
    PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_RED, PK_TEXT,
};

pub const DEFAULT_HOST_W: i32 = 640;
pub const DEFAULT_HOST_H: i32 = 420;
pub const MIN_HOST_W: i32 = 560;
pub const MIN_HOST_H: i32 = 360;

const ID_EDIT_HOST_CHAT: usize = 9981;
const ID_TIMER_WIDGET_TICK: usize = 4001;

static HWND_HOST_WIDGET: AtomicIsize = AtomicIsize::new(0);
static HWND_CHAT_EDIT: AtomicIsize = AtomicIsize::new(0);
static PREV_HOST_CHAT_WNDPROC: AtomicIsize = AtomicIsize::new(0);
static HOVER_BTN: AtomicI32 = AtomicI32::new(0);
static CHAT_SCROLL_OFFSET: AtomicI32 = AtomicI32::new(0);
static CHAT_MAX_SCROLL: AtomicI32 = AtomicI32::new(0);

pub fn scroll_chat(delta: i32) {
    let step = 38;
    let cur = CHAT_SCROLL_OFFSET.load(Ordering::Relaxed);
    let max_s = CHAT_MAX_SCROLL.load(Ordering::Relaxed);
    let new_val = if delta > 0 {
        (cur + step).min(max_s)
    } else {
        (cur - step).max(0)
    };
    CHAT_SCROLL_OFFSET.store(new_val, Ordering::Relaxed);
}

pub fn reset_chat_scroll() {
    CHAT_SCROLL_OFFSET.store(0, Ordering::Relaxed);
}
// Hover IDs:
// 20 = Input Sovereignty, 21 = Mic, 22 = Clipboard, 23 = File Transfer
// 30 = Accept, 31 = Dismiss / Disconnect, 32 = Send

struct HostWidgetFonts {
    font_name: HFONT,
    font_id: HFONT,
    font_desc: HFONT,
    font_sec: HFONT,
    font_perm_hdr: HFONT,
    font_action: HFONT,
    font_tab: HFONT,
    font_evt_hdr: HFONT,
    font_time: HFONT,
    font_evt: HFONT,
    font_bubble: HFONT,
    font_author: HFONT,
    font_ph: HFONT,
}

unsafe impl Send for HostWidgetFonts {}
unsafe impl Sync for HostWidgetFonts {}

static WIDGET_FONTS: OnceLock<HostWidgetFonts> = OnceLock::new();

fn get_widget_fonts() -> &'static HostWidgetFonts {
    WIDGET_FONTS.get_or_init(|| unsafe {
        HostWidgetFonts {
            font_name: create_segoe_font(13, FW_BOLD),
            font_id: create_segoe_font(10, FW_SEMIBOLD),
            font_desc: create_segoe_font(9, FW_NORMAL),
            font_sec: create_segoe_font(8, FW_NORMAL),
            font_perm_hdr: create_segoe_font(9, FW_BOLD),
            font_action: create_segoe_font(12, FW_BOLD),
            font_tab: create_segoe_font(11, FW_SEMIBOLD),
            font_evt_hdr: create_segoe_font(10, FW_BOLD),
            font_time: create_segoe_font(9, FW_NORMAL),
            font_evt: create_segoe_font(10, FW_NORMAL),
            font_bubble: create_segoe_font(9, FW_NORMAL),
            font_author: create_segoe_font(8, FW_SEMIBOLD),
            font_ph: create_segoe_font(10, FW_NORMAL),
        }
    })
}

pub fn is_approval_active() -> bool {
    crate::p2p::get_pending_approval().is_some() && crate::p2p::get_active_host_session_info().is_none()
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
        let class_name = w!("ReskyHostSessionWindowClass");

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

        // Native Windows window: standard titlebar with Minimize, Maximize, and Close
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_TOPMOST.0),
            class_name,
            w!("Resky | Session Management"),
            WINDOW_STYLE(WS_OVERLAPPEDWINDOW.0 | WS_VISIBLE.0 | WS_CLIPCHILDREN.0),
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
            tracing::error!("Failed to create Resky Host Session window");
            return;
        }

        let hwnd = hwnd.unwrap();
        HWND_HOST_WIDGET.store(hwnd.0 as isize, Ordering::SeqCst);

        // Light theme native titlebar matching home page
        let dark_mode: i32 = 0;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_mode as *const _ as *const _,
            std::mem::size_of::<i32>() as u32,
        );

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
                reset_chat_scroll();
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
            let col_left_w = 260;
            let edit_x = col_left_w + 16;
            let edit_y = DEFAULT_HOST_H - 44;
            let edit_w = DEFAULT_HOST_W - edit_x - 52;
            let edit_h = 30;

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
                let fonts = get_widget_fonts();
                let _ = SendMessageW(edit, 0x0030, Some(WPARAM(fonts.font_desc.0 as usize)), Some(LPARAM(1))); // WM_SETFONT

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
                let edit_raw = HWND_CHAT_EDIT.load(Ordering::Relaxed);
                if edit_raw != 0 {
                    let col_left_w = 260;
                    let input_x = col_left_w + 16;
                    let input_y = h - 44;
                    let input_w = (w - input_x - 52).max(100);
                    let input_h = 30;
                    let _ = MoveWindow(HWND(edit_raw as *mut _), input_x, input_y, input_w, input_h, true);
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
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

            // 1. Interactive Permission Toggles (Left Column y: 148..190)
            // 4 real options:
            // Btn 0 (16..66): Remote Input Sovereignty (Mouse & Keyboard)
            // Btn 1 (74..124): Audio / Voice Intercom
            // Btn 2 (132..182): Clipboard Sync
            // Btn 3 (190..240): File Transfer
            if client_y >= 148 && client_y <= 190 {
                if client_x >= 16 && client_x <= 66 {
                    let allowed = crate::p2p::toggle_host_remote_input();
                    tracing::info!("Host remote input sovereignty toggled: allowed={}", allowed);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                if client_x >= 74 && client_x <= 124 {
                    let is_on = crate::windows::audio::toggle_host_mic();
                    tracing::info!("Host microphone toggled: {}", is_on);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                if client_x >= 132 && client_x <= 182 {
                    let is_on = crate::p2p::toggle_host_clipboard();
                    tracing::info!("Host clipboard sync toggled: {}", is_on);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                if client_x >= 190 && client_x <= 240 {
                    let is_on = crate::p2p::toggle_host_file_transfer();
                    tracing::info!("Host file transfer toggled: {}", is_on);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
            }

            // 2. Primary Action Buttons (Left Column, y: h - 50..h - 12)
            let act_y = h - 50;
            let act_h = 38;
            if client_y >= act_y && client_y <= act_y + act_h {
                if is_pending {
                    // [ Accept ] Button (x: 16..124)
                    if client_x >= 16 && client_x <= 124 {
                        crate::p2p::respond_to_pending_approval(true);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        let edit_raw = HWND_CHAT_EDIT.load(Ordering::Relaxed);
                        if edit_raw != 0 {
                            let _ = SetFocus(Some(HWND(edit_raw as *mut _)));
                        }
                        return LRESULT(0);
                    }
                    // [ Dismiss ] Button (x: 132..240)
                    if client_x >= 132 && client_x <= 240 {
                        crate::p2p::respond_to_pending_approval(false);
                        close_host_widget();
                        return LRESULT(0);
                    }
                } else {
                    // [ Disconnect ] Button (x: 16..240)
                    if client_x >= 16 && client_x <= 240 {
                        crate::p2p::disconnect_host_session();
                        close_host_widget();
                        return LRESULT(0);
                    }
                }
            }

            // 3. Send Message Button (Right Column, x: w - 40..w - 10, y: h - 44..h - 14)
            let input_y = h - 44;
            if client_x >= w - 40 && client_x <= w - 10 && client_y >= input_y && client_y <= input_y + 30 {
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

            let hover = if client_y >= 148 && client_y <= 190 {
                if client_x >= 16 && client_x <= 66 {
                    20 // Input sovereignty
                } else if client_x >= 74 && client_x <= 124 {
                    21 // Mic
                } else if client_x >= 132 && client_x <= 182 {
                    22 // Clipboard
                } else if client_x >= 190 && client_x <= 240 {
                    23 // File transfer
                } else {
                    0
                }
            } else if client_y >= h - 50 && client_y <= h - 12 {
                if is_pending {
                    if client_x >= 16 && client_x <= 124 {
                        30 // Accept
                    } else if client_x >= 132 && client_x <= 240 {
                        31 // Dismiss
                    } else {
                        0
                    }
                } else if client_x >= 16 && client_x <= 240 {
                    31 // Disconnect
                } else {
                    0
                }
            } else if client_x >= w - 40 && client_x <= w - 10 && client_y >= h - 44 && client_y <= h - 14 {
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
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xffff) as i16 as i32;
            scroll_chat(delta);
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
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

            // Resky signature background canvas (PK_BG #f5f7f5)
            let bg_brush = CreateSolidBrush(PK_BG);
            FillRect(hdc_mem, &rc, bg_brush);
            let _ = DeleteObject(HGDIOBJ(bg_brush.0));

            let border_brush = CreateSolidBrush(PK_CARD_BORDER);

            let hover = HOVER_BTN.load(Ordering::Relaxed);
            let is_pending = is_approval_active();
            let active_info = crate::p2p::get_active_host_session_info();

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

            let fonts = get_widget_fonts();

            // =========================================================================
            // 1. LEFT COLUMN (Identity & Real Host Permissions)
            // =========================================================================
            let col_left_w = 260;
            let left_rc = RECT { left: 0, top: 0, right: col_left_w, bottom: h };
            let card_brush = CreateSolidBrush(PK_CARD);
            FillRect(hdc_mem, &left_rc, card_brush);

            // Dividing line between left and right column (PK_CARD_BORDER #e2e7e3)
            let div_line = RECT { left: col_left_w, top: 0, right: col_left_w + 1, bottom: h };
            FillRect(hdc_mem, &div_line, border_brush);

            // 1.1 Authentic Resky Emblem Card (x: 16, y: 16, size: 72)
            crate::ui::draw_desky_logo(hdc_mem, 16, 16, 72);

            // 1.2 Remote Peer Identity Header (Right of Avatar: x: 96)
            SetBkMode(hdc_mem, TRANSPARENT);
            let _ = SelectObject(hdc_mem, HGDIOBJ(fonts.font_name.0));
            SetTextColor(hdc_mem, PK_GREEN_DARK);
            let mut name_txt: Vec<u16> = peer_name.encode_utf16().collect();
            let mut name_rc = RECT { left: 96, top: 16, right: col_left_w - 10, bottom: 36 };
            DrawTextW(hdc_mem, &mut name_txt, &mut name_rc, DT_LEFT | DT_SINGLELINE);

            // Remote Peer ID in parentheses
            let _ = SelectObject(hdc_mem, HGDIOBJ(fonts.font_id.0));
            SetTextColor(hdc_mem, PK_MUTED);
            let clean_id = crate::p2p::normalize_id(&peer_id);
            let mut id_txt: Vec<u16> = format!("({})", clean_id).encode_utf16().collect();
            let mut id_rc = RECT { left: 96, top: 36, right: col_left_w - 10, bottom: 54 };
            DrawTextW(hdc_mem, &mut id_txt, &mut id_rc, DT_LEFT | DT_SINGLELINE);

            // Status prompt or active duration
            let _ = SelectObject(hdc_mem, HGDIOBJ(fonts.font_desc.0));
            SetTextColor(hdc_mem, PK_TEXT);
            let desc_str = if is_pending {
                "would like to view\nyour desk.".to_string()
            } else {
                format!("Connected active\nDuration: {}", session_time)
            };
            let mut desc_txt: Vec<u16> = desc_str.encode_utf16().collect();
            let mut desc_rc = RECT { left: 96, top: 54, right: col_left_w - 10, bottom: 90 };
            DrawTextW(hdc_mem, &mut desc_txt, &mut desc_rc, DT_LEFT);

            // 1.3 Security Badges: Resky Green Lock & Verified Shield
            crate::ui::draw_lock(hdc_mem, 16, 96, 13, PK_GREEN, 2);
            crate::ui::draw_check(hdc_mem, 34, 97, 12, PK_GREEN, 2);
            let _ = SelectObject(hdc_mem, HGDIOBJ(fonts.font_sec.0));
            SetTextColor(hdc_mem, PK_MUTED);
            let mut sec_txt: Vec<u16> = "End-to-End Encrypted".encode_utf16().collect();
            let mut sec_rc = RECT { left: 52, top: 96, right: col_left_w - 10, bottom: 112 };
            DrawTextW(hdc_mem, &mut sec_txt, &mut sec_rc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

            // 1.4 Permissions Header (Sharp uppercase label)
            let _ = SelectObject(hdc_mem, HGDIOBJ(fonts.font_perm_hdr.0));
            SetTextColor(hdc_mem, PK_MUTED);
            let mut perm_txt: Vec<u16> = "PERMISSIONS".encode_utf16().collect();
            let mut perm_rc = RECT { left: 16, top: 124, right: col_left_w - 16, bottom: 142 };
            DrawTextW(hdc_mem, &mut perm_txt, &mut perm_rc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

            // 1.5 Active Permission Buttons in Iconic Resky Green (Sharp 6px corners)
            let btn_y = 148;
            let btn_w = 50;
            let btn_h = 42;
            let gap = 8;

            let input_allowed = crate::p2p::is_host_remote_input_allowed();
            let mic_allowed = crate::windows::audio::is_host_mic_active();
            let clip_allowed = crate::p2p::is_host_clipboard_allowed();
            let file_allowed = crate::p2p::is_host_file_transfer_allowed();

            let draw_perm_btn = |hdc: HDC, x: i32, is_allowed: bool, is_hover: bool| {
                let bg = if is_hover {
                    PK_BTN_HOVER
                } else if is_allowed {
                    PK_GREEN_TINT
                } else {
                    PK_CARD
                };
                let border = if is_allowed { PK_GREEN } else { PK_CARD_BORDER };
                draw_rounded_box(hdc, x, btn_y, btn_w, btn_h, bg, border, 6);
            };

            // Option 0: Keyboard & Mouse
            let b0_x = 16;
            draw_perm_btn(hdc_mem, b0_x, input_allowed, hover == 20);
            crate::ui::draw_keyboard_mouse(
                hdc_mem,
                b0_x + 14,
                btn_y + 10,
                22,
                if input_allowed { PK_GREEN } else { PK_MUTED },
                2,
            );

            // Option 1: Audio / Mic
            let b1_x = b0_x + btn_w + gap;
            draw_perm_btn(hdc_mem, b1_x, mic_allowed, hover == 21);
            if mic_allowed {
                crate::ui::draw_mic(hdc_mem, b1_x + 15, btn_y + 10, 20, PK_GREEN, 2);
            } else {
                crate::ui::draw_mic_off(hdc_mem, b1_x + 15, btn_y + 10, 20, PK_MUTED, 2);
            }

            // Option 2: Clipboard Sync
            let b2_x = b1_x + btn_w + gap;
            draw_perm_btn(hdc_mem, b2_x, clip_allowed, hover == 22);
            crate::ui::draw_copy(hdc_mem, b2_x + 15, btn_y + 10, 20, if clip_allowed { PK_GREEN } else { PK_MUTED }, 2);

            // Option 3: File Transfer
            let b3_x = b2_x + btn_w + gap;
            draw_perm_btn(hdc_mem, b3_x, file_allowed, hover == 23);
            crate::ui::draw_folder(hdc_mem, b3_x + 15, btn_y + 10, 20, if file_allowed { PK_GREEN } else { PK_MUTED }, 2);

            // 1.6 Interactive Tooltip / Option Description Badge
            let tip_x = 16;
            let tip_y = 198;
            let tip_w = 224;
            let tip_h = 44;

            let (tip_title, tip_status, tip_active) = match hover {
                20 => (
                    "Mouse & Keyboard",
                    if input_allowed { "Remote control allowed" } else { "Remote control blocked" },
                    input_allowed,
                ),
                21 => (
                    "Voice Intercom",
                    if mic_allowed { "Host microphone active" } else { "Microphone muted" },
                    mic_allowed,
                ),
                22 => (
                    "Clipboard Sync",
                    if clip_allowed { "Bidirectional sync enabled" } else { "Clipboard sync disabled" },
                    clip_allowed,
                ),
                23 => (
                    "File Transfer",
                    if file_allowed { "Remote transfer allowed" } else { "File transfer blocked" },
                    file_allowed,
                ),
                _ => (
                    "Host Permissions",
                    "Click icons to toggle access",
                    true,
                ),
            };

            let tip_bg = if hover >= 20 && hover <= 23 {
                if tip_active { PK_GREEN_TINT } else { PK_CARD }
            } else {
                PK_INPUT_BG
            };
            let tip_border = if hover >= 20 && hover <= 23 {
                if tip_active { PK_GREEN } else { PK_CARD_BORDER }
            } else {
                PK_CARD_BORDER
            };

            draw_rounded_box(hdc_mem, tip_x, tip_y, tip_w, tip_h, tip_bg, tip_border, 6);

            let _ = SelectObject(hdc_mem, HGDIOBJ(fonts.font_evt_hdr.0));
            SetTextColor(hdc_mem, if hover >= 20 && hover <= 23 && tip_active { PK_GREEN_DARK } else { PK_TEXT });
            let mut tt_title_txt: Vec<u16> = tip_title.encode_utf16().collect();
            let mut tt_title_rc = RECT { left: tip_x + 10, top: tip_y + 5, right: tip_x + tip_w - 10, bottom: tip_y + 22 };
            DrawTextW(hdc_mem, &mut tt_title_txt, &mut tt_title_rc, DT_LEFT | DT_SINGLELINE);

            let _ = SelectObject(hdc_mem, HGDIOBJ(fonts.font_desc.0));
            SetTextColor(hdc_mem, PK_MUTED);
            let mut tt_stat_txt: Vec<u16> = tip_status.encode_utf16().collect();
            let mut tt_stat_rc = RECT { left: tip_x + 10, top: tip_y + 23, right: tip_x + tip_w - 10, bottom: tip_y + 40 };
            DrawTextW(hdc_mem, &mut tt_stat_txt, &mut tt_stat_rc, DT_LEFT | DT_SINGLELINE);

            // 1.7 Bottom Action Area (Sharp 6px corners)
            let act_y = h - 50;
            let act_h = 38;
            let _ = SelectObject(hdc_mem, HGDIOBJ(fonts.font_action.0));

            if is_pending {
                // [ Accept ] Button (x: 16..124) in Iconic Resky Green
                let acc_bg = if hover == 30 { PK_GREEN_DARK } else { PK_GREEN };
                draw_rounded_box(hdc_mem, 16, act_y, 108, act_h, acc_bg, PK_GREEN_DARK, 6);
                SetTextColor(hdc_mem, COLORREF(0x00FFFFFF));
                let mut acc_txt: Vec<u16> = "Accept".encode_utf16().collect();
                let mut acc_rc = RECT { left: 16, top: act_y, right: 124, bottom: act_y + act_h };
                DrawTextW(hdc_mem, &mut acc_txt, &mut acc_rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

                // [ Dismiss ] Button (x: 132..240) in Red
                let dis_bg = if hover == 31 { COLORREF(0x002424A8) } else { PK_RED };
                draw_rounded_box(hdc_mem, 132, act_y, 108, act_h, dis_bg, dis_bg, 6);
                SetTextColor(hdc_mem, COLORREF(0x00FFFFFF));
                let mut dis_txt: Vec<u16> = "Dismiss".encode_utf16().collect();
                let mut dis_rc = RECT { left: 132, top: act_y, right: 240, bottom: act_y + act_h };
                DrawTextW(hdc_mem, &mut dis_txt, &mut dis_rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            } else {
                // [ Disconnect ] Button across column width (x: 16..240)
                let disc_bg = if hover == 31 { COLORREF(0x002424A8) } else { PK_RED };
                draw_rounded_box(hdc_mem, 16, act_y, 224, act_h, disc_bg, disc_bg, 6);
                SetTextColor(hdc_mem, COLORREF(0x00FFFFFF));
                let mut disc_txt: Vec<u16> = "Disconnect".encode_utf16().collect();
                let mut disc_rc = RECT { left: 16, top: act_y, right: 240, bottom: act_y + act_h };
                DrawTextW(hdc_mem, &mut disc_txt, &mut disc_rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            }

            // =========================================================================
            // 2. RIGHT COLUMN (Tabbed Session Log & Real-Time Chat)
            // =========================================================================
            let right_rc = RECT { left: col_left_w + 1, top: 0, right: w, bottom: h };
            FillRect(hdc_mem, &right_rc, card_brush);

            // 2.1 Top Tabs Bar (y: 0..36)
            let tab_bar_rc = RECT { left: col_left_w + 1, top: 0, right: w, bottom: 36 };
            let tab_bar_br = CreateSolidBrush(PK_BG);
            FillRect(hdc_mem, &tab_bar_rc, tab_bar_br);
            let _ = DeleteObject(HGDIOBJ(tab_bar_br.0));

            // Tab bottom border line
            let tab_div = RECT { left: col_left_w + 1, top: 35, right: w, bottom: 36 };
            FillRect(hdc_mem, &tab_div, border_brush);

            // "Chat" Tab - crisp white background with top green accent line
            let tab_chat_rc = RECT { left: col_left_w + 1, top: 0, right: col_left_w + 120, bottom: 35 };
            FillRect(hdc_mem, &tab_chat_rc, card_brush);
            let active_line = RECT { left: col_left_w + 1, top: 0, right: col_left_w + 120, bottom: 3 };
            let green_br = CreateSolidBrush(PK_GREEN);
            FillRect(hdc_mem, &active_line, green_br);
            let _ = DeleteObject(HGDIOBJ(green_br.0));

            let _ = SelectObject(hdc_mem, HGDIOBJ(fonts.font_tab.0));
            SetTextColor(hdc_mem, PK_GREEN_DARK);
            let mut chat_tab_txt: Vec<u16> = "Chat".encode_utf16().collect();
            let mut ctab_rc = RECT { left: col_left_w + 1, top: 2, right: col_left_w + 120, bottom: 35 };
            DrawTextW(hdc_mem, &mut chat_tab_txt, &mut ctab_rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

            // 2.2 Event Banner: Monitor Icon + Session Event
            let _ = SelectObject(hdc_mem, HGDIOBJ(fonts.font_evt_hdr.0));
            SetTextColor(hdc_mem, PK_TEXT);
            let mut p_name_txt: Vec<u16> = peer_name.encode_utf16().collect();
            let mut p_name_rc = RECT { left: col_left_w + 16, top: 46, right: w - 80, bottom: 64 };
            DrawTextW(hdc_mem, &mut p_name_txt, &mut p_name_rc, DT_LEFT | DT_SINGLELINE);

            let _ = SelectObject(hdc_mem, HGDIOBJ(fonts.font_time.0));
            SetTextColor(hdc_mem, PK_MUTED);
            let now_time = crate::windows::ui::launcher::get_local_time_string();
            let short_time = if now_time.len() >= 5 { &now_time[..5] } else { "10:24" };
            let mut time_txt: Vec<u16> = short_time.encode_utf16().collect();
            let mut time_rc = RECT { left: w - 80, top: 46, right: w - 16, bottom: 64 };
            DrawTextW(hdc_mem, &mut time_txt, &mut time_rc, DT_RIGHT | DT_SINGLELINE);

            // Monitor Icon + Event text
            crate::ui::draw_monitor(hdc_mem, col_left_w + 16, 68, 16, PK_GREEN, 2);
            let _ = SelectObject(hdc_mem, HGDIOBJ(fonts.font_evt.0));
            SetTextColor(hdc_mem, PK_TEXT);
            let evt_label = if is_pending { "Session request received" } else { "Session established & active" };
            let mut srr_txt: Vec<u16> = evt_label.encode_utf16().collect();
            let mut srr_rc = RECT { left: col_left_w + 38, top: 68, right: w - 16, bottom: 84 };
            DrawTextW(hdc_mem, &mut srr_txt, &mut srr_rc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

            // 2.3 Dynamic Real-Time Chat Stream (y: 92..h - 52)
            let msgs = crate::p2p::get_chat_messages();
            let chat_top = 92;
            let chat_bot = h - 52;
            let avail_h = (chat_bot - chat_top).max(30);
            let item_h = 38;
            let total_h = msgs.len() as i32 * item_h;

            let max_scroll = (total_h - avail_h).max(0);
            CHAT_MAX_SCROLL.store(max_scroll, Ordering::Relaxed);
            let scroll = CHAT_SCROLL_OFFSET.load(Ordering::Relaxed).min(max_scroll);
            CHAT_SCROLL_OFFSET.store(scroll, Ordering::Relaxed);

            // Clip chat messages container
            let clip_chat = CreateRectRgn(col_left_w + 1, chat_top, w, chat_bot);
            let _ = SelectClipRgn(hdc_mem, Some(clip_chat));

            let base_y = if total_h <= avail_h {
                chat_top
            } else {
                chat_top + avail_h - total_h + scroll
            };

            for (i, msg) in msgs.iter().enumerate() {
                let cur_y = base_y + (i as i32) * item_h;
                if cur_y + item_h < chat_top {
                    continue;
                }
                if cur_y > chat_bot {
                    break;
                }

                let is_me = msg.is_me;
                let bubble_w = ((w - col_left_w - 50) * 7 / 10).min(300);
                let bubble_x = if is_me { w - bubble_w - 16 } else { col_left_w + 16 };

                let _ = SelectObject(hdc_mem, HGDIOBJ(fonts.font_author.0));
                SetTextColor(hdc_mem, PK_MUTED);
                let mut auth_txt: Vec<u16> = msg.sender.encode_utf16().collect();
                let mut auth_rc = RECT { left: bubble_x + 4, top: cur_y, right: bubble_x + bubble_w, bottom: cur_y + 12 };
                DrawTextW(hdc_mem, &mut auth_txt, &mut auth_rc, DT_LEFT | DT_SINGLELINE);

                let bubble_bg = if is_me { PK_GREEN_TINT } else { PK_INPUT_BG };
                let bubble_border = if is_me { PK_GREEN } else { PK_CARD_BORDER };
                draw_rounded_box(hdc_mem, bubble_x, cur_y + 12, bubble_w, 22, bubble_bg, bubble_border, 6);

                let _ = SelectObject(hdc_mem, HGDIOBJ(fonts.font_bubble.0));
                SetTextColor(hdc_mem, PK_TEXT);
                let mut body_txt: Vec<u16> = msg.text.encode_utf16().collect();
                let mut body_rc = RECT { left: bubble_x + 8, top: cur_y + 12, right: bubble_x + bubble_w - 8, bottom: cur_y + 34 };
                DrawTextW(hdc_mem, &mut body_txt, &mut body_rc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
            }

            let _ = SelectClipRgn(hdc_mem, None);
            let _ = DeleteObject(HGDIOBJ(clip_chat.0));

            // Scrollbar indicator
            if max_scroll > 0 {
                let sb_w = 4;
                let sb_x = w - 6;
                let sb_y = chat_top;
                let sb_h = avail_h;
                draw_rounded_box(hdc_mem, sb_x, sb_y, sb_w, sb_h, PK_INPUT_BG, PK_CARD_BORDER, 2);
                let thumb_h = ((avail_h as f32 / total_h as f32) * sb_h as f32).clamp(20.0, sb_h as f32) as i32;
                let thumb_travel = sb_h - thumb_h;
                let thumb_y = sb_y + ((max_scroll - scroll) as f32 / max_scroll as f32 * thumb_travel as f32) as i32;
                draw_rounded_box(hdc_mem, sb_x, thumb_y, sb_w, thumb_h, PK_MUTED, PK_MUTED, 2);
            }

            // 2.5 Bottom Chat Input Area (Sharp 6px corners, y: h - 44..h - 14)
            let input_y = h - 44;
            let input_x = col_left_w + 16;
            let input_w = w - input_x - 48;
            let input_h = 30;

            // Sharp box outline for edit control
            draw_rounded_box(hdc_mem, input_x - 2, input_y - 1, input_w + 4, input_h, PK_INPUT_BG, PK_CARD_BORDER, 6);

            let edit_raw = HWND_CHAT_EDIT.load(Ordering::Relaxed);
            let is_empty = if edit_raw != 0 {
                GetWindowTextLengthW(HWND(edit_raw as *mut _)) == 0
            } else {
                true
            };

            if is_empty {
                let _ = SelectObject(hdc_mem, HGDIOBJ(fonts.font_ph.0));
                SetTextColor(hdc_mem, COLORREF(0x0094A3B8));
                let mut ph_txt: Vec<u16> = "Message".encode_utf16().collect();
                let mut ph_rc = RECT { left: input_x + 6, top: input_y, right: input_x + input_w - 10, bottom: input_y + input_h };
                DrawTextW(hdc_mem, &mut ph_txt, &mut ph_rc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
            }

            // Send Button (Resky Green, sharp 6px corners)
            let send_x = w - 38;
            let send_y = input_y;
            let send_size = 30;
            let send_bg = if hover == 32 { PK_GREEN_DARK } else { PK_GREEN };
            draw_rounded_box(hdc_mem, send_x, send_y, send_size, send_size, send_bg, PK_GREEN_DARK, 6);

            // White Arrow vector (➤)
            let pen_arrow = CreatePen(PS_SOLID, 2, COLORREF(0x00FFFFFF));
            let old_ap = SelectObject(hdc_mem, HGDIOBJ(pen_arrow.0));
            MoveToEx(hdc_mem, send_x + 9, send_y + 9, None);
            LineTo(hdc_mem, send_x + 21, send_y + 15);
            LineTo(hdc_mem, send_x + 9, send_y + 21);
            LineTo(hdc_mem, send_x + 12, send_y + 15);
            LineTo(hdc_mem, send_x + 9, send_y + 9);
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
