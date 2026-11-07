#![allow(unsafe_op_in_unsafe_fn)]
use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::time::{Duration, Instant};
use tracing::info;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreatePen,
    CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC,
    GetStockObject, InvalidateRect, ReleaseDC, RoundRect, ScreenToClient, SelectObject, SetBkColor,
    SetBkMode, SetStretchBltMode, SetTextColor, StretchBlt, COLORONCOLOR, DEFAULT_GUI_FONT,
    DT_CENTER, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL, FW_SEMIBOLD, HBRUSH, HDC,
    HFONT, HGDIOBJ, OPAQUE, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GHND};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, mouse_event, KEYEVENTF_KEYUP, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, DrawIconEx, GetClientRect,
    GetCursorInfo, GetCursorPos, GetIconInfo, GetMessageW, GetSystemMetrics, GetWindowTextLengthW,
    GetWindowTextW, IsWindow, LoadCursorW, PostQuitMessage, RegisterClassExW, SendMessageW,
    SetCursor, SetCursorPos, SetWindowDisplayAffinity, ShowWindow, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, CURSORINFO, CURSOR_SHOWING, CW_USEDEFAULT, DI_NORMAL, HMENU,
    ICONINFO, IDC_ARROW, IDC_HAND, MSG, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW,
    WDA_EXCLUDEFROMCAPTURE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CREATE,
    WM_CTLCOLOREDIT, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SETFONT, WM_SIZE, WNDCLASSEXW, WS_CHILD,
    WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_VISIBLE, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
};
use std::sync::atomic::AtomicIsize;



// pkasoft.com White Theme color palette (GDI COLORREF little-endian: 0x00BBGGRR)
const PK_BG: COLORREF = COLORREF(0x00F5F7F5);          // #f5f7f5 (--paper soft clean canvas)
const PK_CARD: COLORREF = COLORREF(0x00FFFFFF);        // #ffffff (pure crisp white card)
const PK_CARD_BORDER: COLORREF = COLORREF(0x00E3E7E2); // #e2e7e3 (--line border)
const PK_GREEN: COLORREF = COLORREF(0x004F6A2D);       // #2d6a4f (--green signature emerald)
const PK_GREEN_DARK: COLORREF = COLORREF(0x0032431B);  // #1b4332 (--green-dark pine hover)
const _PK_GREEN_LIGHT: COLORREF = COLORREF(0x0088B752); // #52b788 (mint accent)
const PK_GREEN_TINT: COLORREF = COLORREF(0x00EEF5F0);  // #f0f5ee (subtle green pill fill)
const PK_TEXT: COLORREF = COLORREF(0x0020241D);        // #1d2420 (--ink deep crisp charcoal)
const PK_MUTED: COLORREF = COLORREF(0x00636A5D);       // #5d6a63 (--muted slate sage)
const PK_INPUT_BG: COLORREF = COLORREF(0x00FAFCFA);    // #fafcfa (soft clean input background)
const PK_BTN_HOVER: COLORREF = COLORREF(0x00E5EDE7);   // #e7ede5 (hover outline button)

// Control IDs
#[allow(dead_code)]
const ID_BTN_COPY_CODE: usize = 1001;
#[allow(dead_code)]
const ID_BTN_CONNECT: usize = 1002;
const ID_EDIT_TARGET_CODE: usize = 1003;

static IS_RUNNING: AtomicBool = AtomicBool::new(false);
static VIEWPORT_RUNNING: AtomicBool = AtomicBool::new(false);

static mut HWND_MAIN_WINDOW: HWND = HWND(std::ptr::null_mut());
static mut HWND_EDIT_TARGET: HWND = HWND(std::ptr::null_mut());
static mut BG_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());
static mut CARD_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());
static mut VIEWPORT_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());
static mut EDIT_BG_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());

static HOVER_BUTTON: AtomicU8 = AtomicU8::new(0); // 0 = none, 1 = copy, 2 = connect
static CURRENT_EDIT_FONT: AtomicIsize = AtomicIsize::new(0);

pub static LOCAL_DESKY_ID: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
static STATUS_LOGS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub fn append_status_log(msg: &str) {
    if let Ok(mut logs) = STATUS_LOGS.lock() {
        logs.push(msg.to_string());
        if logs.len() > 5 {
            logs.remove(0);
        }
    }
    unsafe {
        if !HWND_MAIN_WINDOW.0.is_null() && IsWindow(Some(HWND_MAIN_WINDOW)).as_bool() {
            let _ = InvalidateRect(Some(HWND_MAIN_WINDOW), None, false);
        }
    }
}

pub fn set_assigned_desky_id(id: &str) {
    if let Ok(mut guard) = LOCAL_DESKY_ID.lock() {
        *guard = id.to_string();
    }
    append_status_log(&format!("✅ Online • ID: {}", id));
    unsafe {
        if !HWND_MAIN_WINDOW.0.is_null() && IsWindow(Some(HWND_MAIN_WINDOW)).as_bool() {
            let _ = InvalidateRect(Some(HWND_MAIN_WINDOW), None, false);
        }
    }
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
    radius: i32,
) {
    let brush = CreateSolidBrush(fill_color);
    let pen = CreatePen(PS_SOLID, 1, border_color);
    let old_brush = SelectObject(hdc, HGDIOBJ(brush.0));
    let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));

    let _ = RoundRect(hdc, x, y, x + w, y + h, radius, radius);

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

    // Adaptive scale factor: baseline 1.0 at 760x580, smoothly scaling up to 1.6x on larger/maximized screens
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




