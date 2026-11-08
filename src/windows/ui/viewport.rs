//! Dedicated Viewport Remote Stream Canvas Window & AnyDesk-Style Tabbed Session Manager

#![allow(unsafe_op_in_unsafe_fn)]
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tracing::info;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC,
    CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC, InvalidateRect,
    ReleaseDC, ScreenToClient, SelectObject, SetBkMode, SetBrushOrgEx, SetStretchBltMode,
    SetTextColor, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, COLORONCOLOR,
    DIB_RGB_COLORS, DT_CENTER, DT_END_ELLIPSIS, DT_SINGLELINE, DT_VCENTER, FW_BOLD,
    FW_NORMAL, FW_SEMIBOLD, HALFTONE, HBRUSH, HDC, HGDIOBJ, PAINTSTRUCT, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, RemoveClipboardFormatListener,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos, IsWindow,
    LoadCursorW, SetCursor, SetForegroundWindow, SetWindowDisplayAffinity, SetWindowTextW,
    ShowWindow, CW_USEDEFAULT, IDC_ARROW, IDC_HAND, MINMAXINFO, SW_RESTORE, SW_SHOW,
    WDA_EXCLUDEFROMCAPTURE, WINDOW_EX_STYLE, WM_CHAR, WM_CLIPBOARDUPDATE, WM_DESTROY, WM_DROPFILES,
    WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_SETCURSOR, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

use crate::p2p::SessionStatus;
use crate::protocol::{InputEvent, MouseButton};

use super::launcher::bring_main_window_to_front;
use super::theme::{
    create_segoe_font, draw_rounded_box, format_bitrate, PK_BG, PK_BTN_HOVER, PK_CARD,
    PK_CARD_BORDER, PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_RED, PK_TEXT,
};

pub(crate) static mut VIEWPORT_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());

pub(crate) static VIEWPORT_RUNNING: AtomicBool = AtomicBool::new(false);

static REMOTE_ORIG_W: AtomicU32 = AtomicU32::new(0);
static REMOTE_ORIG_H: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Debug, PartialEq)]
pub struct SessionTab {
    pub id: String,
    pub title: String,
}

static SESSION_TABS: OnceLock<Mutex<Vec<SessionTab>>> = OnceLock::new();
static ACTIVE_TAB_INDEX: AtomicUsize = AtomicUsize::new(0);
static HWND_VIEWPORT: AtomicIsize = AtomicIsize::new(0);
static TAB_HOVER_STATE: AtomicI32 = AtomicI32::new(-1);
static DRAGGING_TAB_INDEX: AtomicI32 = AtomicI32::new(-1);
static DRAG_START_POS: Mutex<Option<(i32, i32)>> = Mutex::new(None);

pub fn get_session_tabs() -> Vec<SessionTab> {
    let store = SESSION_TABS.get_or_init(|| Mutex::new(Vec::new()));
    store.lock().map(|g| g.clone()).unwrap_or_default()
}

const TOOLBAR_HEIGHT: i32 = 40;

// Display mode:
// 0 = Auto Fit (aspect ratio preserved, centered)
// 1 = Fill Window (stretched to borders)
// 2 = 1:1 Original resolution
static DISPLAY_MODE: AtomicU8 = AtomicU8::new(0);

// Target FPS: 30, 60, 120
pub(crate) static TARGET_FPS: AtomicU32 = AtomicU32::new(60);

// Toolbar visibility: true = shown, false = collapsed/hidden
static SHOW_TOOLBAR: AtomicBool = AtomicBool::new(true);

// Full-Screen Settings Page state
static SHOW_SETTINGS_MODAL: AtomicBool = AtomicBool::new(false);

// Active AnyDesk-style settings sidebar category:
// 0 = Display & Quality, 1 = Adaptive Bitrate (ABR), 2 = Performance & Input, 3 = Connection Info
static SETTINGS_CATEGORY: AtomicU8 = AtomicU8::new(0);

// Remote cursor capture toggle (renders live host cursor on stream)
pub(crate) static CAPTURE_CURSOR: AtomicBool = AtomicBool::new(true);

// Silent Admin Mode toggle: false = normal desktop stream, true = background admin manager
pub static VIEWPORT_IS_ADMIN_MODE: AtomicBool = AtomicBool::new(false);
static ADMIN_HOVER_STATE: AtomicI32 = AtomicI32::new(-1);


fn map_client_to_screen(
    client_x: i32,
    client_y: i32,
    client_w: i32,
    client_h: i32,
    screen_w: i32,
    screen_h: i32,
    toolbar_h: i32,
    mode: u8,
) -> Option<(i32, i32)> {
    if client_y < toolbar_h {
        return None; // Inside toolbar area
    }
    let view_y = client_y - toolbar_h;
    let view_h = client_h - toolbar_h;
    if view_h <= 0 || client_w <= 0 || screen_w <= 0 || screen_h <= 0 {
        return None;
    }

    match mode {
        0 => {
            // Auto Fit (Preserve Aspect Ratio)
            let scale_x = client_w as f32 / screen_w as f32;
            let scale_y = view_h as f32 / screen_h as f32;
            let scale = scale_x.min(scale_y);

            let render_w = (screen_w as f32 * scale) as i32;
            let render_h = (screen_h as f32 * scale) as i32;

            let offset_x = (client_w - render_w) / 2;
            let offset_y = (view_h - render_h) / 2;

            let rel_x = client_x - offset_x;
            let rel_y = view_y - offset_y;

            if rel_x >= 0 && rel_x < render_w && rel_y >= 0 && rel_y < render_h {
                let sx = ((rel_x as f32 / scale) as i32).clamp(0, screen_w - 1);
                let sy = ((rel_y as f32 / scale) as i32).clamp(0, screen_h - 1);
                Some((sx, sy))
            } else {
                None
            }
        }
        1 => {
            // Stretch / Fill Window
            let sx = ((client_x * screen_w) / client_w).clamp(0, screen_w - 1);
            let sy = ((view_y * screen_h) / view_h).clamp(0, screen_h - 1);
            Some((sx, sy))
        }
        _ => {
            // 1:1 Original Resolution
            let offset_x = (client_w - screen_w) / 2;
            let offset_y = (view_h - screen_h) / 2;
            let rel_x = client_x - offset_x;
            let rel_y = view_y - offset_y;
            if rel_x >= 0 && rel_x < screen_w && rel_y >= 0 && rel_y < screen_h {
                Some((rel_x, rel_y))
            } else {
                None
            }
        }
    }
}

