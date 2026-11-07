#![allow(unsafe_op_in_unsafe_fn)]
use crate::p2p::SessionStatus;
use crate::protocol::{InputEvent, MouseButton};
use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU8, Ordering};
use std::time::{Duration, Instant};
use tracing::info;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC, InvalidateRect,
    Rectangle, ReleaseDC, ScreenToClient, SelectObject, SetBkColor, SetBkMode, SetBrushOrgEx,
    SetStretchBltMode, SetTextColor, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    COLORONCOLOR, DIB_RGB_COLORS, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL,
    FW_SEMIBOLD, HALFTONE, HBRUSH, HDC, HFONT, HGDIOBJ, OPAQUE, PAINTSTRUCT, PS_SOLID, SRCCOPY,
    TRANSPARENT,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GHND};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetCursorPos,
    GetMessageW, GetWindowTextLengthW, GetWindowTextW, IsWindow, LoadCursorW, PostQuitMessage,
    RegisterClassExW, SendMessageW, SetCursor, SetForegroundWindow, SetWindowDisplayAffinity,
    SetWindowPos, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, HMENU,
    IDC_ARROW, IDC_HAND, MSG, SWP_NOACTIVATE, SWP_NOZORDER, SW_HIDE, SW_RESTORE, SW_SHOW,
    WDA_EXCLUDEFROMCAPTURE, WINDOW_EX_STYLE, WM_CREATE, WM_CTLCOLOREDIT, WM_DESTROY, WM_ERASEBKGND,
    WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SETFONT, WM_SIZE,
    WNDCLASSEXW, WS_CHILD, WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_VISIBLE,
};

// pkasoft.com White Theme color palette (GDI COLORREF little-endian: 0x00BBGGRR)
const PK_BG: COLORREF = COLORREF(0x00F5F7F5);          // #f5f7f5 (--paper soft clean canvas)
const PK_CARD: COLORREF = COLORREF(0x00FFFFFF);        // #ffffff (pure crisp white card)
const PK_CARD_BORDER: COLORREF = COLORREF(0x00E3E7E2); // #e2e7e3 (--line border)
const PK_GREEN: COLORREF = COLORREF(0x004F6A2D);       // #2d6a4f (--green signature emerald)
const PK_GREEN_DARK: COLORREF = COLORREF(0x0032431B);  // #1b4332 (--green-dark pine hover)
const PK_GREEN_TINT: COLORREF = COLORREF(0x00EEF5F0);  // #f0f5ee (subtle green pill fill)
const PK_TEXT: COLORREF = COLORREF(0x0020241D);        // #1d2420 (--ink deep crisp charcoal)
const PK_MUTED: COLORREF = COLORREF(0x00636A5D);       // #5d6a63 (--muted slate sage)
const PK_INPUT_BG: COLORREF = COLORREF(0x00FAFCFA);    // #fafcfa (soft clean input background)
const PK_BTN_HOVER: COLORREF = COLORREF(0x00E5EDE7);   // #e7ede5 (hover outline button)
const PK_RED: COLORREF = COLORREF(0x003030C5);         // #c53030 (crimson decline button)

// Control IDs
const ID_EDIT_TARGET_CODE: usize = 1003;

static IS_RUNNING: AtomicBool = AtomicBool::new(false);
static VIEWPORT_RUNNING: AtomicBool = AtomicBool::new(false);

static REMOTE_ORIG_W: AtomicU32 = AtomicU32::new(0);
static REMOTE_ORIG_H: AtomicU32 = AtomicU32::new(0);

static mut HWND_MAIN_WINDOW: HWND = HWND(std::ptr::null_mut());
static mut HWND_EDIT_TARGET: HWND = HWND(std::ptr::null_mut());
static mut BG_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());
static mut CARD_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());
static mut VIEWPORT_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());
static mut EDIT_BG_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());

static HOVER_BUTTON: AtomicU8 = AtomicU8::new(0); // 0 = none, 1 = copy, 2 = connect, 3 = accept, 4 = decline
static CURRENT_EDIT_FONT: AtomicIsize = AtomicIsize::new(0);

pub static LOCAL_DESKY_ID: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
static STATUS_LOGS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub fn request_main_redraw() {
    unsafe {
        if !HWND_MAIN_WINDOW.0.is_null() && IsWindow(Some(HWND_MAIN_WINDOW)).as_bool() {
            let _ = InvalidateRect(Some(HWND_MAIN_WINDOW), None, false);
        }
    }
}