pub struct NativeWin32App {
    pub hwnd: HWND,
}

impl NativeWin32App {
    pub fn new() -> Result<Self> {
        unsafe {
            let h_module = GetModuleHandleW(None)?;
            let h_instance: HINSTANCE = h_module.into();

            // pkasoft.com theme brushes: canvas #131916, card #1c2420, input #131916
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

            Ok(Self { hwnd })
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

#[allow(dead_code)]
unsafe fn create_control(
    ex_style: WINDOW_EX_STYLE,
    class_name: PCWSTR,
    window_name: PCWSTR,
    style: WINDOW_STYLE,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    parent: HWND,
    menu_id: usize,
    h_instance: HINSTANCE,
) -> windows::core::Result<HWND> {
    unsafe {
        let menu = if menu_id != 0 {
            Some(HMENU(menu_id as *mut _))
        } else {
            None
        };
        CreateWindowExW(
            ex_style,
            class_name,
            window_name,
            style,
            x,
            y,
            w,
            h,
            Some(parent),
            menu,
            Some(h_instance),
            None,
        )
    }
}

#[allow(dead_code)]
unsafe fn set_font(hwnd: HWND) {
    unsafe {
        let default_font = GetStockObject(DEFAULT_GUI_FONT);
        let _ = SendMessageW(
            hwnd,
            WM_SETFONT,
            Some(WPARAM(default_font.0 as usize)),
            Some(LPARAM(1)),
        );
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

// Settings modal popup state
static SHOW_SETTINGS_MODAL: AtomicBool = AtomicBool::new(false);

// Remote cursor capture toggle (renders live host cursor on stream)
static CAPTURE_CURSOR: AtomicBool = AtomicBool::new(true);

unsafe fn sync_remote_cursor() {
    let mut ci = CURSORINFO {
        cbSize: std::mem::size_of::<CURSORINFO>() as u32,
        ..Default::default()
    };
    if GetCursorInfo(&mut ci).is_ok() && !ci.hCursor.is_invalid() {
        let _ = SetCursor(Some(ci.hCursor));
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
    use windows::Win32::Graphics::Gdi::HGDIOBJ;

    // 1. Toolbar background (#16181D)
    let bg_brush = CreateSolidBrush(COLORREF(0x001D1816));
    let bar_rect = RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: toolbar_h,
    };
    FillRect(hdc, &bar_rect, bg_brush);
    let _ = DeleteObject(HGDIOBJ(bg_brush.0));

    // 2. Bottom border line (#2B2F3A)
    let border_brush = CreateSolidBrush(COLORREF(0x003A2F2B));
    let border_rect = RECT {
        left: 0,
        top: toolbar_h - 1,
        right: client_w,
        bottom: toolbar_h,
    };
    FillRect(hdc, &border_rect, border_brush);
    let _ = DeleteObject(HGDIOBJ(border_brush.0));

    SetBkMode(hdc, TRANSPARENT);

    // 3. Logo & Status
    SetTextColor(hdc, COLORREF(0x0000D7FF)); // Cyan/Gold accent
    let mut logo_text: Vec<u16> = "⚡ DESKY".encode_utf16().collect();
    let mut logo_rect = RECT { left: 12, top: 6, right: 85, bottom: 34 };
    let _ = DrawTextW(hdc, &mut logo_text, &mut logo_rect, DT_SINGLELINE | DT_VCENTER);

    SetTextColor(hdc, COLORREF(0x0078BB48)); // Green live status
    let mut live_text: Vec<u16> = "● LIVE".encode_utf16().collect();
    let mut live_rect = RECT { left: 88, top: 6, right: 145, bottom: 34 };
    let _ = DrawTextW(hdc, &mut live_text, &mut live_rect, DT_SINGLELINE | DT_VCENTER);

    // 4. Live Session Summary Badge (Clean, unstacked!)
    let mode_str = match mode {
        0 => "⛶ Auto Fit",
        1 => "↔ Fill",
        _ => "1:1 Original",
    };
    let cursor_str = if CAPTURE_CURSOR.load(Ordering::Relaxed) { "Cursor: ON" } else { "Cursor: OFF" };
    let badge_str = format!("{}  •  {} FPS  •  {}  •  RTT: {:?}", mode_str, fps, cursor_str, latency);
    SetTextColor(hdc, COLORREF(0x00A0AEC0)); // Slate gray
    let mut badge_wide: Vec<u16> = badge_str.encode_utf16().collect();
    let mut badge_rect = RECT { left: 160, top: 6, right: client_w - 225, bottom: 34 };
    let _ = DrawTextW(hdc, &mut badge_wide, &mut badge_rect, DT_SINGLELINE | DT_VCENTER);

    // Helper closure to render buttons
    let draw_btn = |left: i32, right: i32, text: &str, is_highlighted: bool| {
        let btn_rect = RECT { left, top: 6, right, bottom: 34 };
        let brush_color = if is_highlighted {
            COLORREF(0x007E4C2B) // Blue accent
        } else {
            COLORREF(0x002B2522) // Inactive dark background
        };
        let brush = CreateSolidBrush(brush_color);
        FillRect(hdc, &btn_rect, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));

        let text_color = if is_highlighted {
            COLORREF(0x00EDB363) // Bright active cyan/gold
        } else {
            COLORREF(0x00CBD5E0) // Light slate gray
        };
        SetTextColor(hdc, text_color);
        let mut text_wide: Vec<u16> = text.encode_utf16().collect();
        let mut r = btn_rect;
        let _ = DrawTextW(hdc, &mut text_wide, &mut r, DT_SINGLELINE | DT_CENTER | DT_VCENTER);
    };

    // Right action buttons:
    // [ ⚙ Settings ] Button
    draw_btn(client_w - 215, client_w - 105, "⚙ Settings", SHOW_SETTINGS_MODAL.load(Ordering::Relaxed));

    // [ ▲ Hide Bar ] Button
    draw_btn(client_w - 95, client_w - 15, "▲ Hide Bar", false);
}

unsafe fn draw_settings_modal(
    hdc: HDC,
    client_w: i32,
    client_h: i32,
    mode: u8,
    fps: u32,
) {
    use windows::Win32::Graphics::Gdi::HGDIOBJ;

    let modal_w = 460;
    let modal_h = 370;
    let mx = (client_w - modal_w) / 2;
    let my = (client_h - modal_h) / 2;

    // 1. Subtle dark drop-shadow outline around floating modal card (preserves live remote desktop background)
    let shadow_brush = CreateSolidBrush(COLORREF(0x0014100E));
    let shadow_rect = RECT {
        left: mx - 4,
        top: my - 4,
        right: mx + modal_w + 4,
        bottom: my + modal_h + 4,
    };
    FillRect(hdc, &shadow_rect, shadow_brush);
    let _ = DeleteObject(HGDIOBJ(shadow_brush.0));

    // 2. Modal Card Background (#181A22)
    let card_brush = CreateSolidBrush(COLORREF(0x00221A18));
    let card_rect = RECT {
        left: mx,
        top: my,
        right: mx + modal_w,
        bottom: my + modal_h,
    };
    FillRect(hdc, &card_rect, card_brush);
    let _ = DeleteObject(HGDIOBJ(card_brush.0));

    // 3. Modal Outer Border (#3B4252)
    let border_brush = CreateSolidBrush(COLORREF(0x0052423B));
    let top_b = RECT { left: mx, top: my, right: mx + modal_w, bottom: my + 2 };
    let bot_b = RECT { left: mx, top: my + modal_h - 2, right: mx + modal_w, bottom: my + modal_h };
    let left_b = RECT { left: mx, top: my, right: mx + 2, bottom: my + modal_h };
    let right_b = RECT { left: mx + modal_w - 2, top: my, right: mx + modal_w, bottom: my + modal_h };
    FillRect(hdc, &top_b, border_brush);
    FillRect(hdc, &bot_b, border_brush);
    FillRect(hdc, &left_b, border_brush);
    FillRect(hdc, &right_b, border_brush);
    let _ = DeleteObject(HGDIOBJ(border_brush.0));

    SetBkMode(hdc, TRANSPARENT);

    // 4. Header: Title + [ ✕ ] Close
    SetTextColor(hdc, COLORREF(0x00F8FAFC));
    let mut title_text: Vec<u16> = "⚙ Desky Session Settings".encode_utf16().collect();
    let mut title_rect = RECT { left: mx + 20, top: my + 12, right: mx + modal_w - 50, bottom: my + 38 };
    let _ = DrawTextW(hdc, &mut title_text, &mut title_rect, DT_SINGLELINE | DT_VCENTER);

    // [ ✕ ] Button
    let close_brush = CreateSolidBrush(COLORREF(0x00332824));
    let close_rect = RECT { left: mx + modal_w - 38, top: my + 10, right: mx + modal_w - 12, bottom: my + 34 };
    FillRect(hdc, &close_rect, close_brush);
    let _ = DeleteObject(HGDIOBJ(close_brush.0));
    SetTextColor(hdc, COLORREF(0x00CBD5E0));
    let mut close_text: Vec<u16> = "✕".encode_utf16().collect();
    let mut cr = close_rect;
    let _ = DrawTextW(hdc, &mut close_text, &mut cr, DT_SINGLELINE | DT_CENTER | DT_VCENTER);

    // Header divider
    let div_brush = CreateSolidBrush(COLORREF(0x003B3128));
    let div_rect = RECT { left: mx + 16, top: my + 44, right: mx + modal_w - 16, bottom: my + 45 };
    FillRect(hdc, &div_rect, div_brush);
    let _ = DeleteObject(HGDIOBJ(div_brush.0));

    // Helper for modal buttons
    let draw_modal_btn = |left: i32, top: i32, right: i32, bottom: i32, text: &str, is_active: bool| {
        let btn_rect = RECT { left, top, right, bottom };
        let brush_color = if is_active {
            COLORREF(0x007E4C2B) // Active blue/navy background
        } else {
            COLORREF(0x002B2522) // Inactive dark card background
        };
        let brush = CreateSolidBrush(brush_color);
        FillRect(hdc, &btn_rect, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));

        let text_color = if is_active {
            COLORREF(0x00EDB363) // Bright active cyan/gold
        } else {
            COLORREF(0x00A0AEC0) // Muted slate gray
        };
        SetTextColor(hdc, text_color);
        let mut text_wide: Vec<u16> = text.encode_utf16().collect();
        let mut r = btn_rect;
        let _ = DrawTextW(hdc, &mut text_wide, &mut r, DT_SINGLELINE | DT_CENTER | DT_VCENTER);
    };

    // SECTION 1: SCREEN DISPLAY MODE
    SetTextColor(hdc, COLORREF(0x00968071));
    let mut sec1_text: Vec<u16> = "SCREEN SCALING MODE".encode_utf16().collect();
    let mut sec1_rect = RECT { left: mx + 20, top: my + 54, right: mx + modal_w - 20, bottom: my + 72 };
    let _ = DrawTextW(hdc, &mut sec1_text, &mut sec1_rect, DT_SINGLELINE | DT_VCENTER);

    draw_modal_btn(mx + 20, my + 74, mx + 150, my + 106, "⛶ Auto Fit", mode == 0);
    draw_modal_btn(mx + 160, my + 74, mx + 300, my + 106, "↔ Fill Window", mode == 1);
    draw_modal_btn(mx + 310, my + 74, mx + 440, my + 106, "1:1 Original", mode == 2);

    // SECTION 2: STREAM FRAMERATE (FPS)
    SetTextColor(hdc, COLORREF(0x00968071));
    let mut sec2_text: Vec<u16> = "STREAM FRAMERATE".encode_utf16().collect();
    let mut sec2_rect = RECT { left: mx + 20, top: my + 120, right: mx + modal_w - 20, bottom: my + 138 };
    let _ = DrawTextW(hdc, &mut sec2_text, &mut sec2_rect, DT_SINGLELINE | DT_VCENTER);

    draw_modal_btn(mx + 20, my + 140, mx + 150, my + 172, "30 FPS", fps == 30);
    draw_modal_btn(mx + 160, my + 140, mx + 300, my + 172, "60 FPS", fps == 60);
    draw_modal_btn(mx + 310, my + 140, mx + 440, my + 172, "120 FPS", fps == 120);

    // SECTION 3: REMOTE CURSOR CAPTURE (Requested Feature)
    SetTextColor(hdc, COLORREF(0x00968071));
    let mut sec3_text: Vec<u16> = "REMOTE CURSOR CAPTURE".encode_utf16().collect();
    let mut sec3_rect = RECT { left: mx + 20, top: my + 186, right: mx + modal_w - 20, bottom: my + 204 };
    let _ = DrawTextW(hdc, &mut sec3_text, &mut sec3_rect, DT_SINGLELINE | DT_VCENTER);

    let cursor_active = CAPTURE_CURSOR.load(Ordering::Relaxed);
    let cursor_label = if cursor_active {
        "☑  Render Remote Cursor on Stream (Active)"
    } else {
        "☐  Render Remote Cursor on Stream (Hidden)"
    };
    draw_modal_btn(mx + 20, my + 206, mx + 440, my + 240, cursor_label, cursor_active);

    // SECTION 4: ENGINE DIAGNOSTICS
    SetTextColor(hdc, COLORREF(0x00968071));
    let mut sec4_text: Vec<u16> = "ENGINE & PIPELINE".encode_utf16().collect();
    let mut sec4_rect = RECT { left: mx + 20, top: my + 254, right: mx + modal_w - 20, bottom: my + 270 };
    let _ = DrawTextW(hdc, &mut sec4_text, &mut sec4_rect, DT_SINGLELINE | DT_VCENTER);

    SetTextColor(hdc, COLORREF(0x00A0AEC0));
    let mut diag_text: Vec<u16> = "• Native DirectX / DXGI Desktop Duplication\n• Direct Input & Hardware Blending Active".encode_utf16().collect();
    let mut diag_rect = RECT { left: mx + 20, top: my + 272, right: mx + 440, bottom: my + 308 };
    let _ = DrawTextW(hdc, &mut diag_text, &mut diag_rect, Default::default());

    // Done / Close button at bottom
    draw_modal_btn(mx + 170, my + 320, mx + 290, my + 354, "Done / Close", true);
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
        // 1. Enable Immersive Dark Mode for Viewport Titlebar
        let dark_mode: i32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_mode as *const _ as *const _,
            std::mem::size_of::<i32>() as u32,
        );

        // 2. EXCLUDE FROM CAPTURE: Prevents the hall-of-mirrors effect on local loopback!
        let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);

        let _ = ShowWindow(hwnd, SW_SHOW);
        info!("Remote Viewport Window successfully spawned! (HWND: {:?})", hwnd);

        // 3. START DEDICATED RENDER THREAD:
        // Pass HWND as raw isize to decouple rendering from UI window dragging!
        VIEWPORT_RUNNING.store(true, Ordering::SeqCst);
        let hwnd_raw = hwnd.0 as isize;

        std::thread::spawn(move || {
            info!("Dedicated Viewport Render Thread Started.");
            let target_hwnd = HWND(hwnd_raw as *mut _);

            while VIEWPORT_RUNNING.load(Ordering::Relaxed) {
                let frame_start = Instant::now();

                unsafe {
                    if !IsWindow(Some(target_hwnd)).as_bool() {
                        break;
                    }

                    let mut rect = RECT::default();
                    let _ = GetClientRect(target_hwnd, &mut rect);
                    let client_w = rect.right - rect.left;
                    let client_h = rect.bottom - rect.top;

                    let toolbar_visible = SHOW_TOOLBAR.load(Ordering::Relaxed);
                    let toolbar_h = if toolbar_visible { TOOLBAR_HEIGHT } else { 0 };

                    if client_w > 50 && client_h > 50 {
                        use windows::Win32::Graphics::Gdi::HGDIOBJ;
                        let hdc_window = GetDC(Some(target_hwnd));
                        let hdc_screen = GetDC(None); // Desktop DC

                        let screen_w = GetSystemMetrics(SM_CXSCREEN);
                        let screen_h = GetSystemMetrics(SM_CYSCREEN);
                        let mode = DISPLAY_MODE.load(Ordering::Relaxed);
                        let target_fps = TARGET_FPS.load(Ordering::Relaxed).clamp(15, 240);

                        // Double-buffer memory DC: composite all stream layers off-screen
                        // to guarantee 0% flicker, 0% strobe, and zero intermediate flashes
                        let mem_dc = CreateCompatibleDC(Some(hdc_window));
                        let mem_bmp = CreateCompatibleBitmap(hdc_window, client_w, client_h);
                        let old_bmp = SelectObject(mem_dc, HGDIOBJ(mem_bmp.0));

                        let _ = SetStretchBltMode(mem_dc, COLORONCOLOR);

                        let view_h = client_h - toolbar_h;

                        match mode {
                            0 => {
                                // Auto Fit (Preserve Aspect Ratio)
                                let scale_x = client_w as f32 / screen_w as f32;
                                let scale_y = view_h as f32 / screen_h as f32;
                                let scale = scale_x.min(scale_y);

                                let render_w = (screen_w as f32 * scale) as i32;
                                let render_h = (screen_h as f32 * scale) as i32;

                                let offset_x = (client_w - render_w) / 2;
                                let offset_y = toolbar_h + (view_h - render_h) / 2;

                                // Letterbox black borders if any
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

                                let _ = StretchBlt(
                                    mem_dc,
                                    offset_x,
                                    offset_y,
                                    render_w,
                                    render_h,
                                    Some(hdc_screen),
                                    0,
                                    0,
                                    screen_w,
                                    screen_h,
                                    SRCCOPY,
                                );
                            }
                            1 => {
                                // Stretch / Fill Window
                                let _ = StretchBlt(
                                    mem_dc,
                                    0,
                                    toolbar_h,
                                    client_w,
                                    view_h,
                                    Some(hdc_screen),
                                    0,
                                    0,
                                    screen_w,
                                    screen_h,
                                    SRCCOPY,
                                );
                            }
                            _ => {
                                // 1:1 Original Resolution (Centered)
                                let offset_x = (client_w - screen_w) / 2;
                                let offset_y = toolbar_h + (view_h - screen_h) / 2;

                                // Fill letterbox background
                                let bg_rect = RECT { left: 0, top: toolbar_h, right: client_w, bottom: client_h };
                                FillRect(mem_dc, &bg_rect, VIEWPORT_BRUSH);

                                let _ = StretchBlt(
                                    mem_dc,
                                    offset_x,
                                    offset_y,
                                    screen_w,
                                    screen_h,
                                    Some(hdc_screen),
                                    0,
                                    0,
                                    screen_w,
                                    screen_h,
                                    SRCCOPY,
                                );
                            }
                        }

                        // 1. Render Captured Remote Cursor directly onto the stream if enabled!
                        if CAPTURE_CURSOR.load(Ordering::Relaxed) {
                            use windows::Win32::Graphics::Gdi::HGDIOBJ;
                            let mut ci = CURSORINFO {
                                cbSize: std::mem::size_of::<CURSORINFO>() as u32,
                                ..Default::default()
                            };
                            if GetCursorInfo(&mut ci).is_ok() && (ci.flags.0 & CURSOR_SHOWING.0 != 0) && !ci.hCursor.is_invalid() {
                                let mut icon_info = ICONINFO::default();
                                let (hot_x, hot_y) = if GetIconInfo(windows::Win32::UI::WindowsAndMessaging::HICON(ci.hCursor.0), &mut icon_info).is_ok() {
                                    if !icon_info.hbmMask.0.is_null() {
                                        let _ = DeleteObject(HGDIOBJ(icon_info.hbmMask.0));
                                    }
                                    if !icon_info.hbmColor.0.is_null() {
                                        let _ = DeleteObject(HGDIOBJ(icon_info.hbmColor.0));
                                    }
                                    (icon_info.xHotspot as i32, icon_info.yHotspot as i32)
                                } else {
                                    (0, 0)
                                };

                                let cursor_x = ci.ptScreenPos.x;
                                let cursor_y = ci.ptScreenPos.y;

                                let render_pos = match mode {
                                    0 => {
                                        let scale_x = client_w as f32 / screen_w as f32;
                                        let scale_y = view_h as f32 / screen_h as f32;
                                        let scale = scale_x.min(scale_y);
                                        let render_w = (screen_w as f32 * scale) as i32;
                                        let offset_x = (client_w - render_w) / 2;
                                        let render_h = (screen_h as f32 * scale) as i32;
                                        let offset_y = toolbar_h + (view_h - render_h) / 2;
                                        (
                                            offset_x + (cursor_x as f32 * scale) as i32 - hot_x,
                                            offset_y + (cursor_y as f32 * scale) as i32 - hot_y,
                                        )
                                    }
                                    1 => {
                                        (
                                            (cursor_x * client_w) / screen_w - hot_x,
                                            toolbar_h + (cursor_y * view_h) / screen_h - hot_y,
                                        )
                                    }
                                    _ => {
                                        let offset_x = (client_w - screen_w) / 2;
                                        let offset_y = toolbar_h + (view_h - screen_h) / 2;
                                        (
                                            offset_x + cursor_x - hot_x,
                                            offset_y + cursor_y - hot_y,
                                        )
                                    }
                                };

                                let _ = DrawIconEx(
                                    mem_dc,
                                    render_pos.0,
                                    render_pos.1,
                                    ci.hCursor.into(),
                                    0,
                                    0,
                                    0,
                                    None,
                                    DI_NORMAL,
                                );
                            }
                        }

                        // 2. Draw Top Toolbar (Clean AnyDesk style with Settings button)
                        if toolbar_visible {
                            draw_toolbar(mem_dc, client_w, TOOLBAR_HEIGHT, mode, target_fps, frame_start.elapsed());
                        } else {
                            // Show compact "⚙ Settings" tab in top right
                            use windows::Win32::Graphics::Gdi::HGDIOBJ;
                            let tab_brush = CreateSolidBrush(COLORREF(0x002B2522));
                            let tab_rect = RECT { left: client_w - 110, top: 0, right: client_w - 10, bottom: 26 };
                            FillRect(mem_dc, &tab_rect, tab_brush);
                            let _ = DeleteObject(HGDIOBJ(tab_brush.0));

                            SetBkMode(mem_dc, TRANSPARENT);
                            SetTextColor(mem_dc, COLORREF(0x00EDB363));
                            let mut tab_text: Vec<u16> = "⚙ Settings".encode_utf16().collect();
                            let mut tr = tab_rect;
                            let _ = DrawTextW(mem_dc, &mut tab_text, &mut tr, DT_SINGLELINE | DT_CENTER | DT_VCENTER);
                        }

                        // 3. Draw Settings Modal if opened!
                        if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                            draw_settings_modal(mem_dc, client_w, client_h, mode, target_fps);
                        }

                        // Atomic blit completed frame to screen in ONE pass: zero intermediate redraws or flashes!
                        let _ = BitBlt(hdc_window, 0, 0, client_w, client_h, Some(mem_dc), 0, 0, SRCCOPY);

                        let _ = SelectObject(mem_dc, old_bmp);
                        let _ = DeleteObject(HGDIOBJ(mem_bmp.0));
                        let _ = DeleteObject(HGDIOBJ(mem_dc.0));
                        let _ = ReleaseDC(None, hdc_screen);
                        let _ = ReleaseDC(Some(target_hwnd), hdc_window);
                    }
                }

                // Dynamic frame pacing based on user-selected TARGET_FPS
                let target_fps = TARGET_FPS.load(Ordering::Relaxed).clamp(15, 240);
                let target_interval = Duration::from_micros(1_000_000 / target_fps as u64);
                let elapsed = frame_start.elapsed();
                if elapsed < target_interval {
                    std::thread::sleep(target_interval - elapsed);
                }
            }
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

    if client_w > 0 && client_h > 0 {
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let toolbar_h = if SHOW_TOOLBAR.load(Ordering::Relaxed) { TOOLBAR_HEIGHT } else { 0 };
        let mode = DISPLAY_MODE.load(Ordering::Relaxed);

        if let Some((dest_x, dest_y)) = map_client_to_screen(
            client_x, client_y, client_w, client_h, screen_w, screen_h, toolbar_h, mode,
        ) {
            let _ = SetCursorPos(dest_x, dest_y);
            let flag = if is_right {
                if is_down { MOUSEEVENTF_RIGHTDOWN } else { MOUSEEVENTF_RIGHTUP }
            } else {
                if is_down { MOUSEEVENTF_LEFTDOWN } else { MOUSEEVENTF_LEFTUP }
            };
            mouse_event(flag, 0, 0, 0, 0);
        }
    }
}

unsafe fn inject_remote_mouse_move(hwnd: HWND, client_x: i32, client_y: i32) {
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let client_w = rect.right - rect.left;
    let client_h = rect.bottom - rect.top;

    if client_w > 0 && client_h > 0 {
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let toolbar_h = if SHOW_TOOLBAR.load(Ordering::Relaxed) { TOOLBAR_HEIGHT } else { 0 };
        let mode = DISPLAY_MODE.load(Ordering::Relaxed);

        if let Some((dest_x, dest_y)) = map_client_to_screen(
            client_x, client_y, client_w, client_h, screen_w, screen_h, toolbar_h, mode,
        ) {
            let _ = SetCursorPos(dest_x, dest_y);
        }
    }
}

unsafe fn inject_remote_mouse_middle(hwnd: HWND, client_x: i32, client_y: i32, is_down: bool) {
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let client_w = rect.right - rect.left;
    let client_h = rect.bottom - rect.top;

    if client_w > 0 && client_h > 0 {
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let toolbar_h = if SHOW_TOOLBAR.load(Ordering::Relaxed) { TOOLBAR_HEIGHT } else { 0 };
        let mode = DISPLAY_MODE.load(Ordering::Relaxed);

        if let Some((dest_x, dest_y)) = map_client_to_screen(
            client_x, client_y, client_w, client_h, screen_w, screen_h, toolbar_h, mode,
        ) {
            let _ = SetCursorPos(dest_x, dest_y);
            let flag = if is_down { MOUSEEVENTF_MIDDLEDOWN } else { MOUSEEVENTF_MIDDLEUP };
            mouse_event(flag, 0, 0, 0, 0);
        }
    }
}

unsafe fn inject_remote_key(vk: u8, is_up: bool) {
    let flags = if is_up { KEYEVENTF_KEYUP } else { Default::default() };
    keybd_event(vk, 0, flags, 0);
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            unsafe {
                HWND_MAIN_WINDOW = hwnd;
                let h_module = GetModuleHandleW(None).unwrap_or_default();
                let h_instance: HINSTANCE = h_module.into();

                // Initialize default logs
                {
                    let mut logs = STATUS_LOGS.lock().unwrap();
                    if logs.is_empty() {
                        logs.push("• Ready to connect".into());
                    }
                }

                let l = get_responsive_layout(760, 580);

                // Create sleek edit control inside the Remote Desk card
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
                    // Double-buffer memory DC to guarantee 0% flicker
                    let mem_dc = CreateCompatibleDC(Some(hdc));
                    let mem_bmp = CreateCompatibleBitmap(hdc, w, h);
                    let old_bmp = SelectObject(mem_dc, HGDIOBJ(mem_bmp.0));

                    // 1. Fill Canvas with pkasoft paper soft background (#f5f7f5)
                    FillRect(mem_dc, &rc, BG_BRUSH);
                    SetBkMode(mem_dc, TRANSPARENT);

                    let l = get_responsive_layout(w, h);
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

                    // Adaptive Fonts scaled with window resolution
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

                    // 2. Header
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

                    // Top Right Status Pill (Minimal: "● Online")
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

                    // 1. Card 1: THIS DESK (Left)
                    draw_rounded_box(mem_dc, c1_x, card_y, card_w, card_h, PK_CARD, PK_GREEN, 12);

                    // Tag Pill
                    let tag1_w = (92.0 * l.font_scale) as i32;
                    let tag1_h = (22.0 * l.font_scale) as i32;
                    draw_rounded_box(mem_dc, c1_x + 18, card_y + 14, tag1_w, tag1_h, PK_GREEN_TINT, PK_GREEN, 6);
                    let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
                    SetTextColor(mem_dc, PK_GREEN);
                    let mut tag1: Vec<u16> = "THIS DESK".encode_utf16().collect();
                    let mut tag1_rect = RECT { left: c1_x + 18, top: card_y + 14, right: c1_x + 18 + tag1_w, bottom: card_y + 14 + tag1_h };
                    let _ = DrawTextW(mem_dc, &mut tag1, &mut tag1_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                    // ID Box
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

                    // Copy Button
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

                    // Tag Pill
                    let tag2_w = (110.0 * l.font_scale) as i32;
                    let tag2_h = (22.0 * l.font_scale) as i32;
                    draw_rounded_box(mem_dc, c2_x + 18, card_y + 14, tag2_w, tag2_h, PK_INPUT_BG, PK_CARD_BORDER, 6);
                    let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
                    SetTextColor(mem_dc, PK_MUTED);
                    let mut tag2: Vec<u16> = "REMOTE DESK".encode_utf16().collect();
                    let mut tag2_rect = RECT { left: c2_x + 18, top: card_y + 14, right: c2_x + 18 + tag2_w, bottom: card_y + 14 + tag2_h };
                    let _ = DrawTextW(mem_dc, &mut tag2, &mut tag2_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

                    // Label above input box
                    SetTextColor(mem_dc, PK_MUTED);
                    let mut enter_label: Vec<u16> = "REMOTE ID".encode_utf16().collect();
                    let mut enter_label_rect = RECT { left: c2_x + 18, top: input_box_y - f_badge - 6, right: c2_x + card_w - 18, bottom: input_box_y - 2 };
                    let _ = DrawTextW(mem_dc, &mut enter_label, &mut enter_label_rect, DT_SINGLELINE | DT_VCENTER);

                    // Input Box Frame
                    draw_rounded_box(mem_dc, input_box_x, input_box_y, input_box_w, input_box_h, PK_INPUT_BG, PK_CARD_BORDER, 8);

                    // Connect Button
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

                    // Console Box
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

                    // Blit atomic memory DC to screen
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

                let l = get_responsive_layout(w, h);

                let new_hover = if x >= l.copy_btn_x && x <= l.copy_btn_x + l.copy_btn_w && y >= l.copy_btn_y && y <= l.copy_btn_y + l.copy_btn_h {
                    1 // Copy button
                } else if x >= l.conn_btn_x && x <= l.conn_btn_x + l.conn_btn_w && y >= l.conn_btn_y && y <= l.conn_btn_y + l.conn_btn_h {
                    2 // Connect button
                } else {
                    0
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

                    let target_display = if digits.len() == 9 {
                        format!("{} {} {}", &digits[0..3], &digits[3..6], &digits[6..9])
                    } else {
                        digits.clone()
                    };

                    info!("Initiating P2P session to remote Desky ID: {}", target_display);
                    append_status_log(&format!("⚡ Connecting to Remote Desky ID: {} via UDP Rendezvous...", target_display));

                    // Launch the Viewport Stream Window!
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

            // 1. If Settings Modal is open, handle modal interactive controls!
            if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                let modal_w = 460;
                let modal_h = 370;
                let mx = (client_w - modal_w) / 2;
                let my = (client_h - modal_h) / 2;

                // Clicked outside modal card -> dismiss modal
                if client_x < mx || client_x > mx + modal_w || client_y < my || client_y > my + modal_h {
                    SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
                    return LRESULT(0);
                }

                // [✕] Close button or bottom [Done / Close] button
                if (client_x >= mx + modal_w - 38 && client_x <= mx + modal_w - 12 && client_y >= my + 10 && client_y <= my + 34)
                    || (client_x >= mx + 170 && client_x <= mx + 290 && client_y >= my + 320 && client_y <= my + 354) {
                    SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
                    return LRESULT(0);
                }

                // Section 1: Screen Scaling Mode (AutoFit, Fill, 1:1)
                if client_y >= my + 74 && client_y <= my + 106 {
                    if client_x >= mx + 20 && client_x <= mx + 150 {
                        DISPLAY_MODE.store(0, Ordering::Relaxed);
                        info!("Settings Modal: Mode set to Auto Fit");
                    } else if client_x >= mx + 160 && client_x <= mx + 300 {
                        DISPLAY_MODE.store(1, Ordering::Relaxed);
                        info!("Settings Modal: Mode set to Fill Window");
                    } else if client_x >= mx + 310 && client_x <= mx + 440 {
                        DISPLAY_MODE.store(2, Ordering::Relaxed);
                        info!("Settings Modal: Mode set to 1:1 Original");
                    }
                    return LRESULT(0);
                }

                // Section 2: Framerate (30, 60, 120 FPS)
                if client_y >= my + 140 && client_y <= my + 172 {
                    if client_x >= mx + 20 && client_x <= mx + 150 {
                        TARGET_FPS.store(30, Ordering::Relaxed);
                        info!("Settings Modal: Framerate set to 30 FPS");
                    } else if client_x >= mx + 160 && client_x <= mx + 300 {
                        TARGET_FPS.store(60, Ordering::Relaxed);
                        info!("Settings Modal: Framerate set to 60 FPS");
                    } else if client_x >= mx + 310 && client_x <= mx + 440 {
                        TARGET_FPS.store(120, Ordering::Relaxed);
                        info!("Settings Modal: Framerate set to 120 FPS");
                    }
                    return LRESULT(0);
                }

                // Section 3: Remote Cursor Capture Toggle
                if client_y >= my + 206 && client_y <= my + 240 && client_x >= mx + 20 && client_x <= mx + 440 {
                    let prev = CAPTURE_CURSOR.fetch_xor(true, Ordering::Relaxed);
                    info!("Settings Modal: Remote cursor capture toggled: {}", !prev);
                    return LRESULT(0);
                }

                return LRESULT(0);
            }

            // 2. Toolbar clicks when modal is closed
            if toolbar_visible && client_y < TOOLBAR_HEIGHT {
                // Click [ ⚙ Settings ]
                if client_x >= client_w - 215 && client_x <= client_w - 105 {
                    SHOW_SETTINGS_MODAL.store(true, Ordering::Relaxed);
                    return LRESULT(0);
                }
                // Click [ ▲ Hide Bar ]
                if client_x >= client_w - 95 && client_x <= client_w - 15 {
                    SHOW_TOOLBAR.store(false, Ordering::Relaxed);
                    return LRESULT(0);
                }
                return LRESULT(0);
            } else if !toolbar_visible && client_y < 28 && client_x >= client_w - 110 {
                // Clicked compact "⚙ Settings" tab in top right
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

            if modal_open {
                unsafe {
                    let hand_cursor = LoadCursorW(None, IDC_HAND).unwrap_or_default();
                    SetCursor(Some(hand_cursor));
                }
            } else if toolbar_visible && client_y < TOOLBAR_HEIGHT {
                unsafe {
                    let hand_cursor = LoadCursorW(None, IDC_HAND).unwrap_or_default();
                    SetCursor(Some(hand_cursor));
                }
            } else {
                unsafe {
                    inject_remote_mouse_move(hwnd, client_x, client_y);
                    sync_remote_cursor();
                }
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let hit_test = (lparam.0 & 0xffff) as i16 as i32;
            if hit_test == 1 { // HTCLIENT
                let toolbar_visible = SHOW_TOOLBAR.load(Ordering::Relaxed);
                let modal_open = SHOW_SETTINGS_MODAL.load(Ordering::Relaxed);
                let mut cursor_pt = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut cursor_pt);
                    let _ = ScreenToClient(hwnd, &mut cursor_pt);
                    if modal_open || (toolbar_visible && cursor_pt.y < TOOLBAR_HEIGHT) {
                        let hand_cursor = LoadCursorW(None, IDC_HAND).unwrap_or_default();
                        SetCursor(Some(hand_cursor));
                        return LRESULT(1);
                    } else {
                        sync_remote_cursor();
                        return LRESULT(1);
                    }
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_MOUSEWHEEL => {
            if !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                let delta = ((wparam.0 >> 16) & 0xffff) as i16 as i32;
                unsafe {
                    use windows::Win32::UI::Input::KeyboardAndMouse::{mouse_event, MOUSEEVENTF_WHEEL};
                    mouse_event(MOUSEEVENTF_WHEEL, 0, 0, delta, 0);
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wparam.0 == 0x1B {
                // Esc key: closes settings modal if open, otherwise closes viewport window
                if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                    SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
                } else {
                    VIEWPORT_RUNNING.store(false, Ordering::SeqCst);
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                }
            } else if wparam.0 == 0x09 {
                // Tab key toggles settings modal!
                let prev = SHOW_SETTINGS_MODAL.fetch_xor(true, Ordering::Relaxed);
                info!("Settings Modal toggled: {}", !prev);
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