pub unsafe fn draw_tab_bar(
    hdc: HDC,
    client_w: i32,
    tab_bar_h: i32,
    tabs: &[SessionTab],
    active_idx: usize,
    hover_state: i32,
) {
    // 1. Clean Tab Bar Background (PK_BG)
    let bar_rect = RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: tab_bar_h,
    };
    let bg_brush = CreateSolidBrush(PK_BG);
    FillRect(hdc, &bar_rect, bg_brush);
    let _ = DeleteObject(HGDIOBJ(bg_brush.0));

    // 2. Bottom Divider Line (PK_CARD_BORDER)
    let border_brush = CreateSolidBrush(PK_CARD_BORDER);
    let border_rect = RECT {
        left: 0,
        top: tab_bar_h - 1,
        right: client_w,
        bottom: tab_bar_h,
    };
    FillRect(hdc, &border_rect, border_brush);

    // 3. Left Brand Cell (DESKY)
    let brand_w = 92;
    let div1 = RECT { left: brand_w, top: 0, right: brand_w + 1, bottom: tab_bar_h };
    FillRect(hdc, &div1, border_brush);

    let font_brand = create_segoe_font(14, FW_BOLD);
    let old_font = SelectObject(hdc, HGDIOBJ(font_brand.0));
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, PK_GREEN_DARK);
    let mut brand_txt: Vec<u16> = "DESKY".encode_utf16().collect();
    let mut brand_rc = RECT { left: 0, top: 0, right: brand_w, bottom: tab_bar_h };
    let _ = DrawTextW(hdc, &mut brand_txt, &mut brand_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    let font_tab = create_segoe_font(12, FW_SEMIBOLD);
    let font_plus = create_segoe_font(16, FW_BOLD);

    let start_x = brand_w + 8;
    let count = tabs.len().max(1) as i32;
    let tab_w = 175.min(((client_w - start_x - 50) / count).max(110));
    let tab_h = 30;
    let tab_y = 5;

    for (i, tab) in tabs.iter().enumerate() {
        let tx = start_x + (i as i32) * (tab_w + 4);
        let is_active = i == active_idx;
        let is_tab_hover = hover_state == i as i32;
        let is_close_hover = hover_state == 1000 + i as i32;

        let tab_bg = if is_active {
            PK_CARD
        } else if is_tab_hover {
            PK_BTN_HOVER
        } else {
            PK_INPUT_BG
        };
        let tab_border = if is_active { PK_GREEN } else { PK_CARD_BORDER };

        // Tab Box
        draw_rounded_box(hdc, tx, tab_y, tab_w, tab_h, tab_bg, tab_border, 6);

        if is_active {
            let accent_brush = CreateSolidBrush(PK_GREEN);
            let accent_rc = RECT {
                left: tx + 6,
                top: tab_y + tab_h - 3,
                right: tx + tab_w - 6,
                bottom: tab_y + tab_h - 1,
            };
            FillRect(hdc, &accent_rc, accent_brush);
            let _ = DeleteObject(HGDIOBJ(accent_brush.0));
        }

        // Status Dot (Vector Lucide Radio/Dot)
        crate::ui::draw_radio(
            hdc,
            tx + 8,
            tab_y + (tab_h - 12) / 2,
            12,
            if is_active { PK_GREEN } else { PK_MUTED },
            2,
        );

        // Tab Title Text
        let _ = SelectObject(hdc, HGDIOBJ(font_tab.0));
        SetTextColor(hdc, if is_active { PK_TEXT } else { PK_MUTED });
        let mut title_txt: Vec<u16> = tab.title.encode_utf16().collect();
        let mut title_rc = RECT { left: tx + 24, top: tab_y, right: tx + tab_w - 24, bottom: tab_y + tab_h };
        let _ = DrawTextW(hdc, &mut title_txt, &mut title_rc, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);

        // Close button (Lucide X vector)
        let close_x = tx + tab_w - 22;
        let close_y = tab_y + 6;
        let close_sz = 18;
        if is_close_hover {
            let ch_brush = CreateSolidBrush(COLORREF(0x00EBF0FF));
            let ch_rc = RECT { left: close_x, top: close_y, right: close_x + close_sz, bottom: close_y + close_sz };
            FillRect(hdc, &ch_rc, ch_brush);
            let _ = DeleteObject(HGDIOBJ(ch_brush.0));
        }
        let close_fg = if is_close_hover { PK_RED } else { PK_MUTED };
        crate::ui::draw_x(hdc, close_x + 3, close_y + 3, 12, close_fg, 2);
    }

    // 4. New Tab '+' Button (Lucide Plus vector)
    let plus_x = start_x + (tabs.len() as i32) * (tab_w + 4);
    let plus_hover = hover_state == 999;
    let plus_bg = if plus_hover { PK_BTN_HOVER } else { PK_INPUT_BG };
    let plus_border = if plus_hover { PK_GREEN } else { PK_CARD_BORDER };
    let plus_fg = if plus_hover { PK_GREEN_DARK } else { PK_MUTED };
    draw_rounded_box(hdc, plus_x, tab_y, 30, tab_h, plus_bg, plus_border, 6);
    crate::ui::draw_plus(hdc, plus_x + 8, tab_y + 8, 14, plus_fg, 2);

    // 5. Mode Switchers: [ 🖥️ Desktop ] and [ ⚡ Admin ] (Lucide Monitor & Terminal vectors)
    let mode_w = 98;
    let mode_y = 5;
    let is_admin = VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed);

    let desk_x = client_w - (mode_w * 2 + 16);
    let admin_x = client_w - (mode_w + 10);

    let desk_hover = hover_state == 9001;
    let admin_hover = hover_state == 9002;

    let (d_bg, d_border, d_fg) = if !is_admin {
        (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
    } else if desk_hover {
        (PK_BTN_HOVER, PK_CARD_BORDER, PK_TEXT)
    } else {
        (PK_CARD, PK_CARD_BORDER, PK_MUTED)
    };
    draw_rounded_box(hdc, desk_x, mode_y, mode_w, tab_h, d_bg, d_border, 6);
    crate::ui::draw_monitor(hdc, desk_x + 8, mode_y + 7, 16, d_fg, 2);
    let _ = SelectObject(hdc, HGDIOBJ(font_tab.0));
    SetTextColor(hdc, d_fg);
    let mut d_txt: Vec<u16> = "Desktop".encode_utf16().collect();
    let mut d_rc = RECT { left: desk_x + 24, top: mode_y, right: desk_x + mode_w, bottom: mode_y + tab_h };
    let _ = DrawTextW(hdc, &mut d_txt, &mut d_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    let (a_bg, a_border, a_fg) = if is_admin {
        (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
    } else if admin_hover {
        (PK_BTN_HOVER, PK_CARD_BORDER, PK_TEXT)
    } else {
        (PK_CARD, PK_CARD_BORDER, PK_MUTED)
    };
    draw_rounded_box(hdc, admin_x, mode_y, mode_w, tab_h, a_bg, a_border, 6);
    crate::ui::draw_terminal(hdc, admin_x + 8, mode_y + 7, 16, a_fg, 2);
    SetTextColor(hdc, a_fg);
    let mut a_txt: Vec<u16> = "Admin".encode_utf16().collect();
    let mut a_rc = RECT { left: admin_x + 22, top: mode_y, right: admin_x + mode_w, bottom: mode_y + tab_h };
    let _ = DrawTextW(hdc, &mut a_txt, &mut a_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(HGDIOBJ(border_brush.0));
    let _ = DeleteObject(HGDIOBJ(font_brand.0));
    let _ = DeleteObject(HGDIOBJ(font_tab.0));
    let _ = DeleteObject(HGDIOBJ(font_plus.0));
}

fn get_tab_hit_state(hwnd: HWND, client_x: i32, client_y: i32) -> i32 {
    let mut rc = RECT::default();
    unsafe { let _ = GetClientRect(hwnd, &mut rc); }
    let client_w = rc.right - rc.left;
    if client_y < 0 || client_y >= TOOLBAR_HEIGHT {
        return -1;
    }
    let tabs = get_session_tabs();
    let brand_w = 92;
    let start_x = brand_w + 8;
    let count = tabs.len().max(1) as i32;
    let tab_w = 175.min(((client_w - start_x - 50) / count).max(110));
    let tab_y = 5;
    let tab_h = 30;

    for (i, _) in tabs.iter().enumerate() {
        let tx = start_x + (i as i32) * (tab_w + 4);
        if client_x >= tx && client_x < tx + tab_w && client_y >= tab_y && client_y < tab_y + tab_h {
            let close_x = tx + tab_w - 22;
            if client_x >= close_x {
                return 1000 + i as i32;
            }
            return i as i32;
        }
    }

    let plus_x = start_x + (tabs.len() as i32) * (tab_w + 4);
    if client_x >= plus_x && client_x < plus_x + 30 && client_y >= tab_y && client_y < tab_y + tab_h {
        return 999;
    }

    // Mode Switchers Hit Testing: [ 🖥️ Desktop ] and [ ⚡ Admin ]
    let mode_w = 98;
    let desk_x = client_w - (mode_w * 2 + 16);
    let admin_x = client_w - (mode_w + 10);
    if client_y >= tab_y && client_y < tab_y + tab_h {
        if client_x >= desk_x && client_x < desk_x + mode_w {
            return 9001;
        }
        if client_x >= admin_x && client_x < admin_x + mode_w {
            return 9002;
        }
    }

    -1
}

fn switch_session_tab(tab_idx: usize) {
    let store = SESSION_TABS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(tabs) = store.lock() {
        if let Some(tab) = tabs.get(tab_idx) {
            ACTIVE_TAB_INDEX.store(tab_idx, Ordering::Relaxed);
            crate::p2p::start_client_session(
                &tab.id,
                TARGET_FPS.load(Ordering::Relaxed),
                CAPTURE_CURSOR.load(Ordering::Relaxed),
            );
        }
    }
}

fn close_session_tab(hwnd: HWND, tab_idx: usize) {
    let store = SESSION_TABS.get_or_init(|| Mutex::new(Vec::new()));
    let remaining = if let Ok(mut tabs) = store.lock() {
        if tab_idx < tabs.len() {
            tabs.remove(tab_idx);
        }
        tabs.clone()
    } else {
        Vec::new()
    };

    if remaining.is_empty() {
        VIEWPORT_RUNNING.store(false, Ordering::SeqCst);
        HWND_VIEWPORT.store(0, Ordering::Relaxed);
        crate::p2p::stop_client_session();
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    } else {
        let cur = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
        let new_idx = if cur >= remaining.len() { remaining.len() - 1 } else { cur };
        ACTIVE_TAB_INDEX.store(new_idx, Ordering::Relaxed);
        switch_session_tab(new_idx);
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

fn detach_tab(tab_idx: usize, drop_screen_x: i32, drop_screen_y: i32) {
    let store = SESSION_TABS.get_or_init(|| Mutex::new(Vec::new()));
    let detached_tab = if let Ok(mut tabs) = store.lock() {
        if tabs.len() > 1 && tab_idx < tabs.len() {
            let tab = tabs.remove(tab_idx);
            let cur_active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
            if cur_active >= tabs.len() {
                ACTIVE_TAB_INDEX.store(tabs.len() - 1, Ordering::Relaxed);
            }
            Some(tab)
        } else {
            None
        }
    } else {
        None
    };

    if let Some(tab) = detached_tab {
        unsafe {
            spawn_detached_viewport_window(&tab.id, drop_screen_x, drop_screen_y);
        }
    }
}

pub fn spawn_or_add_tab_viewport(target: &str) {
    let existing = HWND_VIEWPORT.load(Ordering::Relaxed);
    if existing != 0 && unsafe { IsWindow(Some(HWND(existing as *mut _))).as_bool() } {
        let hwnd = HWND(existing as *mut _);
        let store = SESSION_TABS.get_or_init(|| Mutex::new(Vec::new()));
        let mut idx = 0;
        if let Ok(mut tabs) = store.lock() {
            if let Some(pos) = tabs.iter().position(|t| t.id == target) {
                idx = pos;
            } else {
                tabs.push(SessionTab {
                    id: target.to_string(),
                    title: target.to_string(),
                });
                idx = tabs.len() - 1;
            }
        }
        ACTIVE_TAB_INDEX.store(idx, Ordering::Relaxed);
        let window_title = format!("Desky | [{}]", target);
        let wide_title: Vec<u16> = window_title.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let _ = SetWindowTextW(hwnd, PCWSTR(wide_title.as_ptr()));
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    } else {
        let store = SESSION_TABS.get_or_init(|| Mutex::new(Vec::new()));
        if let Ok(mut tabs) = store.lock() {
            tabs.clear();
            tabs.push(SessionTab {
                id: target.to_string(),
                title: target.to_string(),
            });
        }
        ACTIVE_TAB_INDEX.store(0, Ordering::Relaxed);
        unsafe {
            spawn_viewport_window(target);
        }
    }
}

unsafe fn spawn_detached_viewport_window(target: &str, screen_x: i32, screen_y: i32) {
    let h_module = GetModuleHandleW(None).unwrap_or_default();
    let h_instance: HINSTANCE = h_module.into();

    let window_title = format!("Desky | [{}]", target);
    let wide_title: Vec<u16> = window_title.encode_utf16().chain(std::iter::once(0)).collect();

    let viewport_hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        w!("DeskyViewportClass"),
        PCWSTR(wide_title.as_ptr()),
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        screen_x,
        screen_y,
        1280,
        720,
        None,
        None,
        Some(h_instance),
        None,
    );

    if let Ok(hwnd) = viewport_hwnd {
        let dark_mode: i32 = 0;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_mode as *const _ as *const _,
            std::mem::size_of::<i32>() as u32,
        );
        let _ = AddClipboardFormatListener(hwnd);
        DragAcceptFiles(hwnd, true);
        let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        info!("Detached Remote Viewport Window spawned! (HWND: {:?})", hwnd);

        crate::p2p::start_client_session(
            target,
            TARGET_FPS.load(Ordering::Relaxed),
            CAPTURE_CURSOR.load(Ordering::Relaxed),
        );
    }
}

unsafe fn draw_fullscreen_settings_view(
    hdc: HDC,
    client_w: i32,
    client_h: i32,
    mode: u8,
    fps: u32,
) {
    let scale_w = client_w as f32 / 1024.0;
    let scale_h = client_h as f32 / 640.0;
    let s = (scale_w.min(scale_h)).clamp(1.0, 1.40);

    // 1. Full-Screen Canvas (PK_BG)
    let full_rect = RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: client_h,
    };
    let bg_brush = CreateSolidBrush(PK_BG);
    FillRect(hdc, &full_rect, bg_brush);
    let _ = DeleteObject(HGDIOBJ(bg_brush.0));

    let line_brush = CreateSolidBrush(PK_CARD_BORDER);
    let card_brush = CreateSolidBrush(PK_CARD);
    let green_brush = CreateSolidBrush(PK_GREEN);
    let green_dark_brush = CreateSolidBrush(PK_GREEN_DARK);
    let tint_brush = CreateSolidBrush(PK_GREEN_TINT);
    let input_brush = CreateSolidBrush(PK_INPUT_BG);

    // 2. Integrated Top Header Bar (Adaptive)
    let hdr_h = (46.0 * s) as i32;
    let side_w = (210.0 * s) as i32;

    let hdr_rect = RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: hdr_h,
    };
    FillRect(hdc, &hdr_rect, card_brush);
    let hdr_line = RECT {
        left: 0,
        top: hdr_h - 1,
        right: client_w,
        bottom: hdr_h,
    };
    FillRect(hdc, &hdr_line, line_brush);

    let f_brand = ((16.0 * s) as i32).max(16);
    let f_title = ((17.0 * s) as i32).max(17);
    let f_sec = ((11.0 * s) as i32).max(12);
    let f_btn = ((12.0 * s) as i32).max(13);
    let f_info = ((12.0 * s) as i32).max(12);

    let font_brand = create_segoe_font(f_brand, FW_BOLD);
    let font_title = create_segoe_font(f_title, FW_BOLD);
    let font_sec = create_segoe_font(f_sec, FW_BOLD);
    let font_btn = create_segoe_font(f_btn, FW_SEMIBOLD);
    let font_info = create_segoe_font(f_info, FW_NORMAL);

    SetBkMode(hdc, TRANSPARENT);

    // Brand Label
    let brand_cell_div = RECT {
        left: side_w - 1,
        top: 0,
        right: side_w,
        bottom: hdr_h,
    };
    FillRect(hdc, &brand_cell_div, line_brush);

    let old_font = SelectObject(hdc, HGDIOBJ(font_brand.0));
    SetTextColor(hdc, PK_GREEN_DARK);
    let mut brand_txt: Vec<u16> = "DESKY".encode_utf16().collect();
    let mut brand_rc = RECT {
        left: (18.0 * s) as i32,
        top: 0,
        right: side_w - 10,
        bottom: hdr_h,
    };
    let _ = DrawTextW(hdc, &mut brand_txt, &mut brand_rc, DT_SINGLELINE | DT_VCENTER);

    // Breadcrumb
    let active_cat = SETTINGS_CATEGORY.load(Ordering::Relaxed).min(2);
    let crumb_label = match active_cat {
        0 => "SETTINGS  /  DISPLAY & QUALITY",
        1 => "SETTINGS  /  FRAMERATE & INPUT",
        _ => "SETTINGS  /  CONNECTION & REGION",
    };
    let _ = SelectObject(hdc, HGDIOBJ(font_sec.0));
    SetTextColor(hdc, PK_MUTED);
    let mut crumb_txt: Vec<u16> = crumb_label.encode_utf16().collect();
    let mut crumb_rc = RECT {
        left: side_w + (20.0 * s) as i32,
        top: 0,
        right: client_w - (180.0 * s) as i32,
        bottom: hdr_h,
    };
    let _ = DrawTextW(hdc, &mut crumb_txt, &mut crumb_rc, DT_SINGLELINE | DT_VCENTER);

    // Back to stream button
    let back_w = (160.0 * s) as i32;
    let back_x = (client_w - back_w).max(side_w + 20);
    let back_rc = RECT {
        left: back_x,
        top: 0,
        right: client_w,
        bottom: hdr_h - 1,
    };
    FillRect(hdc, &back_rc, green_brush);
    let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
    SetTextColor(hdc, COLORREF(0x00FFFFFF));
    let mut back_txt: Vec<u16> = "< BACK TO STREAM".encode_utf16().collect();
    let mut back_txt_rc = back_rc;
    let _ = DrawTextW(
        hdc,
        &mut back_txt,
        &mut back_txt_rc,
        DT_SINGLELINE | DT_CENTER | DT_VCENTER,
    );

    // Left Navigation Dock
    let side_rc = RECT {
        left: 0,
        top: hdr_h,
        right: side_w,
        bottom: client_h,
    };
    FillRect(hdc, &side_rc, card_brush);
    let side_border = RECT {
        left: side_w - 1,
        top: hdr_h,
        right: side_w,
        bottom: client_h,
    };
    FillRect(hdc, &side_border, line_brush);

    let nav_hdr_h = (32.0 * s) as i32;
    let nav_hdr_bg = RECT {
        left: 0,
        top: hdr_h,
        right: side_w - 1,
        bottom: hdr_h + nav_hdr_h,
    };
    FillRect(hdc, &nav_hdr_bg, input_brush);
    let nav_hdr_line = RECT {
        left: 0,
        top: hdr_h + nav_hdr_h - 1,
        right: side_w,
        bottom: hdr_h + nav_hdr_h,
    };
    FillRect(hdc, &nav_hdr_line, line_brush);

    let _ = SelectObject(hdc, HGDIOBJ(font_sec.0));
    SetTextColor(hdc, PK_MUTED);
    let mut nav_hdr: Vec<u16> = "CATEGORIES".encode_utf16().collect();
    let mut nav_hdr_rc = RECT {
        left: (18.0 * s) as i32,
        top: hdr_h,
        right: side_w - 12,
        bottom: hdr_h + nav_hdr_h,
    };
    let _ = DrawTextW(hdc, &mut nav_hdr, &mut nav_hdr_rc, DT_SINGLELINE | DT_VCENTER);

    let categories = [
        ("01", "DISPLAY & QUALITY"),
        ("02", "FRAMERATE & INPUT"),
        ("03", "CONNECTION & REGION"),
    ];

    let tab_start_y = hdr_h + nav_hdr_h;
    let tab_h = (44.0 * s) as i32;

    for (i, (num, label)) in categories.iter().enumerate() {
        let item_y = tab_start_y + (i as i32) * tab_h;
        let is_sel = active_cat == i as u8;
        let item_rc = RECT {
            left: 0,
            top: item_y,
            right: side_w - 1,
            bottom: item_y + tab_h,
        };

        if is_sel {
            FillRect(hdc, &item_rc, tint_brush);
            let bar_rc = RECT {
                left: 0,
                top: item_y,
                right: 4,
                bottom: item_y + tab_h,
            };
            FillRect(hdc, &bar_rc, green_brush);
        } else {
            FillRect(hdc, &item_rc, card_brush);
        }

        let row_div = RECT {
            left: 0,
            top: item_y + tab_h - 1,
            right: side_w,
            bottom: item_y + tab_h,
        };
        FillRect(hdc, &row_div, line_brush);

        let _ = SelectObject(hdc, HGDIOBJ(font_sec.0));
        SetTextColor(hdc, if is_sel { PK_GREEN } else { PK_MUTED });
        let mut num_wide: Vec<u16> = num.encode_utf16().collect();
        let mut num_rc = RECT {
            left: (16.0 * s) as i32,
            top: item_y,
            right: (38.0 * s) as i32,
            bottom: item_y + tab_h,
        };
        let _ = DrawTextW(hdc, &mut num_wide, &mut num_rc, DT_SINGLELINE | DT_VCENTER);

        let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
        SetTextColor(hdc, if is_sel { PK_GREEN_DARK } else { PK_TEXT });
        let mut lbl_wide: Vec<u16> = label.encode_utf16().collect();
        let mut lbl_rc = RECT {
            left: (42.0 * s) as i32,
            top: item_y,
            right: side_w - 8,
            bottom: item_y + tab_h,
        };
        let _ = DrawTextW(hdc, &mut lbl_wide, &mut lbl_rc, DT_SINGLELINE | DT_VCENTER);
    }

    // Telemetry Dock (Sidebar bottom)
    let (_active_q, active_scale, _active_c444) = crate::p2p::get_active_quality_params();
    let bitrate_str = format_bitrate(crate::p2p::get_session_bitrate_kbps());
    let rtt_ms = crate::p2p::get_session_rtt_ms();
    let orig_w = REMOTE_ORIG_W.load(Ordering::Relaxed);
    let orig_h = REMOTE_ORIG_H.load(Ordering::Relaxed);
    let res_str = if orig_w > 0 && orig_h > 0 {
        format!("{}x{} ({}%)", orig_w, orig_h, active_scale)
    } else {
        format!("{}% Native", active_scale)
    };

    let tele_h = (150.0 * s) as i32;
    let tele_y = (client_h - tele_h).max(tab_start_y + 3 * tab_h + 10);
    let tele_top_line = RECT {
        left: 0,
        top: tele_y,
        right: side_w,
        bottom: tele_y + 1,
    };
    FillRect(hdc, &tele_top_line, line_brush);

    let tele_hdr_h = (30.0 * s) as i32;
    let tele_hdr_bg = RECT {
        left: 0,
        top: tele_y + 1,
        right: side_w - 1,
        bottom: tele_y + tele_hdr_h,
    };
    FillRect(hdc, &tele_hdr_bg, tint_brush);
    let tele_hdr_line = RECT {
        left: 0,
        top: tele_y + tele_hdr_h,
        right: side_w,
        bottom: tele_y + tele_hdr_h + 1,
    };
    FillRect(hdc, &tele_hdr_line, line_brush);

    let _ = SelectObject(hdc, HGDIOBJ(font_sec.0));
    SetTextColor(hdc, PK_GREEN_DARK);
    let mut tele_hdr: Vec<u16> = "LIVE STATUS".encode_utf16().collect();
    let mut tele_hdr_rc = RECT {
        left: (16.0 * s) as i32,
        top: tele_y + 1,
        right: side_w - 12,
        bottom: tele_y + tele_hdr_h,
    };
    let _ = DrawTextW(hdc, &mut tele_hdr, &mut tele_hdr_rc, DT_SINGLELINE | DT_VCENTER);

    let stats_rows = [
        ("REGION", "Istanbul (TR)"),
        ("LATENCY", &format!("{} ms RTT", rtt_ms)),
        ("BITRATE", bitrate_str.as_str()),
        ("STREAM", res_str.as_str()),
    ];
    let _ = SelectObject(hdc, HGDIOBJ(font_info.0));
    let row_step = (24.0 * s) as i32;
    for (idx, (k, v)) in stats_rows.iter().enumerate() {
        let ry = tele_y + tele_hdr_h + 6 + (idx as i32) * row_step;
        SetTextColor(hdc, PK_MUTED);
        let mut kw: Vec<u16> = k.encode_utf16().collect();
        let mut krc = RECT { left: (16.0 * s) as i32, top: ry, right: (82.0 * s) as i32, bottom: ry + row_step };
        let _ = DrawTextW(hdc, &mut kw, &mut krc, DT_SINGLELINE | DT_VCENTER);

        SetTextColor(hdc, PK_TEXT);
        let mut vw: Vec<u16> = v.encode_utf16().collect();
        let mut vrc = RECT { left: (84.0 * s) as i32, top: ry, right: side_w - 8, bottom: ry + row_step };
        let _ = DrawTextW(hdc, &mut vw, &mut vrc, DT_SINGLELINE | DT_VCENTER);
    }

    // Right Workspace Area
    let work_x = side_w + (24.0 * s) as i32;
    let work_w = (client_w - work_x - (24.0 * s) as i32).clamp(360, 960);
    let work_y = hdr_h + (20.0 * s) as i32;

    let mod_hdr_h = (32.0 * s) as i32;
    let badge_w = (84.0 * s) as i32;

    // Helper: Draws module panel
    let draw_module_panel = |top: i32, height: i32, badge: &str, title: &str| {
        draw_rounded_box(hdc, work_x, top, work_w, height, PK_CARD, PK_CARD_BORDER, 0);
        let h_bg = RECT {
            left: work_x + 1,
            top: top + 1,
            right: work_x + work_w - 1,
            bottom: top + mod_hdr_h,
        };
        FillRect(hdc, &h_bg, input_brush);
        let h_line = RECT {
            left: work_x,
            top: top + mod_hdr_h,
            right: work_x + work_w,
            bottom: top + mod_hdr_h + 1,
        };
        FillRect(hdc, &h_line, line_brush);

        let b_rc = RECT {
            left: work_x + 1,
            top: top + 1,
            right: work_x + badge_w,
            bottom: top + mod_hdr_h,
        };
        FillRect(hdc, &b_rc, tint_brush);
        let b_div = RECT {
            left: work_x + badge_w,
            top: top + 1,
            right: work_x + badge_w + 1,
            bottom: top + mod_hdr_h,
        };
        FillRect(hdc, &b_div, line_brush);

        let _ = SelectObject(hdc, HGDIOBJ(font_sec.0));
        SetTextColor(hdc, PK_GREEN_DARK);
        let mut bw: Vec<u16> = badge.encode_utf16().collect();
        let mut b_txt_rc = b_rc;
        let _ = DrawTextW(hdc, &mut bw, &mut b_txt_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

        SetTextColor(hdc, PK_TEXT);
        let mut tw: Vec<u16> = title.encode_utf16().collect();
        let mut t_rc = RECT {
            left: work_x + badge_w + 12,
            top: top + 1,
            right: work_x + work_w - 12,
            bottom: top + mod_hdr_h,
        };
        let _ = DrawTextW(hdc, &mut tw, &mut t_rc, DT_SINGLELINE | DT_VCENTER);
    };

    // Helper: Draws segmented button strip
    let draw_segmented_strip = |x: i32, y: i32, w: i32, h: i32, labels: &[&str], active_idx: usize| {
        let count = labels.len().max(1) as i32;
        let seg_w = w / count;

        draw_rounded_box(hdc, x, y, w, h, PK_INPUT_BG, PK_CARD_BORDER, 0);

        let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
        for (i, label) in labels.iter().enumerate() {
            let sx = x + (i as i32) * seg_w;
            let sr = if i as i32 == count - 1 { x + w } else { sx + seg_w };
            let is_active = i == active_idx;

            let cell_rc = RECT {
                left: sx + 1,
                top: y + 1,
                right: sr - 1,
                bottom: y + h - 1,
            };
            if is_active {
                FillRect(hdc, &cell_rc, green_brush);
                let top_accent = RECT {
                    left: sx + 1,
                    top: y + 1,
                    right: sr - 1,
                    bottom: y + 3,
                };
                FillRect(hdc, &top_accent, green_dark_brush);
                SetTextColor(hdc, COLORREF(0x00FFFFFF));
            } else {
                FillRect(hdc, &cell_rc, input_brush);
                SetTextColor(hdc, PK_TEXT);
            }

            if (i as i32) < count - 1 {
                let div_rc = RECT {
                    left: sr - 1,
                    top: y,
                    right: sr,
                    bottom: y + h,
                };
                FillRect(hdc, &div_rc, line_brush);
            }

            let mut lw: Vec<u16> = label.encode_utf16().collect();
            let mut lrc = cell_rc;
            let _ = DrawTextW(hdc, &mut lw, &mut lrc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);
        }
    };

    let content_y = work_y + (44.0 * s) as i32;
    let strip_x = work_x + (16.0 * s) as i32;
    let strip_w = work_w - (32.0 * s) as i32;
    let strip_h = (38.0 * s) as i32;

    let m1_y = content_y;
    let m1_h = (94.0 * s) as i32;
    let m2_y = m1_y + m1_h + (16.0 * s) as i32;
    let m2_h = (94.0 * s) as i32;

    let q_mode = crate::p2p::get_quality_mode() as usize;

    match active_cat {
        0 => {
            // Module 1: Scaling Mode
            draw_module_panel(m1_y, m1_h, "SCALING", "VIEWPORT SCALING MODE");
            let scale_labels = ["AUTO FIT", "STRETCH", "ORIGINAL 1:1"];
            let strip1_y = m1_y + mod_hdr_h + (12.0 * s) as i32;
            draw_segmented_strip(strip_x, strip1_y, strip_w, strip_h, &scale_labels, (mode as usize).min(2));

            // Module 2: Image Quality
            draw_module_panel(m2_y, m2_h, "QUALITY", "STREAM QUALITY PRESET");
            let q_labels = ["AUTO (ABR)", "HIGH (4:4:4)", "BALANCED", "LOW DATA"];
            let q_idx = match q_mode {
                0 => 0,
                1 | 2 => 1,
                3 => 2,
                _ => 3,
            };
            let strip2_y = m2_y + mod_hdr_h + (12.0 * s) as i32;
            draw_segmented_strip(strip_x, strip2_y, strip_w, strip_h, &q_labels, q_idx);
        }

        1 => {
            // Module 1: Framerate
            draw_module_panel(m1_y, m1_h, "FRAMERATE", "TARGET STREAM FRAMERATE");
            let fps_labels = ["30 FPS", "60 FPS", "120 FPS"];
            let fps_idx = match fps {
                30 => 0,
                120 => 2,
                _ => 1,
            };
            let strip1_y = m1_y + mod_hdr_h + (12.0 * s) as i32;
            draw_segmented_strip(strip_x, strip1_y, strip_w, strip_h, &fps_labels, fps_idx);

            // Module 2: Remote Cursor
            draw_module_panel(m2_y, m2_h, "CURSOR", "REMOTE CURSOR DISPLAY");
            let cur_labels = ["SHOW CURSOR", "HIDE CURSOR"];
            let cursor_on = CAPTURE_CURSOR.load(Ordering::Relaxed);
            let strip2_y = m2_y + mod_hdr_h + (12.0 * s) as i32;
            draw_segmented_strip(strip_x, strip2_y, strip_w, strip_h, &cur_labels, if cursor_on { 0 } else { 1 });
        }

        _ => {
            // Module 1: Connection & Region Information
            let conn_h = (220.0 * s) as i32;
            draw_module_panel(m1_y, conn_h, "CONNECTION", "SESSION & REGION DETAILS");

            let rows = [
                ("SERVER REGION", "Istanbul (TR)"),
                ("LATENCY (RTT)", &format!("{} ms", rtt_ms)),
                ("STREAM BITRATE", bitrate_str.as_str()),
                ("REMOTE RESOLUTION", res_str.as_str()),
                ("CONNECTION TYPE", "Direct P2P UDP"),
                ("ENCRYPTION", "ChaCha20-Poly1305 E2EE"),
            ];

            let row_h = (26.0 * s) as i32;
            for (i, (k, v)) in rows.iter().enumerate() {
                let ry = m1_y + mod_hdr_h + 8 + (i as i32) * (row_h + 3);
                let row_bg = if i % 2 == 0 { PK_INPUT_BG } else { PK_CARD };
                draw_rounded_box(hdc, strip_x, ry, strip_w, row_h, row_bg, PK_CARD_BORDER, 0);

                let _ = SelectObject(hdc, HGDIOBJ(font_sec.0));
                SetTextColor(hdc, PK_MUTED);
                let mut kw: Vec<u16> = k.encode_utf16().collect();
                let mut k_rc = RECT {
                    left: strip_x + 12,
                    top: ry,
                    right: strip_x + (180.0 * s) as i32,
                    bottom: ry + row_h,
                };
                let _ = DrawTextW(hdc, &mut kw, &mut k_rc, DT_SINGLELINE | DT_VCENTER);

                let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
                SetTextColor(hdc, PK_GREEN_DARK);
                let mut vw: Vec<u16> = v.encode_utf16().collect();
                let mut v_rc = RECT {
                    left: strip_x + (190.0 * s) as i32,
                    top: ry,
                    right: strip_x + strip_w - 12,
                    bottom: ry + row_h,
                };
                let _ = DrawTextW(hdc, &mut vw, &mut v_rc, DT_SINGLELINE | DT_VCENTER);
            }
        }
    }

    // Bottom Action Bar
    let footer_h = (40.0 * s) as i32;
    let footer_y = (client_h - footer_h - (16.0 * s) as i32).max(m2_y + m2_h + (16.0 * s) as i32);
    draw_rounded_box(hdc, work_x, footer_y, work_w, footer_h, PK_CARD, PK_CARD_BORDER, 0);

    let apply_w = (220.0 * s) as i32;
    let apply_x = work_x + work_w - apply_w;
    let apply_rc = RECT {
        left: apply_x,
        top: footer_y + 1,
        right: work_x + work_w - 1,
        bottom: footer_y + footer_h - 1,
    };
    FillRect(hdc, &apply_rc, green_brush);

    let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
    SetTextColor(hdc, COLORREF(0x00FFFFFF));
    let mut done_txt: Vec<u16> = "APPLY & CLOSE".encode_utf16().collect();
    let mut done_rc = apply_rc;
    let _ = DrawTextW(hdc, &mut done_txt, &mut done_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    let _ = SelectObject(hdc, HGDIOBJ(font_info.0));
    SetTextColor(hdc, PK_MUTED);
    let mut hint_txt: Vec<u16> = "Press [Tab] or [Esc] to return to stream".encode_utf16().collect();
    let mut hint_rc = RECT {
        left: work_x + 14,
        top: footer_y,
        right: apply_x - 10,
        bottom: footer_y + footer_h,
    };
    let _ = DrawTextW(hdc, &mut hint_txt, &mut hint_rc, DT_SINGLELINE | DT_VCENTER);

    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(HGDIOBJ(line_brush.0));
    let _ = DeleteObject(HGDIOBJ(green_brush.0));
    let _ = DeleteObject(HGDIOBJ(green_dark_brush.0));
    let _ = DeleteObject(HGDIOBJ(tint_brush.0));
    let _ = DeleteObject(HGDIOBJ(input_brush.0));
    let _ = DeleteObject(HGDIOBJ(font_brand.0));
    let _ = DeleteObject(HGDIOBJ(font_title.0));
    let _ = DeleteObject(HGDIOBJ(font_sec.0));
    let _ = DeleteObject(HGDIOBJ(font_btn.0));
    let _ = DeleteObject(HGDIOBJ(font_info.0));
}

unsafe fn spawn_viewport_window(target: &str) {
    let h_module = GetModuleHandleW(None).unwrap_or_default();
    let h_instance: HINSTANCE = h_module.into();

    let window_title = format!("Desky Remote Viewport [Session: {}]", target);
    let wide_title: Vec<u16> = window_title.encode_utf16().chain(std::iter::once(0)).collect();

    let viewport_hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        w!("DeskyViewportClass"),
        PCWSTR(wide_title.as_ptr()),
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        1280,
        720,
        None,
        None,
        Some(h_instance),
        None,
    );

    if let Ok(hwnd) = viewport_hwnd {
        let dark_mode: i32 = 0;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_mode as *const _ as *const _,
            std::mem::size_of::<i32>() as u32,
        );
        let _ = AddClipboardFormatListener(hwnd);
        DragAcceptFiles(hwnd, true);

        let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        let _ = ShowWindow(hwnd, SW_SHOW);
        info!("Remote Viewport Window successfully spawned! (HWND: {:?})", hwnd);
        HWND_VIEWPORT.store(hwnd.0 as isize, Ordering::Relaxed);

        REMOTE_ORIG_W.store(0, Ordering::Relaxed);
        REMOTE_ORIG_H.store(0, Ordering::Relaxed);

        VIEWPORT_RUNNING.store(true, Ordering::SeqCst);
        let hwnd_raw = hwnd.0 as isize;

        std::thread::spawn(move || {
            info!("Dedicated Viewport Render Thread Started.");
            let target_hwnd = HWND(hwnd_raw as *mut _);

            let mut last_rendered_frame_id: u32 = 0;
            let mut last_w: i32 = 0;
            let mut last_h: i32 = 0;
            let mut last_mode: u8 = 255;
            let mut last_q_mode: u8 = 255;
            let mut last_fps: u32 = 0;
            let mut last_cur: bool = true;
            let mut last_toolbar: bool = true;
            let mut last_modal: bool = false;
            let mut last_cat: u8 = 255;
            let mut last_ui_refresh = Instant::now();
            let mut last_frame_change_time = Instant::now();
            let mut sharp_pass_done: bool = false;

            while VIEWPORT_RUNNING.load(Ordering::Relaxed) {
                unsafe {
                    if !IsWindow(Some(target_hwnd)).as_bool() {
                        break;
                    }

                    let mut rect = RECT::default();
                    let _ = GetClientRect(target_hwnd, &mut rect);
                    let client_w = rect.right - rect.left;
                    let client_h = rect.bottom - rect.top;

                    let toolbar_visible = SHOW_TOOLBAR.load(Ordering::Relaxed);
                    let modal_open = SHOW_SETTINGS_MODAL.load(Ordering::Relaxed);
                    let active_cat = SETTINGS_CATEGORY.load(Ordering::Relaxed);
                    let mode = DISPLAY_MODE.load(Ordering::Relaxed);
                    let q_mode = crate::p2p::get_quality_mode();
                    let target_fps = TARGET_FPS.load(Ordering::Relaxed).clamp(15, 240);
                    let cap_cur = CAPTURE_CURSOR.load(Ordering::Relaxed);

                    let latest_frame = crate::p2p::get_latest_remote_frame();
                    let cur_frame_id = latest_frame.as_ref().map(|f| f.frame_id).unwrap_or(0);
                    let frame_changed = cur_frame_id != last_rendered_frame_id;
                    if frame_changed {
                        last_frame_change_time = Instant::now();
                        sharp_pass_done = false;
                    }

                    let needs_sharp_pass = !modal_open
                        && cur_frame_id > 0
                        && !sharp_pass_done
                        && last_frame_change_time.elapsed() >= Duration::from_millis(140);

                    let state_changed = frame_changed
                        || needs_sharp_pass
                        || client_w != last_w
                        || client_h != last_h
                        || mode != last_mode
                        || q_mode != last_q_mode
                        || target_fps != last_fps
                        || cap_cur != last_cur
                        || toolbar_visible != last_toolbar
                        || modal_open != last_modal
                        || (modal_open && active_cat != last_cat)
                        || cur_frame_id == 0
                        || last_ui_refresh.elapsed() >= Duration::from_millis(400);

                    if !state_changed {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }

                    last_rendered_frame_id = cur_frame_id;
                    last_w = client_w;
                    last_h = client_h;
                    last_mode = mode;
                    last_q_mode = q_mode;
                    last_fps = target_fps;
                    last_cur = cap_cur;
                    last_toolbar = toolbar_visible;
                    last_modal = modal_open;
                    last_cat = active_cat;
                    last_ui_refresh = Instant::now();

                    let toolbar_h = if toolbar_visible { TOOLBAR_HEIGHT } else { 0 };

                    if client_w > 50 && client_h > 50 {
                        let hdc_window = GetDC(Some(target_hwnd));
                        let mem_dc = CreateCompatibleDC(Some(hdc_window));
                        let mem_bmp = CreateCompatibleBitmap(hdc_window, client_w, client_h);
                        let old_bmp = SelectObject(mem_dc, HGDIOBJ(mem_bmp.0));

                        if modal_open {
                            draw_fullscreen_settings_view(mem_dc, client_w, client_h, mode, target_fps);
                        } else if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                            let hover_admin = ADMIN_HOVER_STATE.load(Ordering::Relaxed);
                            super::admin::draw_admin_view(mem_dc, client_w, client_h, hover_admin);
                            let tabs = get_session_tabs();
                            let active_idx = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
                            let hover_state = TAB_HOVER_STATE.load(Ordering::Relaxed);
                            draw_tab_bar(mem_dc, client_w, TOOLBAR_HEIGHT, &tabs, active_idx, hover_state);
                        } else {
                            // Fast hardware-friendly COLORONCOLOR during motion (60 FPS smooth),
                            // then a single HALFTONE pass once static for 140 ms for razor-sharp text.
                            if needs_sharp_pass {
                                let _ = SetStretchBltMode(mem_dc, HALFTONE);
                                let _ = SetBrushOrgEx(mem_dc, 0, 0, None);
                                sharp_pass_done = true;
                            } else {
                                let _ = SetStretchBltMode(mem_dc, COLORONCOLOR);
                            }
                            let view_h = client_h - toolbar_h;

                            if let Some(remote_frame) = latest_frame {
                                REMOTE_ORIG_W.store(remote_frame.orig_width, Ordering::Relaxed);
                                REMOTE_ORIG_H.store(remote_frame.orig_height, Ordering::Relaxed);

                                let frame_w = remote_frame.width as i32;
                                let frame_h = remote_frame.height as i32;

                                let bmi = BITMAPINFO {
                                    bmiHeader: BITMAPINFOHEADER {
                                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                                        biWidth: frame_w,
                                        biHeight: -frame_h, // Top-down BGRA
                                        biPlanes: 1,
                                        biBitCount: 32,
                                        biCompression: BI_RGB.0,
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                };

                                match mode {
                                    0 => {
                                        // Auto Fit (Preserve Aspect Ratio)
                                        let scale_x = client_w as f32 / frame_w as f32;
                                        let scale_y = view_h as f32 / frame_h as f32;
                                        let scale = scale_x.min(scale_y);

                                        let render_w = (frame_w as f32 * scale) as i32;
                                        let render_h = (frame_h as f32 * scale) as i32;

                                        let offset_x = (client_w - render_w) / 2;
                                        let offset_y = toolbar_h + (view_h - render_h) / 2;

                                        if offset_x > 0 {
                                            let left_bar = RECT { left: 0, top: toolbar_h, right: offset_x, bottom: client_h };
                                            FillRect(mem_dc, &left_bar, VIEWPORT_BRUSH);
                                            let right_bar = RECT { left: offset_x + render_w, top: toolbar_h, right: client_w, bottom: client_h };
                                            FillRect(mem_dc, &right_bar, VIEWPORT_BRUSH);
                                        }
                                        if (view_h - render_h) > 0 {
                                            let top_bar = RECT { left: 0, top: toolbar_h, right: client_w, bottom: offset_y };
                                            FillRect(mem_dc, &top_bar, VIEWPORT_BRUSH);
                                            let bottom_bar = RECT { left: 0, top: offset_y + render_h, right: client_w, bottom: client_h };
                                            FillRect(mem_dc, &bottom_bar, VIEWPORT_BRUSH);
                                        }

                                        let _ = StretchDIBits(
                                            mem_dc,
                                            offset_x,
                                            offset_y,
                                            render_w,
                                            render_h,
                                            0,
                                            0,
                                            frame_w,
                                            frame_h,
                                            Some(remote_frame.bgra.as_ptr() as *const _),
                                            &bmi,
                                            DIB_RGB_COLORS,
                                            SRCCOPY,
                                        );
                                    }
                                    1 => {
                                        // Stretch / Fill Window
                                        let _ = StretchDIBits(
                                            mem_dc,
                                            0,
                                            toolbar_h,
                                            client_w,
                                            view_h,
                                            0,
                                            0,
                                            frame_w,
                                            frame_h,
                                            Some(remote_frame.bgra.as_ptr() as *const _),
                                            &bmi,
                                            DIB_RGB_COLORS,
                                            SRCCOPY,
                                        );
                                    }
                                    _ => {
                                        // 1:1 Original Resolution (Centered)
                                        let orig_w = remote_frame.orig_width as i32;
                                        let orig_h = remote_frame.orig_height as i32;
                                        let offset_x = (client_w - orig_w) / 2;
                                        let offset_y = toolbar_h + (view_h - orig_h) / 2;

                                        let bg_rect = RECT { left: 0, top: toolbar_h, right: client_w, bottom: client_h };
                                        FillRect(mem_dc, &bg_rect, VIEWPORT_BRUSH);

                                        let _ = StretchDIBits(
                                            mem_dc,
                                            offset_x,
                                            offset_y,
                                            orig_w,
                                            orig_h,
                                            0,
                                            0,
                                            frame_w,
                                            frame_h,
                                            Some(remote_frame.bgra.as_ptr() as *const _),
                                            &bmi,
                                            DIB_RGB_COLORS,
                                            SRCCOPY,
                                        );
                                    }
                                }
                            } else {
                                // Waiting for remote host stream, host approval, or connection error
                                let bg_rect = RECT { left: 0, top: toolbar_h, right: client_w, bottom: client_h };
                                FillRect(mem_dc, &bg_rect, VIEWPORT_BRUSH);

                                let status = crate::p2p::get_session_status();
                                let (status_msg, color) = match status {
                                    SessionStatus::Connecting(id) => (
                                        format!("Connecting to Remote Desk {} ...", id),
                                        COLORREF(0x00EDB363),
                                    ),
                                    SessionStatus::WaitingApproval(id) => (
                                        format!("Waiting for Remote Desk {} to accept your connection request...", id),
                                        COLORREF(0x0078BB48),
                                    ),
                                    SessionStatus::Error(err) => (
                                        format!("{} (Press Esc to close)", err),
                                        COLORREF(0x006060FF),
                                    ),
                                    _ => (
                                        "Waiting for remote video stream...".to_string(),
                                        COLORREF(0x00A0AEC0),
                                    ),
                                };

                                let status_font = create_segoe_font(22, FW_SEMIBOLD);
                                let old_f = SelectObject(mem_dc, HGDIOBJ(status_font.0));
                                SetBkMode(mem_dc, TRANSPARENT);
                                SetTextColor(mem_dc, color);
                                let mut status_wide: Vec<u16> = status_msg.encode_utf16().collect();
                                let mut status_rect = RECT {
                                    left: 20,
                                    top: toolbar_h,
                                    right: client_w - 20,
                                    bottom: client_h,
                                };
                                let _ = DrawTextW(
                                    mem_dc,
                                    &mut status_wide,
                                    &mut status_rect,
                                    DT_SINGLELINE | DT_CENTER | DT_VCENTER,
                                );
                                let _ = SelectObject(mem_dc, old_f);
                                let _ = DeleteObject(HGDIOBJ(status_font.0));
                            }

                            let tabs = get_session_tabs();
                            let active_idx = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
                            let hover_state = TAB_HOVER_STATE.load(Ordering::Relaxed);
                            draw_tab_bar(mem_dc, client_w, TOOLBAR_HEIGHT, &tabs, active_idx, hover_state);
                    }

                        let _ = BitBlt(hdc_window, 0, 0, client_w, client_h, Some(mem_dc), 0, 0, SRCCOPY);

                        let _ = SelectObject(mem_dc, old_bmp);
                        let _ = DeleteObject(HGDIOBJ(mem_bmp.0));
                        let _ = DeleteObject(HGDIOBJ(mem_dc.0));
                        let _ = ReleaseDC(Some(target_hwnd), hdc_window);
                    }
                }
            }
            crate::p2p::stop_client_session();
            info!("Dedicated Viewport Render Thread Exited.");
        });
    } else {
        tracing::error!("Failed to spawn viewport window: {:?}", viewport_hwnd.err());
    }
}

unsafe fn inject_remote_mouse(hwnd: HWND, client_x: i32, client_y: i32, is_down: bool, is_right: bool) {
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let client_w = rect.right - rect.left;
    let client_h = rect.bottom - rect.top;

    let screen_w = REMOTE_ORIG_W.load(Ordering::Relaxed) as i32;
    let screen_h = REMOTE_ORIG_H.load(Ordering::Relaxed) as i32;

    if client_w > 0 && client_h > 0 && screen_w > 0 && screen_h > 0 {
        let toolbar_h = if SHOW_TOOLBAR.load(Ordering::Relaxed) { TOOLBAR_HEIGHT } else { 0 };
        let mode = DISPLAY_MODE.load(Ordering::Relaxed);

        if let Some((dest_x, dest_y)) = map_client_to_screen(
            client_x, client_y, client_w, client_h, screen_w, screen_h, toolbar_h, mode,
        ) {
            crate::p2p::send_remote_input(InputEvent::MouseMove { x: dest_x, y: dest_y });
            let button = if is_right { MouseButton::Right } else { MouseButton::Left };
            let ev = if is_down {
                InputEvent::MouseDown { button }
            } else {
                InputEvent::MouseUp { button }
            };
            crate::p2p::send_remote_input(ev);
        }
    }
}

unsafe fn inject_remote_mouse_move(hwnd: HWND, client_x: i32, client_y: i32) {
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let client_w = rect.right - rect.left;
    let client_h = rect.bottom - rect.top;

    let screen_w = REMOTE_ORIG_W.load(Ordering::Relaxed) as i32;
    let screen_h = REMOTE_ORIG_H.load(Ordering::Relaxed) as i32;

    if client_w > 0 && client_h > 0 && screen_w > 0 && screen_h > 0 {
        let toolbar_h = if SHOW_TOOLBAR.load(Ordering::Relaxed) { TOOLBAR_HEIGHT } else { 0 };
        let mode = DISPLAY_MODE.load(Ordering::Relaxed);

        if let Some((dest_x, dest_y)) = map_client_to_screen(
            client_x, client_y, client_w, client_h, screen_w, screen_h, toolbar_h, mode,
        ) {
            crate::p2p::send_remote_input(InputEvent::MouseMove { x: dest_x, y: dest_y });
        }
    }
}

unsafe fn inject_remote_mouse_middle(hwnd: HWND, client_x: i32, client_y: i32, is_down: bool) {
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let client_w = rect.right - rect.left;
    let client_h = rect.bottom - rect.top;

    let screen_w = REMOTE_ORIG_W.load(Ordering::Relaxed) as i32;
    let screen_h = REMOTE_ORIG_H.load(Ordering::Relaxed) as i32;

    if client_w > 0 && client_h > 0 && screen_w > 0 && screen_h > 0 {
        let toolbar_h = if SHOW_TOOLBAR.load(Ordering::Relaxed) { TOOLBAR_HEIGHT } else { 0 };
        let mode = DISPLAY_MODE.load(Ordering::Relaxed);

        if let Some((dest_x, dest_y)) = map_client_to_screen(
            client_x, client_y, client_w, client_h, screen_w, screen_h, toolbar_h, mode,
        ) {
            crate::p2p::send_remote_input(InputEvent::MouseMove { x: dest_x, y: dest_y });
            let ev = if is_down {
                InputEvent::MouseDown { button: MouseButton::Middle }
            } else {
                InputEvent::MouseUp { button: MouseButton::Middle }
            };
            crate::p2p::send_remote_input(ev);
        }
    }
}

unsafe fn inject_remote_key(vk: u8, is_up: bool) {
    let ev = if is_up {
        InputEvent::KeyUp { key_code: vk as u32 }
    } else {
        InputEvent::KeyDown { key_code: vk as u32 }
    };
    crate::p2p::send_remote_input(ev);
}

pub(crate) unsafe extern "system" fn viewport_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
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
        WM_CLIPBOARDUPDATE => {
            if !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                let files = crate::p2p::read_clipboard_files();
                if !files.is_empty() {
                    let desc = if files.len() == 1 {
                        files[0]
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "1 file".into())
                    } else {
                        format!("{} files", files.len())
                    };
                    info!("📋 Clipboard contains file: '{}' (Press Ctrl+V in viewport to paste/transfer)", desc);
                    super::launcher::append_status_log(&format!(
                        "📋 Copied '{}' to clipboard (Ctrl+V in viewport to transfer)",
                        desc
                    ));
                } else if let Some(text) = crate::p2p::read_clipboard_text() {
                    if !crate::platform::is_clipboard_recently_synced(&text) {
                        crate::platform::mark_clipboard_synced(&text);
                        crate::p2p::send_remote_clipboard(&text);
                    }
                }
            }
            LRESULT(0)
        }
        WM_DROPFILES => {
            unsafe {
                let hdrop = HDROP(wparam.0 as *mut _);
                let count = DragQueryFileW(hdrop, 0xFFFFFFFF, None);
                for i in 0..count {
                    let mut buf = [0u16; 512];
                    let len = DragQueryFileW(hdrop, i, Some(&mut buf));
                    if len > 0 {
                        let path_str = String::from_utf16_lossy(&buf[..len as usize]);
                        let path = std::path::PathBuf::from(path_str);
                        if path.is_file() {
                            info!("Dragging & dropping file for remote transfer: {:?}", path);
                            crate::p2p::send_file_transfer(path);
                        }
                    }
                }
                DragFinish(hdrop);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_LBUTTONDOWN => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            // Viewport dimensions

            let mut rect = RECT::default();
            let _ = unsafe { GetClientRect(hwnd, &mut rect) };
            let client_w = rect.right - rect.left;
            let client_h = rect.bottom - rect.top;

                        // 1. If Full-Screen Settings Page is open, handle settings controls
            if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                let scale_w = client_w as f32 / 1024.0;
                let scale_h = client_h as f32 / 640.0;
                let s = (scale_w.min(scale_h)).clamp(1.0, 1.40);

                let hdr_h = (46.0 * s) as i32;
                let side_w = (210.0 * s) as i32;
                let back_w = (160.0 * s) as i32;
                let back_x = (client_w - back_w).max(side_w + 20);

                // Top-right [ < BACK TO STREAM ] button
                if client_x >= back_x && client_x <= client_w && client_y >= 0 && client_y <= hdr_h {
                    SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
                    return LRESULT(0);
                }

                // Left Navigation Dock Tabs (0..3)
                let nav_hdr_h = (32.0 * s) as i32;
                let tab_start_y = hdr_h + nav_hdr_h;
                let tab_h = (44.0 * s) as i32;
                if client_x >= 0 && client_x < side_w {
                    for i in 0..3 {
                        let item_y = tab_start_y + (i as i32) * tab_h;
                        if client_y >= item_y && client_y < item_y + tab_h {
                            SETTINGS_CATEGORY.store(i as u8, Ordering::Relaxed);
                            return LRESULT(0);
                        }
                    }
                }

                // Right Workspace Geometry
                let work_x = side_w + (24.0 * s) as i32;
                let work_w = (client_w - work_x - (24.0 * s) as i32).clamp(360, 960);
                let work_y = hdr_h + (20.0 * s) as i32;
                let content_y = work_y + (44.0 * s) as i32;
                let strip_x = work_x + (16.0 * s) as i32;
                let strip_w = work_w - (32.0 * s) as i32;
                let strip_h = (38.0 * s) as i32;
                let mod_hdr_h = (32.0 * s) as i32;

                let active_cat = SETTINGS_CATEGORY.load(Ordering::Relaxed).min(2);

                let hit_segment = |strip_y: i32, count: i32| -> Option<usize> {
                    if client_x >= strip_x
                        && client_x <= strip_x + strip_w
                        && client_y >= strip_y
                        && client_y <= strip_y + strip_h
                    {
                        let seg_w = (strip_w / count.max(1)).max(1);
                        let idx = ((client_x - strip_x) / seg_w).clamp(0, count - 1) as usize;
                        Some(idx)
                    } else {
                        None
                    }
                };

                let m1_y = content_y;
                let m1_h = (94.0 * s) as i32;
                let m2_y = m1_y + m1_h + (16.0 * s) as i32;
                let m2_h = (94.0 * s) as i32;

                let strip1_y = m1_y + mod_hdr_h + (12.0 * s) as i32;
                let strip2_y = m2_y + mod_hdr_h + (12.0 * s) as i32;

                match active_cat {
                    0 => {
                        // Category 0: Display & Quality
                        if let Some(idx) = hit_segment(strip1_y, 3) {
                            DISPLAY_MODE.store(idx as u8, Ordering::Relaxed);
                            return LRESULT(0);
                        }
                        if let Some(idx) = hit_segment(strip2_y, 4) {
                            let target_mode = match idx {
                                0 => 0,
                                1 => 2,
                                2 => 3,
                                _ => 4,
                            };
                            crate::p2p::set_quality_mode(target_mode);
                            info!("Settings: Quality preset set to mode {}", target_mode);
                            return LRESULT(0);
                        }
                    }
                    1 => {
                        // Category 1: Framerate & Input
                        if let Some(idx) = hit_segment(strip1_y, 3) {
                            let new_fps = match idx {
                                0 => 30,
                                2 => 120,
                                _ => 60,
                            };
                            TARGET_FPS.store(new_fps, Ordering::Relaxed);
                            crate::p2p::update_client_preferences(
                                new_fps,
                                CAPTURE_CURSOR.load(Ordering::Relaxed),
                            );
                            return LRESULT(0);
                        }
                        if let Some(idx) = hit_segment(strip2_y, 2) {
                            let enabled = idx == 0;
                            CAPTURE_CURSOR.store(enabled, Ordering::Relaxed);
                            crate::p2p::update_client_preferences(
                                TARGET_FPS.load(Ordering::Relaxed),
                                enabled,
                            );
                            return LRESULT(0);
                        }
                    }
                    _ => {}
                }

                // Bottom [ APPLY & CLOSE ] button
                let footer_h = (40.0 * s) as i32;
                let footer_y = (client_h - footer_h - (16.0 * s) as i32).max(m2_y + m2_h + (16.0 * s) as i32);
                let apply_w = (220.0 * s) as i32;
                let apply_x = work_x + work_w - apply_w;
                if client_x >= apply_x
                    && client_x <= work_x + work_w
                    && client_y >= footer_y
                    && client_y <= footer_y + footer_h
                {
                    SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
                    return LRESULT(0);
                }

                return LRESULT(0);
            }

            // 2. Tab Bar Interaction
            let hit = get_tab_hit_state(hwnd, client_x, client_y);
            if hit == 9001 {
                VIEWPORT_IS_ADMIN_MODE.store(false, Ordering::Relaxed);
                return LRESULT(0);
            } else if hit == 9002 {
                VIEWPORT_IS_ADMIN_MODE.store(true, Ordering::Relaxed);
                let state = super::admin::get_admin_state().lock().unwrap();
                if state.processes.is_empty() {
                    crate::p2p::send_admin_request(crate::protocol::AdminReq::GetProcesses);
                }
                return LRESULT(0);
            } else if hit >= 1000 {
                let close_idx = (hit - 1000) as usize;
                close_session_tab(hwnd, close_idx);
                return LRESULT(0);
            } else if hit == 999 {
                // New Tab '+' clicked: bring Launcher window to foreground
                bring_main_window_to_front();
                return LRESULT(0);
            } else if hit >= 0 {
                let tab_idx = hit as usize;
                switch_session_tab(tab_idx);
                DRAGGING_TAB_INDEX.store(hit, Ordering::Relaxed);
                *DRAG_START_POS.lock().unwrap() = Some((client_x, client_y));
                unsafe {
                    let _ = SetCapture(hwnd);
                }
                return LRESULT(0);
            }

            if client_y >= TOOLBAR_HEIGHT {
                if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                    if super::admin::handle_admin_click(client_x, client_y, client_w, client_h) {
                        return LRESULT(0);
                    }
                } else {
                    unsafe {
                        inject_remote_mouse(hwnd, client_x, client_y, true, false);
                    }
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            let drag = DRAGGING_TAB_INDEX.swap(-1, Ordering::Relaxed);
            if drag >= 0 {
                unsafe {
                    let _ = ReleaseCapture();
                }
                if let Some((sx, sy)) = DRAG_START_POS.lock().unwrap().take() {
                    let dx = (client_x - sx).abs();
                    let dy = (client_y - sy).abs();
                    let mut rc = RECT::default();
                    unsafe { let _ = GetClientRect(hwnd, &mut rc); }
                    let client_w = rc.right - rc.left;
                    if dx > 80 || dy > 45 || client_y < -20 || client_x < -20 || client_x > client_w + 20 {
                        let mut pt = POINT { x: client_x, y: client_y };
                        unsafe {
                            let _ = ClientToScreen(hwnd, &mut pt);
                        }
                        detach_tab(drag as usize, pt.x, pt.y);
                        return LRESULT(0);
                    }
                }
            }

            if client_y >= TOOLBAR_HEIGHT {
                unsafe {
                    inject_remote_mouse(hwnd, client_x, client_y, false, false);
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            if client_y >= TOOLBAR_HEIGHT && !VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                unsafe {
                    inject_remote_mouse(hwnd, client_x, client_y, true, true);
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            if client_y >= TOOLBAR_HEIGHT && !VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                unsafe {
                    inject_remote_mouse(hwnd, client_x, client_y, false, true);
                }
            }
            LRESULT(0)
        }
        WM_MBUTTONDOWN => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            if client_y >= TOOLBAR_HEIGHT && !VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                unsafe {
                    inject_remote_mouse_middle(hwnd, client_x, client_y, true);
                }
            }
            LRESULT(0)
        }
        WM_MBUTTONUP => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            if client_y >= TOOLBAR_HEIGHT && !VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                unsafe {
                    inject_remote_mouse_middle(hwnd, client_x, client_y, false);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            if client_y < TOOLBAR_HEIGHT {
                let prev = TAB_HOVER_STATE.load(Ordering::Relaxed);
                let hit = get_tab_hit_state(hwnd, client_x, client_y);
                if prev != hit {
                    TAB_HOVER_STATE.store(hit, Ordering::Relaxed);
                }
            } else {
                TAB_HOVER_STATE.store(-1, Ordering::Relaxed);
                if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                    let mut rect = RECT::default();
                    let _ = unsafe { GetClientRect(hwnd, &mut rect) };
                    let client_w = rect.right - rect.left;
                    let client_h = rect.bottom - rect.top;
                    let hover = super::admin::get_admin_hover_state(client_x, client_y, client_w, client_h);
                    ADMIN_HOVER_STATE.store(hover, Ordering::Relaxed);
                } else {
                    unsafe {
                        inject_remote_mouse_move(hwnd, client_x, client_y);
                    }
                }
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let hit_test = (lparam.0 & 0xffff) as i16 as i32;
            if hit_test == 1 {
                let mut cursor_pt = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut cursor_pt);
                    let _ = ScreenToClient(hwnd, &mut cursor_pt);
                    if cursor_pt.y < TOOLBAR_HEIGHT {
                        let hit = get_tab_hit_state(hwnd, cursor_pt.x, cursor_pt.y);
                        let cursor_name = if hit >= 0 { IDC_HAND } else { IDC_ARROW };
                        let c = LoadCursorW(None, cursor_name).unwrap_or_default();
                        SetCursor(Some(c));
                        return LRESULT(1);
                    }
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xffff) as i16 as i32;
            if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                super::admin::handle_admin_scroll(delta);
            } else {
                crate::p2p::send_remote_input(InputEvent::MouseWheel { delta });
            }
            LRESULT(0)
        }
        WM_CHAR => {
            if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                if let Some(c) = char::from_u32(wparam.0 as u32) {
                    let _ = super::admin::handle_admin_char(c);
                }
                return LRESULT(0);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                let _ = super::admin::handle_admin_key(wparam.0 as u32);
                return LRESULT(0);
            }
            if wparam.0 == 0x1B {
                if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                    SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
                } else {
                    unsafe {
                        inject_remote_key(0x1B, false);
                    }
                }
            } else if wparam.0 == 0x09 {
                let prev = SHOW_SETTINGS_MODAL.fetch_xor(true, Ordering::Relaxed);
                info!("Full-Screen Settings Page toggled: {}", !prev);
            } else if !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                let vk = wparam.0 as u8;
                let is_ctrl = (unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x11) } as i16) < 0;
                if is_ctrl && (vk == b'V' || vk == b'v') {
                    let files = crate::p2p::read_clipboard_files();
                    if !files.is_empty() {
                        for f in files {
                            info!("Clipboard Pasting file: {:?}", f);
                            crate::p2p::send_file_transfer(f);
                        }
                        return LRESULT(0);
                    }
                }
                unsafe {
                    inject_remote_key(vk, false);
                }
            }
            LRESULT(0)
        }
        WM_KEYUP => {
            if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                return LRESULT(0);
            }
            if wparam.0 != 0x1B && wparam.0 != 0x09 && !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                unsafe {
                    inject_remote_key(wparam.0 as u8, true);
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            unsafe {
                let mut ps = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut ps);
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = RemoveClipboardFormatListener(hwnd);
            VIEWPORT_RUNNING.store(false, Ordering::SeqCst);
            crate::p2p::stop_client_session();
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}