pub fn bring_main_window_to_front() {
    unsafe {
        if !HWND_MAIN_WINDOW.0.is_null() && IsWindow(Some(HWND_MAIN_WINDOW)).as_bool() {
            let _ = ShowWindow(HWND_MAIN_WINDOW, SW_RESTORE);
            let _ = SetForegroundWindow(HWND_MAIN_WINDOW);
            let _ = InvalidateRect(Some(HWND_MAIN_WINDOW), None, false);
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
    append_status_log(&format!("✅ Online • ID: {}", id));
    request_main_redraw();
}

unsafe fn create_segoe_font(height: i32, weight: windows::Win32::Graphics::Gdi::FONT_WEIGHT) -> HFONT {
    use windows::Win32::Graphics::Gdi::{FONT_CHARSET, FONT_CLIP_PRECISION, FONT_OUTPUT_PRECISION, FONT_QUALITY};
    CreateFontW(
        height,
        0,
        0,
        0,
        weight.0 as i32,
        0,
        0,
        0,
        FONT_CHARSET(1), // DEFAULT_CHARSET
        FONT_OUTPUT_PRECISION(0),
        FONT_CLIP_PRECISION(0),
        FONT_QUALITY(5), // CLEARTYPE_QUALITY
        0,
        w!("Segoe UI"),
    )
}

unsafe fn draw_rounded_box(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    fill_color: COLORREF,
    border_color: COLORREF,
    _radius: i32,
) {
    let brush = CreateSolidBrush(fill_color);
    let pen = CreatePen(PS_SOLID, 1, border_color);
    let old_brush = SelectObject(hdc, HGDIOBJ(brush.0));
    let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));

    let _ = Rectangle(hdc, x, y, x + w, y + h);

    let _ = SelectObject(hdc, old_brush);
    let _ = SelectObject(hdc, old_pen);
    let _ = DeleteObject(HGDIOBJ(brush.0));
    let _ = DeleteObject(HGDIOBJ(pen.0));
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

    pub b_x: i32,
    pub b_y: i32,
    pub b_w: i32,
    pub b_h: i32,

    pub console_box_x: i32,
    pub console_box_y: i32,
    pub console_box_w: i32,
    pub console_box_h: i32,

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

    // Bottom Activity Card
    let b_x = margin;
    let b_y = card_y + card_h + 16;
    let b_w = w - (2 * margin);
    let b_h = (h - b_y - margin).max(120);

    let console_box_x = b_x + 18;
    let console_box_y = b_y + (30.0 * font_scale) as i32;
    let console_box_w = b_w - 36;
    let console_box_h = (b_h - (42.0 * font_scale) as i32).max(60);

    ResponsiveLayout {
        c1_x, c2_x, card_w, card_h, card_y,
        id_box_x, id_box_y, id_box_w, id_box_h,
        copy_btn_x, copy_btn_y, copy_btn_w, copy_btn_h,
        input_box_x, input_box_y, input_box_w, input_box_h,
        edit_x, edit_y, edit_w, edit_h,
        conn_btn_x, conn_btn_y, conn_btn_w, conn_btn_h,
        b_x, b_y, b_w, b_h,
        console_box_x, console_box_y, console_box_w, console_box_h,
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
                w!("Desky • pkasoft.com"),
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

const TOOLBAR_HEIGHT: i32 = 40;

// Display mode:
// 0 = Auto Fit (aspect ratio preserved, centered)
// 1 = Fill Window (stretched to borders)
// 2 = 1:1 Original resolution
static DISPLAY_MODE: AtomicU8 = AtomicU8::new(0);

// Target FPS: 30, 60, 120
static TARGET_FPS: AtomicU32 = AtomicU32::new(60);

// Toolbar visibility: true = shown, false = collapsed/hidden
static SHOW_TOOLBAR: AtomicBool = AtomicBool::new(true);

// Full-Screen Settings Page state
static SHOW_SETTINGS_MODAL: AtomicBool = AtomicBool::new(false);

// Active AnyDesk-style settings sidebar category:
// 0 = Display & Quality, 1 = Adaptive Bitrate (ABR), 2 = Performance & Input, 3 = Connection Info
static SETTINGS_CATEGORY: AtomicU8 = AtomicU8::new(0);

// Remote cursor capture toggle (renders live host cursor on stream)
static CAPTURE_CURSOR: AtomicBool = AtomicBool::new(true);

fn format_bitrate(kbps: u32) -> String {
    if kbps >= 1000 {
        format!("{:.1} Mbps", kbps as f32 / 1000.0)
    } else if kbps > 0 {
        format!("{} kbps", kbps)
    } else {
        "Measuring...".to_string()
    }
}

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

unsafe fn draw_toolbar(
    hdc: HDC,
    client_w: i32,
    toolbar_h: i32,
    mode: u8,
    fps: u32,
    latency: Duration,
) {
    // 1. Clean White Integrated Toolbar Background (PK_CARD)
    let bar_rect = RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: toolbar_h,
    };
    FillRect(hdc, &bar_rect, CARD_BRUSH);

    // 2. Sharp Bottom Divider Line (PK_CARD_BORDER)
    let border_brush = CreateSolidBrush(PK_CARD_BORDER);
    let border_rect = RECT {
        left: 0,
        top: toolbar_h - 1,
        right: client_w,
        bottom: toolbar_h,
    };
    FillRect(hdc, &border_rect, border_brush);

    let font_bold = create_segoe_font(14, FW_BOLD);
    let font_reg = create_segoe_font(13, FW_SEMIBOLD);
    let old_font = SelectObject(hdc, HGDIOBJ(font_bold.0));
    SetBkMode(hdc, TRANSPARENT);

    // 3. Integrated Left Brand Cell (0..104)
    let div1 = RECT { left: 104, top: 0, right: 105, bottom: toolbar_h };
    FillRect(hdc, &div1, border_brush);
    SetTextColor(hdc, PK_GREEN_DARK);
    let mut logo_text: Vec<u16> = "⚡ DESKY".encode_utf16().collect();
    let mut logo_rect = RECT { left: 0, top: 0, right: 104, bottom: toolbar_h };
    let _ = DrawTextW(hdc, &mut logo_text, &mut logo_rect, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    // 4. Integrated LIVE Status Cell (105..178)
    let tint_brush = CreateSolidBrush(PK_GREEN_TINT);
    let live_cell = RECT { left: 105, top: 0, right: 178, bottom: toolbar_h - 1 };
    FillRect(hdc, &live_cell, tint_brush);
    let _ = DeleteObject(HGDIOBJ(tint_brush.0));
    let div2 = RECT { left: 178, top: 0, right: 179, bottom: toolbar_h };
    FillRect(hdc, &div2, border_brush);

    let _ = SelectObject(hdc, HGDIOBJ(font_reg.0));
    SetTextColor(hdc, PK_GREEN);
    let mut live_text: Vec<u16> = "● LIVE".encode_utf16().collect();
    let mut live_rect = RECT { left: 105, top: 0, right: 178, bottom: toolbar_h };
    let _ = DrawTextW(hdc, &mut live_text, &mut live_rect, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    // 5. Integrated Telemetry Strip
    let mode_str = match mode {
        0 => "AUTO FIT",
        1 => "STRETCH",
        _ => "1:1 PIXEL",
    };
    let q_mode_str = match crate::p2p::get_quality_mode() {
        0 => "ABR AUTO",
        1 => "ULTRA 4:4:4",
        2 => "HIGH 4:4:4",
        3 => "BALANCED",
        _ => "FAST 4:2:0",
    };
    let (q_val, sc_val, _) = crate::p2p::get_active_quality_params();
    let bitrate_str = format_bitrate(crate::p2p::get_session_bitrate_kbps());
    let badge_str = format!(
        "{}   |   {} (Q{} • {}%)   |   {}   |   {} FPS   |   PING {:?}",
        mode_str, q_mode_str, q_val, sc_val, bitrate_str, fps, latency
    );
    SetTextColor(hdc, PK_MUTED);
    let mut badge_wide: Vec<u16> = badge_str.encode_utf16().collect();
    let mut badge_rect = RECT { left: 192, top: 0, right: client_w - 220, bottom: toolbar_h };
    let _ = DrawTextW(hdc, &mut badge_wide, &mut badge_rect, DT_SINGLELINE | DT_VCENTER);

    // 6. Integrated Right Flush Action Cells (0px radius, full height)
    let btn_settings_x = client_w - 216;
    let btn_hide_x = client_w - 104;

    let green_brush = CreateSolidBrush(PK_GREEN);
    let set_rc = RECT {
        left: btn_settings_x,
        top: 0,
        right: btn_hide_x,
        bottom: toolbar_h - 1,
    };
    FillRect(hdc, &set_rc, green_brush);
    let _ = DeleteObject(HGDIOBJ(green_brush.0));

    SetTextColor(hdc, COLORREF(0x00FFFFFF));
    let mut set_wide: Vec<u16> = "⚙ SETTINGS".encode_utf16().collect();
    let mut set_txt_rc = set_rc;
    let _ = DrawTextW(hdc, &mut set_wide, &mut set_txt_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    let hide_brush = CreateSolidBrush(PK_INPUT_BG);
    let hide_rc = RECT {
        left: btn_hide_x,
        top: 0,
        right: client_w,
        bottom: toolbar_h - 1,
    };
    FillRect(hdc, &hide_rc, hide_brush);
    let _ = DeleteObject(HGDIOBJ(hide_brush.0));
    let div3 = RECT { left: btn_hide_x, top: 0, right: btn_hide_x + 1, bottom: toolbar_h };
    FillRect(hdc, &div3, border_brush);

    SetTextColor(hdc, PK_TEXT);
    let mut hide_wide: Vec<u16> = "▲ HIDE".encode_utf16().collect();
    let mut hide_txt_rc = hide_rc;
    let _ = DrawTextW(hdc, &mut hide_wide, &mut hide_txt_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(HGDIOBJ(border_brush.0));
    let _ = DeleteObject(HGDIOBJ(font_bold.0));
    let _ = DeleteObject(HGDIOBJ(font_reg.0));
}

/// Renders an AnyDesk + Decky-Dashboard inspired Full-Screen Settings Page in the signature
/// White & Emerald Green (`pkasoft.com`) theme, with 0px sharp edges, flush integrated sidebar tabs,
/// segmented control strips, and distinct per-category views.
unsafe fn draw_fullscreen_settings_view(
    hdc: HDC,
    client_w: i32,
    client_h: i32,
    mode: u8,
    fps: u32,
) {
    // 1. Opaque Full-Screen Soft Sage-White Canvas (PK_BG)
    let full_rect = RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: client_h,
    };
    FillRect(hdc, &full_rect, BG_BRUSH);

    let line_brush = CreateSolidBrush(PK_CARD_BORDER);
    let green_brush = CreateSolidBrush(PK_GREEN);
    let green_dark_brush = CreateSolidBrush(PK_GREEN_DARK);
    let tint_brush = CreateSolidBrush(PK_GREEN_TINT);
    let input_brush = CreateSolidBrush(PK_INPUT_BG);

    // 2. Integrated Top Dashboard Bar (Height: 52px, flush sharp border)
    let hdr_h = 52;
    let hdr_rect = RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: hdr_h,
    };
    FillRect(hdc, &hdr_rect, CARD_BRUSH);
    let hdr_line = RECT {
        left: 0,
        top: hdr_h - 1,
        right: client_w,
        bottom: hdr_h,
    };
    FillRect(hdc, &hdr_line, line_brush);

    let font_brand = create_segoe_font(18, FW_BOLD);
    let font_title = create_segoe_font(20, FW_BOLD);
    let font_sec = create_segoe_font(12, FW_BOLD);
    let font_btn = create_segoe_font(14, FW_SEMIBOLD);
    let font_info = create_segoe_font(13, FW_NORMAL);

    SetBkMode(hdc, TRANSPARENT);

    let side_w = 250;

    // Top-left Brand Cell (0..side_w) aligned flush with the Left Sidebar
    let brand_cell_div = RECT {
        left: side_w - 1,
        top: 0,
        right: side_w,
        bottom: hdr_h,
    };
    FillRect(hdc, &brand_cell_div, line_brush);

    let old_font = SelectObject(hdc, HGDIOBJ(font_brand.0));
    SetTextColor(hdc, PK_GREEN_DARK);
    let mut brand_txt: Vec<u16> = "⚡ DESKY DASHBOARD".encode_utf16().collect();
    let mut brand_rc = RECT {
        left: 20,
        top: 0,
        right: side_w - 10,
        bottom: hdr_h,
    };
    let _ = DrawTextW(hdc, &mut brand_txt, &mut brand_rc, DT_SINGLELINE | DT_VCENTER);

    // Top-bar Breadcrumb label
    let active_cat = SETTINGS_CATEGORY.load(Ordering::Relaxed);
    let crumb_label = match active_cat {
        0 => "SETTINGS  /  DISPLAY & IMAGE QUALITY",
        1 => "SETTINGS  /  DYNAMIC ADAPTIVE BITRATE (ABR)",
        2 => "SETTINGS  /  INPUT & STREAM FRAMERATE",
        _ => "SETTINGS  /  DIRECT P2P CONNECTION TELEMETRY",
    };
    let _ = SelectObject(hdc, HGDIOBJ(font_sec.0));
    SetTextColor(hdc, PK_MUTED);
    let mut crumb_txt: Vec<u16> = crumb_label.encode_utf16().collect();
    let mut crumb_rc = RECT {
        left: side_w + 24,
        top: 0,
        right: client_w - 250,
        bottom: hdr_h,
    };
    let _ = DrawTextW(hdc, &mut crumb_txt, &mut crumb_rc, DT_SINGLELINE | DT_VCENTER);

    // Top-right Flush Integrated [ ← BACK TO STREAM ] Button (0px radius, docked into top bar)
    let back_w = 220;
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
    let mut back_txt: Vec<u16> = "←  BACK TO STREAM".encode_utf16().collect();
    let mut back_txt_rc = back_rc;
    let _ = DrawTextW(
        hdc,
        &mut back_txt,
        &mut back_txt_rc,
        DT_SINGLELINE | DT_CENTER | DT_VCENTER,
    );

    // =========================================================================
    // LEFT INTEGRATED NAVIGATION DOCK (0..side_w, hdr_h..client_h)
    // =========================================================================
    let side_rc = RECT {
        left: 0,
        top: hdr_h,
        right: side_w,
        bottom: client_h,
    };
    FillRect(hdc, &side_rc, CARD_BRUSH);
    let side_border = RECT {
        left: side_w - 1,
        top: hdr_h,
        right: side_w,
        bottom: client_h,
    };
    FillRect(hdc, &side_border, line_brush);

    // Section Header in Dock
    let nav_hdr_bg = RECT {
        left: 0,
        top: hdr_h,
        right: side_w - 1,
        bottom: hdr_h + 36,
    };
    FillRect(hdc, &nav_hdr_bg, input_brush);
    let nav_hdr_line = RECT {
        left: 0,
        top: hdr_h + 35,
        right: side_w,
        bottom: hdr_h + 36,
    };
    FillRect(hdc, &nav_hdr_line, line_brush);

    let _ = SelectObject(hdc, HGDIOBJ(font_sec.0));
    SetTextColor(hdc, PK_MUTED);
    let mut nav_hdr: Vec<u16> = "CONFIGURATION MODULES".encode_utf16().collect();
    let mut nav_hdr_rc = RECT {
        left: 20,
        top: hdr_h,
        right: side_w - 12,
        bottom: hdr_h + 36,
    };
    let _ = DrawTextW(hdc, &mut nav_hdr, &mut nav_hdr_rc, DT_SINGLELINE | DT_VCENTER);

    let categories = [
        ("01", "DISPLAY & QUALITY"),
        ("02", "ADAPTIVE BITRATE"),
        ("03", "INPUT & FRAMERATE"),
        ("04", "CONNECTION STATUS"),
    ];

    let tab_start_y = hdr_h + 36;
    let tab_h = 52;

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
            // Sharp 5px Emerald Indicator Strip on left edge
            let bar_rc = RECT {
                left: 0,
                top: item_y,
                right: 5,
                bottom: item_y + tab_h,
            };
            FillRect(hdc, &bar_rc, green_brush);
        } else {
            FillRect(hdc, &item_rc, CARD_BRUSH);
        }

        // Sharp 1px bottom divider per tab row
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
            left: 20,
            top: item_y,
            right: 48,
            bottom: item_y + tab_h,
        };
        let _ = DrawTextW(hdc, &mut num_wide, &mut num_rc, DT_SINGLELINE | DT_VCENTER);

        let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
        SetTextColor(hdc, if is_sel { PK_GREEN_DARK } else { PK_TEXT });
        let mut lbl_wide: Vec<u16> = label.encode_utf16().collect();
        let mut lbl_rc = RECT {
            left: 50,
            top: item_y,
            right: side_w - 12,
            bottom: item_y + tab_h,
        };
        let _ = DrawTextW(hdc, &mut lbl_wide, &mut lbl_rc, DT_SINGLELINE | DT_VCENTER);
    }

    // Integrated Bottom Dock Telemetry Panel (Flush with bottom of sidebar)
    let (active_q, active_scale, active_c444) = crate::p2p::get_active_quality_params();
    let bitrate_str = format_bitrate(crate::p2p::get_session_bitrate_kbps());
    let rtt_ms = crate::p2p::get_session_rtt_ms();
    let orig_w = REMOTE_ORIG_W.load(Ordering::Relaxed);
    let orig_h = REMOTE_ORIG_H.load(Ordering::Relaxed);
    let res_str = if orig_w > 0 && orig_h > 0 {
        format!("{}×{} ({}%)", orig_w, orig_h, active_scale)
    } else {
        format!("{}% Native", active_scale)
    };
    let chroma_str = if active_c444 { "4:4:4 Subpixel" } else { "4:2:0 Fast" };

    let tele_h = 180;
    let tele_y = (client_h - tele_h).max(tab_start_y + 4 * tab_h + 12);
    let tele_top_line = RECT {
        left: 0,
        top: tele_y,
        right: side_w,
        bottom: tele_y + 1,
    };
    FillRect(hdc, &tele_top_line, line_brush);

    let tele_hdr_bg = RECT {
        left: 0,
        top: tele_y + 1,
        right: side_w - 1,
        bottom: tele_y + 34,
    };
    FillRect(hdc, &tele_hdr_bg, tint_brush);
    let tele_hdr_line = RECT {
        left: 0,
        top: tele_y + 34,
        right: side_w,
        bottom: tele_y + 35,
    };
    FillRect(hdc, &tele_hdr_line, line_brush);

    let _ = SelectObject(hdc, HGDIOBJ(font_sec.0));
    SetTextColor(hdc, PK_GREEN_DARK);
    let mut tele_hdr: Vec<u16> = "● DIRECT P2P TELEMETRY".encode_utf16().collect();
    let mut tele_hdr_rc = RECT {
        left: 20,
        top: tele_y + 1,
        right: side_w - 12,
        bottom: tele_y + 34,
    };
    let _ = DrawTextW(hdc, &mut tele_hdr, &mut tele_hdr_rc, DT_SINGLELINE | DT_VCENTER);

    let stats_rows = [
        ("BITRATE", bitrate_str.as_str()),
        ("LATENCY", &format!("{} ms RTT", rtt_ms)),
        ("ENCODER", &format!("Q{} • {}", active_q, chroma_str)),
        ("STREAM", res_str.as_str()),
        ("TUNNEL", "100% Direct UDP"),
    ];
    let _ = SelectObject(hdc, HGDIOBJ(font_info.0));
    for (idx, (k, v)) in stats_rows.iter().enumerate() {
        let ry = tele_y + 42 + (idx as i32) * 26;
        SetTextColor(hdc, PK_MUTED);
        let mut kw: Vec<u16> = k.encode_utf16().collect();
        let mut krc = RECT { left: 20, top: ry, right: 96, bottom: ry + 24 };
        let _ = DrawTextW(hdc, &mut kw, &mut krc, DT_SINGLELINE | DT_VCENTER);

        SetTextColor(hdc, PK_TEXT);
        let mut vw: Vec<u16> = v.encode_utf16().collect();
        let mut vrc = RECT { left: 98, top: ry, right: side_w - 12, bottom: ry + 24 };
        let _ = DrawTextW(hdc, &mut vw, &mut vrc, DT_SINGLELINE | DT_VCENTER);
    }

    // =========================================================================
    // RIGHT WORKSPACE AREA: DECKY-STYLE INTEGRATED SHARP MODULES
    // =========================================================================
    let work_x = side_w + 28;
    let work_w = (client_w - work_x - 28).clamp(420, 960);
    let work_y = hdr_h + 24;

    // Helper: Draws a sharp-edged integrated Decky module container with header bar
    let draw_module_panel = |top: i32, height: i32, badge: &str, title: &str| {
        // Outer 0px rectangle card
        draw_rounded_box(hdc, work_x, top, work_w, height, PK_CARD, PK_CARD_BORDER, 0);
        // Integrated Header Strip (height 36px)
        let h_bg = RECT {
            left: work_x + 1,
            top: top + 1,
            right: work_x + work_w - 1,
            bottom: top + 37,
        };
        FillRect(hdc, &h_bg, input_brush);
        let h_line = RECT {
            left: work_x,
            top: top + 37,
            right: work_x + work_w,
            bottom: top + 38,
        };
        FillRect(hdc, &h_line, line_brush);

        // Left Emerald Badge cell inside module header
        let b_rc = RECT {
            left: work_x + 1,
            top: top + 1,
            right: work_x + 92,
            bottom: top + 37,
        };
        FillRect(hdc, &b_rc, tint_brush);
        let b_div = RECT {
            left: work_x + 92,
            top: top + 1,
            right: work_x + 93,
            bottom: top + 37,
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
            left: work_x + 106,
            top: top + 1,
            right: work_x + work_w - 16,
            bottom: top + 37,
        };
        let _ = DrawTextW(hdc, &mut tw, &mut t_rc, DT_SINGLELINE | DT_VCENTER);
    };

    // Helper: Draws a flush, sharp-edged segmented button bar (0px radius, shared 1px dividers)
    let draw_segmented_strip = |x: i32, y: i32, w: i32, h: i32, labels: &[&str], active_idx: usize| {
        let count = labels.len().max(1) as i32;
        let seg_w = w / count;

        // Outer border frame
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
                // Top 3px dark-emerald highlight bar inside active segment
                let top_accent = RECT {
                    left: sx + 1,
                    top: y + 1,
                    right: sr - 1,
                    bottom: y + 4,
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

    // Category Title Banner at top of right workspace
    let (cat_heading, cat_desc) = match active_cat {
        0 => (
            "DISPLAY & IMAGE QUALITY",
            "Configure viewport aspect scaling, 1:1 pixel mapping, and subpixel text clarity presets.",
        ),
        1 => (
            "DYNAMIC ADAPTIVE BITRATE (ABR)",
            "Control how Desky automatically tunes JPEG quantization and 4:4:4 chroma based on real-time link speed.",
        ),
        2 => (
            "INPUT & STREAM FRAMERATE",
            "Adjust target capture framerate (FPS) and remote host cursor rendering synchronization.",
        ),
        _ => (
            "DIRECT P2P CONNECTION STATUS",
            "Live end-to-end UDP hole-punching diagnostics. Zero video or input traffic ever touches the VDS.",
        ),
    };

    let _ = SelectObject(hdc, HGDIOBJ(font_title.0));
    SetTextColor(hdc, PK_TEXT);
    let mut ch_wide: Vec<u16> = cat_heading.encode_utf16().collect();
    let mut ch_rc = RECT {
        left: work_x,
        top: work_y,
        right: work_x + work_w,
        bottom: work_y + 28,
    };
    let _ = DrawTextW(hdc, &mut ch_wide, &mut ch_rc, DT_SINGLELINE | DT_VCENTER);

    let _ = SelectObject(hdc, HGDIOBJ(font_info.0));
    SetTextColor(hdc, PK_MUTED);
    let mut cd_wide: Vec<u16> = cat_desc.encode_utf16().collect();
    let mut cd_rc = RECT {
        left: work_x,
        top: work_y + 28,
        right: work_x + work_w,
        bottom: work_y + 50,
    };
    let _ = DrawTextW(hdc, &mut cd_wide, &mut cd_rc, DT_SINGLELINE | DT_VCENTER);

    let content_y = work_y + 62;
    let strip_x = work_x + 20;
    let strip_w = work_w - 40;
    let q_mode = crate::p2p::get_quality_mode() as usize;

    match active_cat {
        0 => {
            // =================================================================
            // CATEGORY 0: DISPLAY & QUALITY
            // =================================================================
            // Module 1: Viewport Scaling Mode (3 integrated segments)
            let m1_y = content_y;
            let m1_h = 132;
            draw_module_panel(m1_y, m1_h, "SCALING", "VIEWPORT ASPECT & SCALING MODE");

            let _ = SelectObject(hdc, HGDIOBJ(font_info.0));
            SetTextColor(hdc, PK_MUTED);
            let mut m1_hint: Vec<u16> =
                "Select how the remote desktop image is mapped inside your local viewport window:"
                    .encode_utf16()
                    .collect();
            let mut m1_hint_rc = RECT {
                left: strip_x,
                top: m1_y + 46,
                right: strip_x + strip_w,
                bottom: m1_y + 68,
            };
            let _ = DrawTextW(hdc, &mut m1_hint, &mut m1_hint_rc, DT_SINGLELINE | DT_VCENTER);

            let scale_labels = [
                "⛶  AUTO FIT (ASPECT RATIO)",
                "↔  STRETCH TO WINDOW",
                "1:1  ORIGINAL PIXELS",
            ];
            draw_segmented_strip(strip_x, m1_y + 74, strip_w, 42, &scale_labels, mode as usize);

            // Module 2: Text Clarity & Quality Preset (5 integrated segments)
            let m2_y = m1_y + m1_h + 16;
            let m2_h = 132;
            draw_module_panel(m2_y, m2_h, "CLARITY", "IMAGE QUALITY & SUBPIXEL TEXT PRESET");

            let _ = SelectObject(hdc, HGDIOBJ(font_info.0));
            SetTextColor(hdc, PK_MUTED);
            let mut m2_hint: Vec<u16> =
                "4:4:4 Chroma preserves razor-sharp ClearType text edges; Auto ABR adapts dynamically:"
                    .encode_utf16()
                    .collect();
            let mut m2_hint_rc = RECT {
                left: strip_x,
                top: m2_y + 46,
                right: strip_x + strip_w,
                bottom: m2_y + 68,
            };
            let _ = DrawTextW(hdc, &mut m2_hint, &mut m2_hint_rc, DT_SINGLELINE | DT_VCENTER);

            let q_labels = [
                "⚡ AUTO ABR",
                "💎 ULTRA TEXT",
                "🎯 HIGH 4:4:4",
                "⚖ BALANCED",
                "🚀 FAST 4:2:0",
            ];
            draw_segmented_strip(strip_x, m2_y + 74, strip_w, 42, &q_labels, q_mode);

            // Module 3: Active Rendering Pipeline Info
            let m3_y = m2_y + m2_h + 16;
            let m3_h = 96;
            draw_module_panel(m3_y, m3_h, "ENGINE", "HYBRID GDI VIEWPORT RENDERER");
            let _ = SelectObject(hdc, HGDIOBJ(font_info.0));
            SetTextColor(hdc, PK_TEXT);
            let mut m3_txt: Vec<u16> =
                "• Motion Pass: Hardware COLORONCOLOR (60 FPS zero-lag mouse tracking)\n• Idle Sharpening Pass: Automatic HALFTONE box-filter after 140ms static frame"
                    .encode_utf16()
                    .collect();
            let mut m3_rc = RECT {
                left: strip_x,
                top: m3_y + 46,
                right: strip_x + strip_w,
                bottom: m3_y + m3_h - 8,
            };
            let _ = DrawTextW(hdc, &mut m3_txt, &mut m3_rc, Default::default());
        }

        1 => {
            // =================================================================
            // CATEGORY 1: ADAPTIVE BITRATE (ABR)
            // =================================================================
            let m1_y = content_y;
            let m1_h = 132;
            draw_module_panel(m1_y, m1_h, "ABR MODE", "DYNAMIC BITRATE CONTROLLER PROFILE");

            let _ = SelectObject(hdc, HGDIOBJ(font_info.0));
            SetTextColor(hdc, PK_MUTED);
            let mut m1_hint: Vec<u16> =
                "Choose 'AUTO ABR' for automatic network speed adaptation or lock a fixed encoder tier:"
                    .encode_utf16()
                    .collect();
            let mut m1_hint_rc = RECT {
                left: strip_x,
                top: m1_y + 46,
                right: strip_x + strip_w,
                bottom: m1_y + 68,
            };
            let _ = DrawTextW(hdc, &mut m1_hint, &mut m1_hint_rc, DT_SINGLELINE | DT_VCENTER);

            let q_labels = [
                "⚡ AUTO ABR",
                "💎 ULTRA (Q88)",
                "🎯 HIGH (Q78)",
                "⚖ BALANCED (Q72)",
                "🚀 LOW DATA (Q60)",
            ];
            draw_segmented_strip(strip_x, m1_y + 74, strip_w, 42, &q_labels, q_mode);

            // Module 2: Live ABR Telemetry Matrix (4 sharp integrated metric cells)
            let m2_y = m1_y + m1_h + 16;
            let m2_h = 148;
            draw_module_panel(m2_y, m2_h, "METRICS", "REAL-TIME NETWORK & ENCODER TELEMETRY");

            let cell_w = strip_w / 4;
            let cell_y = m2_y + 50;
            let cell_h = 82;
            let metrics = [
                ("LIVE THROUGHPUT", bitrate_str.clone()),
                ("ROUND-TRIP PING", format!("{} ms", rtt_ms)),
                ("ACTIVE QUANTIZER", format!("JPEG Q{}", active_q)),
                ("CHROMA SAMPLING", chroma_str.to_string()),
            ];

            for (i, (m_lbl, m_val)) in metrics.iter().enumerate() {
                let cx = strip_x + (i as i32) * cell_w;
                let cw = if i == 3 { strip_x + strip_w - cx } else { cell_w };
                draw_rounded_box(hdc, cx, cell_y, cw, cell_h, PK_INPUT_BG, PK_CARD_BORDER, 0);

                let _ = SelectObject(hdc, HGDIOBJ(font_sec.0));
                SetTextColor(hdc, PK_MUTED);
                let mut mlw: Vec<u16> = m_lbl.encode_utf16().collect();
                let mut ml_rc = RECT {
                    left: cx + 12,
                    top: cell_y + 10,
                    right: cx + cw - 8,
                    bottom: cell_y + 32,
                };
                let _ = DrawTextW(hdc, &mut mlw, &mut ml_rc, DT_SINGLELINE | DT_VCENTER);

                let _ = SelectObject(hdc, HGDIOBJ(font_title.0));
                SetTextColor(hdc, PK_GREEN_DARK);
                let mut mvw: Vec<u16> = m_val.encode_utf16().collect();
                let mut mv_rc = RECT {
                    left: cx + 12,
                    top: cell_y + 36,
                    right: cx + cw - 8,
                    bottom: cell_y + 72,
                };
                let _ = DrawTextW(hdc, &mut mvw, &mut mv_rc, DT_SINGLELINE | DT_VCENTER);
            }
        }

        2 => {
            // =================================================================
            // CATEGORY 2: INPUT & FRAMERATE
            // =================================================================
            let m1_y = content_y;
            let m1_h = 132;
            draw_module_panel(m1_y, m1_h, "FRAMERATE", "TARGET STREAM CAPTURE RATE (FPS)");

            let _ = SelectObject(hdc, HGDIOBJ(font_info.0));
            SetTextColor(hdc, PK_MUTED);
            let mut m1_hint: Vec<u16> =
                "Higher framerate delivers smoother remote mouse feedback on fast connections:"
                    .encode_utf16()
                    .collect();
            let mut m1_hint_rc = RECT {
                left: strip_x,
                top: m1_y + 46,
                right: strip_x + strip_w,
                bottom: m1_y + 68,
            };
            let _ = DrawTextW(hdc, &mut m1_hint, &mut m1_hint_rc, DT_SINGLELINE | DT_VCENTER);

            let fps_idx = match fps {
                30 => 0,
                120 => 2,
                _ => 1,
            };
            let fps_labels = [
                "30 FPS  (POWER SAVER)",
                "60 FPS  (SMOOTH DEFAULT)",
                "120 FPS  (ULTRA LOW LATENCY)",
            ];
            draw_segmented_strip(strip_x, m1_y + 74, strip_w, 42, &fps_labels, fps_idx);

            // Module 2: Remote Cursor Synchronization (2-segment ON/OFF switch)
            let m2_y = m1_y + m1_h + 16;
            let m2_h = 132;
            draw_module_panel(m2_y, m2_h, "CURSOR", "REMOTE HOST CURSOR OVERLAY");

            let _ = SelectObject(hdc, HGDIOBJ(font_info.0));
            SetTextColor(hdc, PK_MUTED);
            let mut m2_hint: Vec<u16> =
                "Control whether the remote computer's cursor is rendered inside the video stream:"
                    .encode_utf16()
                    .collect();
            let mut m2_hint_rc = RECT {
                left: strip_x,
                top: m2_y + 46,
                right: strip_x + strip_w,
                bottom: m2_y + 68,
            };
            let _ = DrawTextW(hdc, &mut m2_hint, &mut m2_hint_rc, DT_SINGLELINE | DT_VCENTER);

            let cursor_on = CAPTURE_CURSOR.load(Ordering::Relaxed);
            let cur_labels = [
                "☑  SHOW REMOTE CURSOR (ENABLED)",
                "☐  HIDE REMOTE CURSOR (DISABLED)",
            ];
            draw_segmented_strip(
                strip_x,
                m2_y + 74,
                strip_w,
                42,
                &cur_labels,
                if cursor_on { 0 } else { 1 },
            );
        }

        _ => {
            // =================================================================
            // CATEGORY 3: CONNECTION STATUS (Direct P2P Architecture & Diagnostics)
            // =================================================================
            let m1_y = content_y;
            let m1_h = 260;
            draw_module_panel(
                m1_y,
                m1_h,
                "NETWORK",
                "END-TO-END DIRECT P2P UDP TRANSPORT DIAGNOSTICS",
            );

            let local_id = {
                let guard = LOCAL_DESKY_ID.lock().unwrap();
                if !guard.is_empty() {
                    guard.clone()
                } else {
                    "Assigned on Host".to_string()
                }
            };

            let rows = [
                ("TRANSPORT ARCHITECTURE", "100% Direct Peer-to-Peer UDP (Zero VDS Relay)"),
                ("VDS MATCHMAKER ROLE", "Lightweight ID Lookup & NAT Hole-Punching Only"),
                ("LOCAL DESKY ID", local_id.as_str()),
                ("REMOTE DESKTOP RESOLUTION", res_str.as_str()),
                ("LIVE STREAM BITRATE", bitrate_str.as_str()),
                ("MEASURED ROUND-TRIP PING", &format!("{} ms", rtt_ms)),
            ];

            for (i, (k, v)) in rows.iter().enumerate() {
                let ry = m1_y + 46 + (i as i32) * 34;
                let row_bg = if i % 2 == 0 { PK_INPUT_BG } else { PK_CARD };
                draw_rounded_box(hdc, strip_x, ry, strip_w, 34, row_bg, PK_CARD_BORDER, 0);

                let _ = SelectObject(hdc, HGDIOBJ(font_sec.0));
                SetTextColor(hdc, PK_MUTED);
                let mut kw: Vec<u16> = k.encode_utf16().collect();
                let mut k_rc = RECT {
                    left: strip_x + 14,
                    top: ry,
                    right: strip_x + 250,
                    bottom: ry + 34,
                };
                let _ = DrawTextW(hdc, &mut kw, &mut k_rc, DT_SINGLELINE | DT_VCENTER);

                let _ = SelectObject(hdc, HGDIOBJ(font_btn.0));
                SetTextColor(hdc, PK_GREEN_DARK);
                let mut vw: Vec<u16> = v.encode_utf16().collect();
                let mut v_rc = RECT {
                    left: strip_x + 260,
                    top: ry,
                    right: strip_x + strip_w - 14,
                    bottom: ry + 34,
                };
                let _ = DrawTextW(hdc, &mut vw, &mut v_rc, DT_SINGLELINE | DT_VCENTER);
            }
        }
    }

    // =========================================================================
    // BOTTOM INTEGRATED STATUS & ACTION BAR (Docked at bottom of workspace)
    // =========================================================================
    let footer_h = 46;
    let footer_y = (client_h - footer_h - 20).max(content_y + 390);
    draw_rounded_box(hdc, work_x, footer_y, work_w, footer_h, PK_CARD, PK_CARD_BORDER, 0);

    let apply_w = 270;
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
    let mut done_txt: Vec<u16> = "✓  APPLY & RETURN TO STREAM".encode_utf16().collect();
    let mut done_rc = apply_rc;
    let _ = DrawTextW(hdc, &mut done_txt, &mut done_rc, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    let _ = SelectObject(hdc, HGDIOBJ(font_info.0));
    SetTextColor(hdc, PK_MUTED);
    let mut hint_txt: Vec<u16> =
        "SHORTCUT: Press [Tab] or [Esc] anytime to toggle this Decky Settings Dashboard"
            .encode_utf16()
            .collect();
    let mut hint_rc = RECT {
        left: work_x + 16,
        top: footer_y,
        right: apply_x - 12,
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

/// Spawns the dedicated remote desktop viewport presentation window with a decoupled background render thread
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

        let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        let _ = ShowWindow(hwnd, SW_SHOW);
        info!("Remote Viewport Window successfully spawned! (HWND: {:?})", hwnd);

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
                                        format!("⚡ Connecting to Remote Desk {} ...", id),
                                        COLORREF(0x00EDB363),
                                    ),
                                    SessionStatus::WaitingApproval(id) => (
                                        format!("🔔 Waiting for Remote Desk {} to accept your connection request...", id),
                                        COLORREF(0x0078BB48),
                                    ),
                                    SessionStatus::Error(err) => (
                                        format!("❌ {}  (Press Esc to close)", err),
                                        COLORREF(0x006060FF),
                                    ),
                                    _ => (
                                        "⚡ Waiting for remote video stream...".to_string(),
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

                            let rtt = Duration::from_millis(crate::p2p::get_session_rtt_ms() as u64);
                            if toolbar_visible {
                                draw_toolbar(mem_dc, client_w, TOOLBAR_HEIGHT, mode, target_fps, rtt);
                            } else {
                                draw_rounded_box(mem_dc, client_w - 112, 4, 102, 26, PK_CARD, PK_GREEN, 6);
                                SetBkMode(mem_dc, TRANSPARENT);
                                SetTextColor(mem_dc, PK_GREEN_DARK);
                                let mut tab_text: Vec<u16> = "⚙ Settings".encode_utf16().collect();
                                let mut tr = RECT { left: client_w - 112, top: 4, right: client_w - 10, bottom: 30 };
                                let _ = DrawTextW(mem_dc, &mut tab_text, &mut tr, DT_SINGLELINE | DT_CENTER | DT_VCENTER);
                            }
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
                        logs.push("• Ready to connect".into());
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
                    let f_title = (26.0 * l.font_scale) as i32;
                    let f_sub = (14.0 * l.font_scale) as i32;
                    let f_badge = (13.0 * l.font_scale) as i32;
                    let f_id = (34.0 * l.font_scale) as i32;
                    let f_btn = (16.0 * l.font_scale) as i32;
                    let f_console = (13.0 * l.font_scale) as i32;

                    let font_title = create_segoe_font(f_title, FW_BOLD);
                    let font_sub = create_segoe_font(f_sub, FW_NORMAL);
                    let font_badge = create_segoe_font(f_badge, FW_BOLD);
                    let font_id = create_segoe_font(f_id, FW_BOLD);
                    let font_btn = create_segoe_font(f_btn, FW_BOLD);
                    let font_console = create_segoe_font(f_console, FW_NORMAL);

                    // Header
                    let old_font = SelectObject(mem_dc, HGDIOBJ(font_title.0));
                    SetTextColor(mem_dc, PK_GREEN_DARK);
                    let mut title_text: Vec<u16> = "⚡ DESKY".encode_utf16().collect();
                    let mut title_rect = RECT { left: 24, top: 14, right: 260, bottom: 14 + f_title + 6 };
                    let _ = DrawTextW(mem_dc, &mut title_text, &mut title_rect, DT_SINGLELINE | DT_VCENTER);

                    let _ = SelectObject(mem_dc, HGDIOBJ(font_sub.0));
                    SetTextColor(mem_dc, PK_MUTED);
                    let mut sub_text: Vec<u16> = "Remote Desktop".encode_utf16().collect();
                    let mut sub_rect = RECT { left: 24, top: 14 + f_title + 2, right: 300, bottom: 14 + f_title + 2 + f_sub + 6 };
                    let _ = DrawTextW(mem_dc, &mut sub_text, &mut sub_rect, DT_SINGLELINE | DT_VCENTER);

                    let pill_w = (94.0 * l.font_scale) as i32;
                    let pill_h = (30.0 * l.font_scale) as i32;
                    let pill_x = (w - pill_w - 24).max(24);
                    draw_rounded_box(mem_dc, pill_x, 16, pill_w, pill_h, PK_GREEN_TINT, PK_CARD_BORDER, pill_h / 2);
                    let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
                    SetTextColor(mem_dc, PK_GREEN);
                    let mut pill_text: Vec<u16> = "● Online".encode_utf16().collect();
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
                        let mut b_txt: Vec<u16> = "🔔 INCOMING CONNECTION REQUEST".encode_utf16().collect();
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

                        // [ ✓ Accept ] and [ ✕ Decline ] buttons
                        let btn_w = (mw - 72) / 2;
                        let btn_h = 50;
                        let btn_y = my + 190;
                        let acc_x = mx + 24;
                        let dec_x = mx + 48 + btn_w;

                        let acc_bg = if hover == 3 { PK_GREEN_DARK } else { PK_GREEN };
                        draw_rounded_box(mem_dc, acc_x, btn_y, btn_w, btn_h, acc_bg, acc_bg, 10);
                        let _ = SelectObject(mem_dc, HGDIOBJ(font_btn.0));
                        SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
                        let mut acc_txt: Vec<u16> = "✓  Accept Connection".encode_utf16().collect();
                        let mut acc_rc = RECT { left: acc_x, top: btn_y, right: acc_x + btn_w, bottom: btn_y + btn_h };
                        let _ = DrawTextW(mem_dc, &mut acc_txt, &mut acc_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                        draw_rounded_box(mem_dc, dec_x, btn_y, btn_w, btn_h, PK_RED, PK_RED, 10);
                        SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
                        let mut dec_txt: Vec<u16> = "✕  Decline".encode_utf16().collect();
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
                        let b_x = l.b_x;
                        let b_y = l.b_y;
                        let b_w = l.b_w;
                        let b_h = l.b_h;
                        let console_box_x = l.console_box_x;
                        let console_box_y = l.console_box_y;
                        let console_box_w = l.console_box_w;
                        let console_box_h = l.console_box_h;

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

                        // 3. Card 3: ACTIVITY (Bottom)
                        draw_rounded_box(mem_dc, b_x, b_y, b_w, b_h, PK_CARD, PK_CARD_BORDER, 12);
                        let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
                        SetTextColor(mem_dc, PK_MUTED);
                        let mut console_hdr: Vec<u16> = "ACTIVITY".encode_utf16().collect();
                        let mut console_hdr_rect = RECT { left: b_x + 18, top: b_y + 10, right: b_x + 200, bottom: b_y + 10 + f_badge + 6 };
                        let _ = DrawTextW(mem_dc, &mut console_hdr, &mut console_hdr_rect, DT_SINGLELINE | DT_VCENTER);

                        draw_rounded_box(mem_dc, console_box_x, console_box_y, console_box_w, console_box_h, PK_INPUT_BG, PK_CARD_BORDER, 8);
                        let _ = SelectObject(mem_dc, HGDIOBJ(font_console.0));
                        SetTextColor(mem_dc, PK_MUTED);

                        let line_spacing = (22.0 * l.font_scale) as i32;
                        let logs = STATUS_LOGS.lock().unwrap().clone();
                        for (i, line) in logs.iter().enumerate() {
                            let mut line_wide: Vec<u16> = line.encode_utf16().collect();
                            let mut line_rect = RECT {
                                left: console_box_x + 12,
                                top: console_box_y + 8 + (i as i32 * line_spacing),
                                right: console_box_x + console_box_w - 12,
                                bottom: console_box_y + 8 + ((i + 1) as i32 * line_spacing),
                            };
                            let _ = DrawTextW(mem_dc, &mut line_wide, &mut line_rect, DT_SINGLELINE | DT_VCENTER);
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
                        append_status_log(&format!("✅ Desky ID '{}' copied to Windows Clipboard!", id_to_copy));
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
                        append_status_log("⚠️ Please enter a 9-digit remote Desky ID (e.g. 800 258 756)");
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
                        append_status_log("⚠️ That is THIS computer's own ID! Please enter the remote computer's ID.");
                        return LRESULT(0);
                    }

                    let target_display = if digits.len() == 9 {
                        format!("{} {} {}", &digits[0..3], &digits[3..6], &digits[6..9])
                    } else {
                        digits.clone()
                    };

                    info!("Initiating P2P session to remote Desky ID: {}", target_display);
                    append_status_log(&format!("⚡ Requesting session with Remote Desky ID: {}...", target_display));

                    crate::p2p::start_client_session(
                        &target_display,
                        TARGET_FPS.load(Ordering::Relaxed),
                        CAPTURE_CURSOR.load(Ordering::Relaxed),
                    );

                    spawn_viewport_window(&target_display);
                }
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

/// Window Procedure for the Viewport Remote Stream Canvas
unsafe extern "system" fn viewport_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_LBUTTONDOWN => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let toolbar_visible = SHOW_TOOLBAR.load(Ordering::Relaxed);

            let mut rect = RECT::default();
            let _ = unsafe { GetClientRect(hwnd, &mut rect) };
            let client_w = rect.right - rect.left;
            let client_h = rect.bottom - rect.top;

            // 1. If Full-Screen Settings Page is open, handle all Decky/AnyDesk settings controls!
            if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                let hdr_h = 52;
                let side_w = 250;
                let back_w = 220;
                let back_x = (client_w - back_w).max(side_w + 20);

                // Top-right Flush Integrated [ ← BACK TO STREAM ] button
                if client_x >= back_x && client_x <= client_w && client_y >= 0 && client_y <= hdr_h
                {
                    SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
                    return LRESULT(0);
                }

                // Left Integrated Navigation Dock Tabs (0..4)
                let tab_start_y = hdr_h + 36;
                let tab_h = 52;
                if client_x >= 0 && client_x < side_w {
                    for i in 0..4 {
                        let item_y = tab_start_y + (i as i32) * tab_h;
                        if client_y >= item_y && client_y < item_y + tab_h {
                            SETTINGS_CATEGORY.store(i as u8, Ordering::Relaxed);
                            return LRESULT(0);
                        }
                    }
                }

                // Right Workspace Geometry
                let work_x = side_w + 28;
                let work_w = (client_w - work_x - 28).clamp(420, 960);
                let work_y = hdr_h + 24;
                let content_y = work_y + 62;
                let strip_x = work_x + 20;
                let strip_w = work_w - 40;

                let active_cat = SETTINGS_CATEGORY.load(Ordering::Relaxed);

                // Helper to hit-test a segmented strip at (strip_x, strip_y, strip_w, 42)
                let hit_segment = |strip_y: i32, count: i32| -> Option<usize> {
                    if client_x >= strip_x
                        && client_x <= strip_x + strip_w
                        && client_y >= strip_y
                        && client_y <= strip_y + 42
                    {
                        let seg_w = (strip_w / count.max(1)).max(1);
                        let idx = ((client_x - strip_x) / seg_w).clamp(0, count - 1) as usize;
                        Some(idx)
                    } else {
                        None
                    }
                };

                let m1_y = content_y;
                let m1_h = 132;
                let m2_y = m1_y + m1_h + 16;

                match active_cat {
                    0 => {
                        // Category 0: Display & Quality
                        // Strip 1 (m1_y + 74): 3 Viewport Scaling segments
                        if let Some(idx) = hit_segment(m1_y + 74, 3) {
                            DISPLAY_MODE.store(idx as u8, Ordering::Relaxed);
                            return LRESULT(0);
                        }
                        // Strip 2 (m2_y + 74): 5 Quality / ABR segments
                        if let Some(idx) = hit_segment(m2_y + 74, 5) {
                            crate::p2p::set_quality_mode(idx as u8);
                            info!("Settings: Quality / ABR mode set to {}", idx);
                            return LRESULT(0);
                        }
                    }
                    1 => {
                        // Category 1: Adaptive Bitrate (ABR)
                        // Strip 1 (m1_y + 74): 5 ABR Profile segments
                        if let Some(idx) = hit_segment(m1_y + 74, 5) {
                            crate::p2p::set_quality_mode(idx as u8);
                            info!("Settings: Quality / ABR mode set to {}", idx);
                            return LRESULT(0);
                        }
                    }
                    2 => {
                        // Category 2: Input & Framerate
                        // Strip 1 (m1_y + 74): 3 Target FPS segments (30 / 60 / 120)
                        if let Some(idx) = hit_segment(m1_y + 74, 3) {
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
                        // Strip 2 (m2_y + 74): 2 Remote Cursor segments (0 = Enabled, 1 = Disabled)
                        if let Some(idx) = hit_segment(m2_y + 74, 2) {
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

                // Bottom Integrated [ ✓ APPLY & RETURN TO STREAM ] button
                let footer_h = 46;
                let footer_y = (client_h - footer_h - 20).max(content_y + 390);
                let apply_w = 270;
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

            // 2. Integrated Toolbar clicks when settings page is closed
            if toolbar_visible && client_y < TOOLBAR_HEIGHT {
                if client_x >= client_w - 216 && client_x < client_w - 104 {
                    SHOW_SETTINGS_MODAL.store(true, Ordering::Relaxed);
                    return LRESULT(0);
                }
                if client_x >= client_w - 104 && client_x <= client_w {
                    SHOW_TOOLBAR.store(false, Ordering::Relaxed);
                    return LRESULT(0);
                }
                return LRESULT(0);
            } else if !toolbar_visible && client_y < 32 && client_x >= client_w - 114 {
                SHOW_SETTINGS_MODAL.store(true, Ordering::Relaxed);
                SHOW_TOOLBAR.store(true, Ordering::Relaxed);
                return LRESULT(0);
            }

            unsafe {
                inject_remote_mouse(hwnd, client_x, client_y, true, false);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let toolbar_h = if SHOW_TOOLBAR.load(Ordering::Relaxed) { TOOLBAR_HEIGHT } else { 0 };

            if !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) && client_y >= toolbar_h {
                unsafe {
                    inject_remote_mouse(hwnd, client_x, client_y, false, false);
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let toolbar_h = if SHOW_TOOLBAR.load(Ordering::Relaxed) { TOOLBAR_HEIGHT } else { 0 };

            if !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) && client_y >= toolbar_h {
                unsafe {
                    inject_remote_mouse(hwnd, client_x, client_y, true, true);
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let toolbar_h = if SHOW_TOOLBAR.load(Ordering::Relaxed) { TOOLBAR_HEIGHT } else { 0 };

            if !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) && client_y >= toolbar_h {
                unsafe {
                    inject_remote_mouse(hwnd, client_x, client_y, false, true);
                }
            }
            LRESULT(0)
        }
        WM_MBUTTONDOWN => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let toolbar_h = if SHOW_TOOLBAR.load(Ordering::Relaxed) { TOOLBAR_HEIGHT } else { 0 };

            if !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) && client_y >= toolbar_h {
                unsafe {
                    inject_remote_mouse_middle(hwnd, client_x, client_y, true);
                }
            }
            LRESULT(0)
        }
        WM_MBUTTONUP => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let toolbar_h = if SHOW_TOOLBAR.load(Ordering::Relaxed) { TOOLBAR_HEIGHT } else { 0 };

            if !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) && client_y >= toolbar_h {
                unsafe {
                    inject_remote_mouse_middle(hwnd, client_x, client_y, false);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let toolbar_visible = SHOW_TOOLBAR.load(Ordering::Relaxed);
            let modal_open = SHOW_SETTINGS_MODAL.load(Ordering::Relaxed);

            if !modal_open && (!toolbar_visible || client_y >= TOOLBAR_HEIGHT) {
                unsafe {
                    inject_remote_mouse_move(hwnd, client_x, client_y);
                }
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let hit_test = (lparam.0 & 0xffff) as i16 as i32;
            if hit_test == 1 {
                let toolbar_visible = SHOW_TOOLBAR.load(Ordering::Relaxed);
                let modal_open = SHOW_SETTINGS_MODAL.load(Ordering::Relaxed);
                let mut cursor_pt = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut cursor_pt);
                    let _ = ScreenToClient(hwnd, &mut cursor_pt);
                    if modal_open || (toolbar_visible && cursor_pt.y < TOOLBAR_HEIGHT) {
                        let hand_cursor = LoadCursorW(None, IDC_HAND).unwrap_or_default();
                        SetCursor(Some(hand_cursor));
                    } else {
                        let arrow_cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
                        SetCursor(Some(arrow_cursor));
                    }
                    return LRESULT(1);
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_MOUSEWHEEL => {
            if !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                let delta = ((wparam.0 >> 16) & 0xffff) as i16 as i32;
                crate::p2p::send_remote_input(InputEvent::MouseWheel { delta });
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wparam.0 == 0x1B {
                if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                    SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
                } else {
                    VIEWPORT_RUNNING.store(false, Ordering::SeqCst);
                    crate::p2p::stop_client_session();
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                }
            } else if wparam.0 == 0x09 {
                let prev = SHOW_SETTINGS_MODAL.fetch_xor(true, Ordering::Relaxed);
                info!("Full-Screen Settings Page toggled: {}", !prev);
            } else if !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                unsafe {
                    inject_remote_key(wparam.0 as u8, false);
                }
            }
            LRESULT(0)
        }
        WM_KEYUP => {
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
            VIEWPORT_RUNNING.store(false, Ordering::SeqCst);
            crate::p2p::stop_client_session();
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn copy_to_clipboard(text: &str) {
    unsafe {
        if OpenClipboard(None).is_ok() {
            let _ = EmptyClipboard();
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes_len = wide.len() * 2;

            if let Ok(handle) = GlobalAlloc(GHND, bytes_len) {
                let ptr = GlobalLock(handle);
                if !ptr.is_null() {
                    std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr as *mut u8, bytes_len);
                    let _ = GlobalUnlock(handle);
                    let _ = SetClipboardData(13, Some(windows::Win32::Foundation::HANDLE(handle.0)));
                }
            }
            let _ = CloseClipboard();
        }
    }
}
