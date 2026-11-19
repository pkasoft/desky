//! Main Win32 Launcher Window & Application Lifecycle
//!
//! Exposes responsive Home page rendering, layout calculations, and
//! application lifecycle for the unified Desky window.

#![allow(unsafe_op_in_unsafe_fn)]
use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU8, Ordering};
use std::time::Duration;
use tracing::info;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetTextExtentPoint32W, InvalidateRect,
    SelectObject, SetBkMode, SetTextColor, DT_CALCRECT, DT_CENTER, DT_LEFT, DT_SINGLELINE, DT_VCENTER,
    DT_WORDBREAK, FW_BOLD, FW_NORMAL, FW_SEMIBOLD, HBRUSH, HDC, HGDIOBJ, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DispatchMessageW, GetMessageW, GetWindowTextLengthW, GetWindowTextW,
    IsWindow, IsWindowVisible, LoadCursorW, PostMessageW, RegisterClassExW, SendMessageW, SetCursor,
    SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, IDC_ARROW, IDC_HAND, MSG, SWP_NOACTIVATE,
    SWP_NOZORDER, SW_HIDE, SW_SHOW, WINDOW_EX_STYLE, WM_SETFONT,
    WNDCLASSEXW, WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

use super::theme::{
    copy_to_clipboard, create_segoe_font, draw_rounded_box, PK_BG, PK_BTN_HOVER, PK_CARD,
    PK_CARD_BORDER, PK_GREEN, PK_GREEN_DARK, PK_GREEN_TINT, PK_INPUT_BG, PK_MUTED, PK_TEXT,
};
use super::viewport::{viewport_wndproc, VIEWPORT_BRUSH};

pub const WM_APP_REDRAW: u32 = 0x8001;
pub const WM_APP_BRING_FRONT: u32 = 0x8002;
pub const WM_APP_SHOW_PROGRESS: u32 = 0x8004;
pub const WM_APP_SYNC_HOST_UI: u32 = 0x8005;

pub static UI_THREAD_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub const ID_EDIT_TARGET_CODE: usize = 1003;

static IS_RUNNING: AtomicBool = AtomicBool::new(false);

pub(crate) static mut HWND_MAIN_WINDOW: HWND = HWND(std::ptr::null_mut());
pub(crate) static mut HWND_EDIT_TARGET: HWND = HWND(std::ptr::null_mut());
static mut BG_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());
static mut EDIT_BG_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());

static HOVER_BUTTON: AtomicU8 = AtomicU8::new(0); // 0 = none, 1 = copy, 2 = connect, 3 = accept, 4 = decline
static CURRENT_EDIT_FONT: AtomicIsize = AtomicIsize::new(0);

pub static LOCAL_DESKY_ID: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
pub static BOTTOM_TAB_RADAR: AtomicBool = AtomicBool::new(false);
pub static BOTTOM_CARD3_TAB: AtomicU8 = AtomicU8::new(0); // 0 = Recent Sessions, 1 = Address Book, 2 = LAN Radar
static STATUS_LOGS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub fn request_main_redraw() {
    unsafe {
        if !HWND_MAIN_WINDOW.0.is_null() && IsWindow(Some(HWND_MAIN_WINDOW)).as_bool() {
            let _ = InvalidateRect(Some(HWND_MAIN_WINDOW), None, false);
            let _ = PostMessageW(Some(HWND_MAIN_WINDOW), WM_APP_REDRAW, WPARAM(0), LPARAM(0));
        }
    }
}

#[allow(dead_code)]
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
        if logs.len() > 12 {
            logs.remove(0);
        }
    }
    request_main_redraw();
}

/// Retrieves the current system local time formatted as `HH:MM:SS`
pub fn get_local_time_string() -> String {
    unsafe {
        let st = windows::Win32::System::SystemInformation::GetLocalTime();
        format!("{:02}:{:02}:{:02}", st.wHour, st.wMinute, st.wSecond)
    }
}

static LAN_SCAN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

pub fn is_lan_scan_active() -> bool {
    LAN_SCAN_IN_PROGRESS.load(Ordering::Relaxed)
}

/// Initiates an active LAN broadcast ping and logs the process, time, and peer discovery results
pub fn trigger_lan_radar_scan() {
    if LAN_SCAN_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return;
    }

    let start_time = get_local_time_string();
    append_status_log(&format!("LAN Radar [{}]: Broadcast scan initiated...", start_time));
    crate::p2p::trigger_lan_ping();

    std::thread::spawn(move || {
        // Collect UDP broadcast responses over 1.6 seconds
        std::thread::sleep(Duration::from_millis(1600));
        LAN_SCAN_IN_PROGRESS.store(false, Ordering::SeqCst);

        let peers = crate::p2p::get_lan_peers();
        let finish_time = get_local_time_string();
        if peers.is_empty() {
            append_status_log(&format!(
                "LAN Radar [{}]: Scan complete - No devices found on local subnet.",
                finish_time
            ));
        } else {
            let names: Vec<String> = peers.iter().take(3).map(|p| p.hostname.clone()).collect();
            let summary = if peers.len() > 3 {
                format!("{} and {} more", names.join(", "), peers.len() - 3)
            } else {
                names.join(", ")
            };
            append_status_log(&format!(
                "LAN Radar [{}]: Scan complete - Found {} device(s) ({})",
                finish_time,
                peers.len(),
                summary
            ));
        }
        request_main_redraw();
    });
}

pub fn prompt_incoming_request(client_id: &str) {
    append_status_log(&format!(
        "Incoming request from ID: {} - Host approval prompt opened.",
        client_id
    ));
    unsafe {
        if !HWND_MAIN_WINDOW.0.is_null() && IsWindow(Some(HWND_MAIN_WINDOW)).as_bool() {
            let _ = PostMessageW(
                Some(HWND_MAIN_WINDOW),
                WM_APP_SYNC_HOST_UI,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

pub fn set_assigned_desky_id(id: &str) {
    if let Ok(mut guard) = LOCAL_DESKY_ID.lock() {
        *guard = id.to_string();
    }
    unsafe {
        if !HWND_MAIN_WINDOW.0.is_null() {
            crate::windows::tray::update_tray_tooltip(
                HWND_MAIN_WINDOW,
                &format!("Desky - ID: {}", id),
            );
        }
    }
    append_status_log(&format!("Online | ID: {}", id));
    request_main_redraw();
}

pub fn get_hover_button() -> u8 {
    HOVER_BUTTON.load(Ordering::Relaxed)
}

pub fn set_main_window_hwnd(hwnd: HWND) {
    unsafe {
        HWND_MAIN_WINDOW = hwnd;
        UI_THREAD_ID.store(
            windows::Win32::System::Threading::GetCurrentThreadId(),
            Ordering::SeqCst,
        );
    }
}

pub fn set_edit_target_hwnd(hwnd: HWND) {
    unsafe {
        HWND_EDIT_TARGET = hwnd;
    }
}

pub fn set_edit_target_visibility(visible: bool) {
    unsafe {
        if !HWND_EDIT_TARGET.0.is_null() && IsWindow(Some(HWND_EDIT_TARGET)).as_bool() {
            let is_currently_visible = IsWindowVisible(HWND_EDIT_TARGET).as_bool();
            if is_currently_visible != visible {
                let _ = ShowWindow(HWND_EDIT_TARGET, if visible { SW_SHOW } else { SW_HIDE });
            }
        }
    }
}

/// Single source of truth for whether the native "Remote Desk" edit box may be shown.
/// The edit is a real Win32 child window, so it always paints on top of GDI-drawn
/// overlays; it must be hidden whenever anything covers the Home tab card.
pub fn should_show_edit_target() -> bool {
    use crate::windows::ui::viewport::{ACTIVE_TAB_INDEX, SHOW_SETTINGS_MODAL};
    ACTIVE_TAB_INDEX.load(Ordering::Relaxed) == 0
        && !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed)
}

/// Re-applies the desired visibility of the edit box (no-op when already correct).
pub fn sync_edit_target_visibility() {
    set_edit_target_visibility(should_show_edit_target());
}

pub fn get_edit_bg_brush() -> HBRUSH {
    unsafe { EDIT_BG_BRUSH }
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

/// Computes responsive layout geometry for any window size with top tab bar offset
pub fn get_responsive_layout(w: i32, h: i32) -> ResponsiveLayout {
    let top_offset = 40;
    let margin = 24;
    let gap = 18;
    let card_w = (w - (2 * margin) - gap).max(220) / 2;

    let scale_w = w as f32 / 1024.0;
    let scale_h = h as f32 / 680.0;
    let font_scale = (scale_w.min(scale_h)).clamp(1.0, 1.4);

    let card_h = ((185.0 * font_scale) as i32).min((h - top_offset - 200) / 2).max(175);
    let card_y = top_offset + 14;

    let c1_x = margin;
    let c2_x = margin + card_w + gap;

    // Card 1: This Desk elements
    let id_box_x = c1_x + 18;
    let id_box_y = card_y + (46.0 * font_scale) as i32;
    let id_box_w = card_w - 36;
    let id_box_h = (64.0 * font_scale) as i32;

    let copy_btn_x = c1_x + 18;
    let copy_btn_y = id_box_y + id_box_h + (14.0 * font_scale) as i32;
    let copy_btn_w = card_w - 36;
    let copy_btn_h = (42.0 * font_scale) as i32;

    // Card 2: Remote Desk elements
    let input_box_x = c2_x + 18;
    let input_box_y = card_y + (46.0 * font_scale) as i32;
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
    let col_h = (h - col_y - margin).max(140);
    let col1_x = c1_x;
    let col2_x = c2_x;

    ResponsiveLayout {
        c1_x,
        c2_x,
        card_w,
        card_h,
        card_y,
        id_box_x,
        id_box_y,
        id_box_w,
        id_box_h,
        copy_btn_x,
        copy_btn_y,
        copy_btn_w,
        copy_btn_h,
        input_box_x,
        input_box_y,
        input_box_w,
        input_box_h,
        edit_x,
        edit_y,
        edit_w,
        edit_h,
        conn_btn_x,
        conn_btn_y,
        conn_btn_w,
        conn_btn_h,
        col1_x,
        col2_x,
        col_w,
        col_h,
        col_y,
        font_scale,
    }
}

pub fn get_card3_tab_rects(
    col1_x: i32,
    col_y: i32,
    font_scale: f32,
    _contacts_count: usize,
    _peers_count: usize,
) -> (
    (i32, i32, i32, i32, String),
    (i32, i32, i32, i32, String),
    (i32, i32, i32, i32, String),
) {
    let tab_h = (25.0 * font_scale) as i32;
    let tab_w = (34.0 * font_scale) as i32;
    let tab_y = col_y + 14;

    let t0_x = col1_x + 18;
    let t1_x = t0_x + tab_w + 6;
    let t2_x = t1_x + tab_w + 6;

    (
        (t0_x, tab_y, tab_w, tab_h, "Recents".to_string()),
        (t1_x, tab_y, tab_w, tab_h, "Contacts".to_string()),
        (t2_x, tab_y, tab_w, tab_h, "LAN Radar".to_string()),
    )
}

pub unsafe fn update_edit_layout(w: i32, h: i32) {
    if !HWND_EDIT_TARGET.0.is_null() && IsWindow(Some(HWND_EDIT_TARGET)).as_bool() {
        let l = get_responsive_layout(w, h);
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
}

pub struct NativeWin32App;

impl NativeWin32App {
    pub fn new() -> Result<Self> {
        unsafe {
            let h_module = GetModuleHandleW(None)?;
            let h_instance: HINSTANCE = h_module.into();

            BG_BRUSH = CreateSolidBrush(PK_BG);
            EDIT_BG_BRUSH = CreateSolidBrush(PK_INPUT_BG);
            VIEWPORT_BRUSH = CreateSolidBrush(COLORREF(0x000C0A09));

            // Register Viewport Window Class as the single primary application window class
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
                hbrBackground: BG_BRUSH,
                lpszMenuName: PCWSTR::null(),
                lpszClassName: viewport_class_name,
                hIconSm: Default::default(),
            };
            RegisterClassExW(&vwc);

            // Initialize Session Tabs with Tab 0 permanently as [ Home ]
            super::viewport::init_session_tabs();

            let start_in_tray = crate::identity::is_started_in_tray();
            let win_style = if start_in_tray {
                WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN
            } else {
                WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_CLIPCHILDREN
            };

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                viewport_class_name,
                w!("Desky | pkasoft.com"),
                win_style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                1024,
                680,
                None,
                None,
                Some(h_instance),
                None,
            )?;
            HWND_MAIN_WINDOW = hwnd;

            // Register native Win32 System Tray icon
            crate::windows::tray::add_tray_icon(hwnd);
            if start_in_tray {
                info!("Desky started silently in background System Tray (--tray)");
            }

            // Light theme titlebar
            let dark_mode: i32 = 0;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark_mode as *const _ as *const _,
                std::mem::size_of::<i32>() as u32,
            );

            // Start dedicated viewport render thread immediately for unified window
            super::viewport::start_viewport_render_thread(hwnd);

            info!("Native Win32 Unified Desky Window spawned successfully (HWND: {:?})", hwnd);
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

/// Renders the complete pkasoft.com White Theme Homepage onto the provided DC
pub unsafe fn draw_homepage_view(mem_dc: HDC, w: i32, h: i32, hover: u8) {
    let rc = RECT {
        left: 0,
        top: 40,
        right: w,
        bottom: h,
    };
    FillRect(mem_dc, &rc, BG_BRUSH);
    SetBkMode(mem_dc, TRANSPARENT);

    let l = get_responsive_layout(w, h);
    let f_sub = ((14.0 * l.font_scale) as i32).max(13);
    let f_badge = ((13.0 * l.font_scale) as i32).max(12);
    let f_id = ((34.0 * l.font_scale) as i32).max(28);
    let f_btn = ((16.0 * l.font_scale) as i32).max(14);
    let f_console = ((13.0 * l.font_scale) as i32).max(12);

    let font_sub = create_segoe_font(f_sub, FW_NORMAL);
    let font_badge = create_segoe_font(f_badge, FW_BOLD);
    let font_id = create_segoe_font(f_id, FW_BOLD);
    let font_btn = create_segoe_font(f_btn, FW_BOLD);
    let font_console = create_segoe_font(f_console, FW_NORMAL);

        // Normal Homepage Cards

        // 1. Card 1: THIS DESK (Left)
        draw_rounded_box(mem_dc, l.c1_x, l.card_y, l.card_w, l.card_h, PK_CARD, PK_GREEN, 12);

        let tag1_w = (92.0 * l.font_scale) as i32;
        let tag1_h = (22.0 * l.font_scale) as i32;
        draw_rounded_box(mem_dc, l.c1_x + 18, l.card_y + 14, tag1_w, tag1_h, PK_GREEN_TINT, PK_GREEN, 6);
        let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
        SetTextColor(mem_dc, PK_GREEN);
        let mut tag1: Vec<u16> = "THIS DESK".encode_utf16().collect();
        let mut tag1_rect = RECT {
            left: l.c1_x + 18,
            top: l.card_y + 14,
            right: l.c1_x + 18 + tag1_w,
            bottom: l.card_y + 14 + tag1_h,
        };
        let _ = DrawTextW(mem_dc, &mut tag1, &mut tag1_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

        draw_rounded_box(mem_dc, l.id_box_x, l.id_box_y, l.id_box_w, l.id_box_h, PK_INPUT_BG, PK_CARD_BORDER, 8);
        SetTextColor(mem_dc, PK_MUTED);
        let mut id_label: Vec<u16> = "YOUR ID".encode_utf16().collect();
        let mut id_label_rect = RECT {
            left: l.id_box_x + 14,
            top: l.id_box_y + 6,
            right: l.id_box_x + l.id_box_w,
            bottom: l.id_box_y + 6 + f_badge + 4,
        };
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
        let mut id_rect = RECT {
            left: l.id_box_x + 14,
            top: l.id_box_y + 4 + f_badge,
            right: l.id_box_x + l.id_box_w,
            bottom: l.id_box_y + l.id_box_h,
        };
        let _ = DrawTextW(mem_dc, &mut id_wide, &mut id_rect, DT_SINGLELINE | DT_VCENTER);

        let copy_bg = if hover == 1 { PK_BTN_HOVER } else { PK_CARD };
        let copy_border = if hover == 1 { PK_GREEN } else { PK_CARD_BORDER };
        draw_rounded_box(mem_dc, l.copy_btn_x, l.copy_btn_y, l.copy_btn_w, l.copy_btn_h, copy_bg, copy_border, 8);
        let copy_fg = if hover == 1 { PK_GREEN_DARK } else { PK_TEXT };

        let _ = SelectObject(mem_dc, HGDIOBJ(font_btn.0));
        SetTextColor(mem_dc, copy_fg);
        let mut copy_text: Vec<u16> = "Copy ID".encode_utf16().collect();
        let mut copy_sz = SIZE::default();
        let _ = GetTextExtentPoint32W(mem_dc, &copy_text, &mut copy_sz);

        let icon_sz = 16;
        let gap = 8;
        let copy_total_w = copy_sz.cx + gap + icon_sz;
        let copy_start_x = l.copy_btn_x + (l.copy_btn_w - copy_total_w) / 2;
        let copy_icon_y = l.copy_btn_y + (l.copy_btn_h - icon_sz) / 2;

        crate::ui::draw_copy(mem_dc, copy_start_x, copy_icon_y, icon_sz, copy_fg, 2);

        let mut copy_rect = RECT {
            left: copy_start_x + icon_sz + gap,
            top: l.copy_btn_y,
            right: copy_start_x + copy_total_w + 4,
            bottom: l.copy_btn_y + l.copy_btn_h,
        };
        let _ = DrawTextW(mem_dc, &mut copy_text, &mut copy_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

        // 2. Card 2: REMOTE DESK (Right)
        draw_rounded_box(mem_dc, l.c2_x, l.card_y, l.card_w, l.card_h, PK_CARD, PK_CARD_BORDER, 12);

        let tag2_w = (110.0 * l.font_scale) as i32;
        let tag2_h = (22.0 * l.font_scale) as i32;
        draw_rounded_box(mem_dc, l.c2_x + 18, l.card_y + 14, tag2_w, tag2_h, PK_INPUT_BG, PK_CARD_BORDER, 6);
        let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
        SetTextColor(mem_dc, PK_MUTED);
        let mut tag2: Vec<u16> = "REMOTE DESK".encode_utf16().collect();
        let mut tag2_rect = RECT {
            left: l.c2_x + 18,
            top: l.card_y + 14,
            right: l.c2_x + 18 + tag2_w,
            bottom: l.card_y + 14 + tag2_h,
        };
        let _ = DrawTextW(mem_dc, &mut tag2, &mut tag2_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

        draw_rounded_box(mem_dc, l.input_box_x, l.input_box_y, l.input_box_w, l.input_box_h, PK_INPUT_BG, PK_CARD_BORDER, 8);

        let conn_bg = if hover == 2 { PK_GREEN_DARK } else { PK_GREEN };
        draw_rounded_box(mem_dc, l.conn_btn_x, l.conn_btn_y, l.conn_btn_w, l.conn_btn_h, conn_bg, conn_bg, 8);

        let _ = SelectObject(mem_dc, HGDIOBJ(font_btn.0));
        SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
        let mut conn_text: Vec<u16> = "Connect".encode_utf16().collect();
        let mut conn_sz = SIZE::default();
        let _ = GetTextExtentPoint32W(mem_dc, &conn_text, &mut conn_sz);

        let conn_total_w = conn_sz.cx + gap + icon_sz;
        let conn_start_x = l.conn_btn_x + (l.conn_btn_w - conn_total_w) / 2;
        let conn_icon_y = l.conn_btn_y + (l.conn_btn_h - icon_sz) / 2;

        let mut conn_rect = RECT {
            left: conn_start_x,
            top: l.conn_btn_y,
            right: conn_start_x + conn_sz.cx,
            bottom: l.conn_btn_y + l.conn_btn_h,
        };
        let _ = DrawTextW(mem_dc, &mut conn_text, &mut conn_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER);

        let conn_icon_x = conn_start_x + conn_sz.cx + gap;
        crate::ui::draw_arrow_right(mem_dc, conn_icon_x, conn_icon_y, icon_sz, COLORREF(0x00FFFFFF), 2);

        // 3. Card 3: RECENT SESSIONS & LAN RADAR (Bottom Left)
        draw_rounded_box(mem_dc, l.col1_x, l.col_y, l.col_w, l.col_h, PK_CARD, PK_CARD_BORDER, 12);

        let card3_tab = BOTTOM_CARD3_TAB.load(Ordering::Relaxed);
        let lan_peers = crate::p2p::get_lan_peers();
        let contacts = crate::identity::load_address_book();

        let (t0, t1, t2) = get_card3_tab_rects(
            l.col1_x,
            l.col_y,
            l.font_scale,
            contacts.len(),
            lan_peers.len(),
        );

        let icon_sz = (15.0 * l.font_scale) as i32;

        // Tag 1: RECENTS (Clock Icon Only)
        let (t0_x, t0_y, t0_w, t0_h, _) = t0;
        let (t0_bg, t0_border, t0_fg) = if card3_tab == 0 {
            (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
        } else if hover == 10 {
            (PK_BTN_HOVER, PK_CARD_BORDER, PK_TEXT)
        } else {
            (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
        };
        draw_rounded_box(mem_dc, t0_x, t0_y, t0_w, t0_h, t0_bg, t0_border, 6);
        let t0_icon_x = t0_x + (t0_w - icon_sz) / 2;
        let t0_icon_y = t0_y + (t0_h - icon_sz) / 2;
        crate::ui::draw_clock(mem_dc, t0_icon_x, t0_icon_y, icon_sz, t0_fg, 2);

        // Tag 2: CONTACTS (Users Icon Only)
        let (t1_x, t1_y, t1_w, t1_h, _) = t1;
        let (t1_bg, t1_border, t1_fg) = if card3_tab == 1 {
            (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
        } else if hover == 11 {
            (PK_BTN_HOVER, PK_CARD_BORDER, PK_TEXT)
        } else {
            (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
        };
        draw_rounded_box(mem_dc, t1_x, t1_y, t1_w, t1_h, t1_bg, t1_border, 6);
        let t1_icon_x = t1_x + (t1_w - icon_sz) / 2;
        let t1_icon_y = t1_y + (t1_h - icon_sz) / 2;
        crate::ui::draw_users(mem_dc, t1_icon_x, t1_icon_y, icon_sz, t1_fg, 2);

        // Tag 3: LAN RADAR (Radio/Radar Broadcast Icon Only)
        let (t2_x, t2_y, t2_w, t2_h, _) = t2;
        let (t2_bg, t2_border, t2_fg) = if card3_tab == 2 {
            (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
        } else if hover == 12 {
            (PK_BTN_HOVER, PK_CARD_BORDER, PK_TEXT)
        } else {
            (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
        };
        draw_rounded_box(mem_dc, t2_x, t2_y, t2_w, t2_h, t2_bg, t2_border, 6);
        let t2_icon_x = t2_x + (t2_w - icon_sz) / 2;
        let t2_icon_y = t2_y + (t2_h - icon_sz) / 2;
        crate::ui::draw_radio(mem_dc, t2_icon_x, t2_icon_y, icon_sz, t2_fg, 2);

        // Tooltip hint on hover
        if hover >= 10 && hover <= 12 {
            let (tip_x, tip_label) = match hover {
                10 => (t0_x, "Recents".to_string()),
                11 => (t1_x, if contacts.len() > 0 { format!("Contacts ({})", contacts.len()) } else { "Contacts".to_string() }),
                _ => (t2_x, if lan_peers.len() > 0 { format!("LAN Radar ({})", lan_peers.len()) } else { "LAN Radar".to_string() }),
            };
            let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
            let mut tip_utf16: Vec<u16> = tip_label.encode_utf16().collect();
            let mut tip_calc = RECT::default();
            let _ = DrawTextW(mem_dc, &mut tip_utf16, &mut tip_calc, DT_CALCRECT | DT_SINGLELINE);
            let tip_w = tip_calc.right - tip_calc.left + 16;
            let tip_h = (22.0 * l.font_scale) as i32;
            let tip_y = t0_y - tip_h - 4;
            draw_rounded_box(mem_dc, tip_x, tip_y, tip_w, tip_h, PK_TEXT, PK_TEXT, 5);
            SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
            let mut tip_rc = RECT { left: tip_x, top: tip_y, right: tip_x + tip_w, bottom: tip_y + tip_h };
            let _ = DrawTextW(mem_dc, &mut tip_utf16, &mut tip_rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }

        // When in Radar mode, show Scan button at top-right; in Address Book, show [+ Add]
        if card3_tab == 2 {
            let is_scanning = is_lan_scan_active();
            let scan_w = if is_scanning { 74 } else { 54 };
            let scan_x = l.col1_x + l.col_w - scan_w - 18;
            let (scan_bg, scan_border, scan_fg) = if is_scanning {
                (PK_GREEN_TINT, PK_GREEN, PK_GREEN_DARK)
            } else {
                (PK_INPUT_BG, PK_CARD_BORDER, PK_MUTED)
            };
            draw_rounded_box(mem_dc, scan_x, l.col_y + 14, scan_w, t0_h, scan_bg, scan_border, 6);
            let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
            SetTextColor(mem_dc, scan_fg);
            let mut scan_txt: Vec<u16> = (if is_scanning { "Scanning..." } else { "Scan" }).encode_utf16().collect();
            let mut scan_rc = RECT {
                left: scan_x,
                top: l.col_y + 14,
                right: scan_x + scan_w,
                bottom: l.col_y + 14 + t0_h,
            };
            let _ = DrawTextW(mem_dc, &mut scan_txt, &mut scan_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
        } else if card3_tab == 1 {
            let add_w = 54;
            let add_x = l.col1_x + l.col_w - add_w - 18;
            draw_rounded_box(mem_dc, add_x, l.col_y + 14, add_w, t0_h, PK_GREEN_TINT, PK_GREEN, 6);
            let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
            SetTextColor(mem_dc, PK_GREEN_DARK);
            let mut add_txt: Vec<u16> = "+ Add".encode_utf16().collect();
            let mut add_rc = RECT {
                left: add_x,
                top: l.col_y + 14,
                right: add_x + add_w,
                bottom: l.col_y + 14 + t0_h,
            };
            let _ = DrawTextW(mem_dc, &mut add_txt, &mut add_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
        }

        if card3_tab == 2 {
            // Render LAN Radar View
            if lan_peers.is_empty() {
                crate::ui::draw_radio(mem_dc, l.col1_x + 22, l.col_y + 50, 18, PK_GREEN, 2);

                let _ = SelectObject(mem_dc, HGDIOBJ(font_btn.0));
                SetTextColor(mem_dc, PK_TEXT);
                let empty_title = if is_lan_scan_active() {
                    "Scanning local subnet for Desky devices...".to_string()
                } else {
                    "No Desky devices found on local network".to_string()
                };
                let mut empty_msg1: Vec<u16> = empty_title.encode_utf16().collect();
                let mut empty_rc1 = RECT {
                    left: l.col1_x + 48,
                    top: l.col_y + 48,
                    right: l.col1_x + l.col_w - 20,
                    bottom: l.col_y + 70,
                };
                let _ = DrawTextW(mem_dc, &mut empty_msg1, &mut empty_rc1, DT_SINGLELINE | DT_VCENTER);

                let _ = SelectObject(mem_dc, HGDIOBJ(font_console.0));
                SetTextColor(mem_dc, PK_MUTED);
                let mut empty_msg2: Vec<u16> =
                    "Desky instances on your local Wi-Fi or subnet appear here with 1-click zero-cloud connect."
                        .encode_utf16()
                        .collect();
                let mut empty_rc2 = RECT {
                    left: l.col1_x + 20,
                    top: l.col_y + 74,
                    right: l.col1_x + l.col_w - 20,
                    bottom: l.col_y + 114,
                };
                let _ = DrawTextW(mem_dc, &mut empty_msg2, &mut empty_rc2, DT_WORDBREAK);
            } else {
                let item_h = (44.0 * l.font_scale) as i32;
                let item_w = l.col_w - 36;
                for (idx, peer) in lan_peers.iter().take(2).enumerate() {
                    let item_x = l.col1_x + 18;
                    let item_y = l.col_y + 46 + (idx as i32 * (item_h + 8));

                    draw_rounded_box(mem_dc, item_x, item_y, item_w, item_h, PK_INPUT_BG, PK_CARD_BORDER, 8);

                    crate::ui::draw_radio(mem_dc, item_x + 10, item_y + 12, 18, PK_GREEN, 2);

                    let _ = SelectObject(mem_dc, HGDIOBJ(font_btn.0));
                    SetTextColor(mem_dc, PK_TEXT);
                    let title_text = if !peer.user.is_empty() {
                        format!("{} ({})", peer.hostname, peer.user)
                    } else {
                        peer.hostname.clone()
                    };
                    let mut title_txt: Vec<u16> = title_text.encode_utf16().collect();
                    let mut title_rc = RECT {
                        left: item_x + 36,
                        top: item_y + 4,
                        right: item_x + item_w - 86,
                        bottom: item_y + 24,
                    };
                    let _ = DrawTextW(mem_dc, &mut title_txt, &mut title_rc, DT_SINGLELINE | DT_VCENTER);

                    let _ = SelectObject(mem_dc, HGDIOBJ(font_console.0));
                    SetTextColor(mem_dc, PK_MUTED);
                    let sub_text = format!(
                        "ID: {} | <{}ms | {}",
                        peer.id,
                        peer.latency_ms,
                        peer.ip.map(|i| i.to_string()).unwrap_or_default()
                    );
                    let mut sub_txt: Vec<u16> = sub_text.encode_utf16().collect();
                    let mut sub_rc = RECT {
                        left: item_x + 36,
                        top: item_y + 24,
                        right: item_x + item_w - 86,
                        bottom: item_y + item_h - 4,
                    };
                    let _ = DrawTextW(mem_dc, &mut sub_txt, &mut sub_rc, DT_SINGLELINE | DT_VCENTER);

                    let pill_btn_w = 72;
                    let pill_btn_h = 28;
                    let pill_btn_x = item_x + item_w - pill_btn_w - 6;
                    let pill_btn_y = item_y + (item_h - pill_btn_h) / 2;
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
        } else if card3_tab == 1 {
            // Render Address Book View
            if contacts.is_empty() {
                let _ = SelectObject(mem_dc, HGDIOBJ(font_console.0));
                SetTextColor(mem_dc, PK_MUTED);
                let mut empty_msg1: Vec<u16> = "No contacts in Address Book yet.".encode_utf16().collect();
                let mut empty_rc1 = RECT {
                    left: l.col1_x + 20,
                    top: l.col_y + 48,
                    right: l.col1_x + l.col_w - 20,
                    bottom: l.col_y + 70,
                };
                let _ = DrawTextW(mem_dc, &mut empty_msg1, &mut empty_rc1, DT_SINGLELINE | DT_VCENTER);

                let mut empty_msg2: Vec<u16> =
                    "Enter a 9-digit Desky ID above and click [+ Add] to save it here with a nickname."
                        .encode_utf16()
                        .collect();
                let mut empty_rc2 = RECT {
                    left: l.col1_x + 20,
                    top: l.col_y + 72,
                    right: l.col1_x + l.col_w - 20,
                    bottom: l.col_y + 112,
                };
                let _ = DrawTextW(mem_dc, &mut empty_msg2, &mut empty_rc2, DT_WORDBREAK);
            } else {
                let item_h = (44.0 * l.font_scale) as i32;
                let item_w = l.col_w - 36;
                for (idx, entry) in contacts.iter().take(2).enumerate() {
                    let item_x = l.col1_x + 18;
                    let item_y = l.col_y + 46 + (idx as i32 * (item_h + 8));

                    draw_rounded_box(mem_dc, item_x, item_y, item_w, item_h, PK_INPUT_BG, PK_CARD_BORDER, 8);

                    crate::ui::draw_monitor(mem_dc, item_x + 10, item_y + 13, 18, PK_GREEN, 2);

                    let _ = SelectObject(mem_dc, HGDIOBJ(font_btn.0));
                    SetTextColor(mem_dc, PK_TEXT);
                    let mut alias_txt: Vec<u16> = entry.alias.encode_utf16().collect();
                    let mut alias_rc = RECT {
                        left: item_x + 36,
                        top: item_y + 4,
                        right: item_x + item_w - 105,
                        bottom: item_y + 24,
                    };
                    let _ = DrawTextW(mem_dc, &mut alias_txt, &mut alias_rc, DT_SINGLELINE | DT_VCENTER);

                    let _ = SelectObject(mem_dc, HGDIOBJ(font_console.0));
                    SetTextColor(mem_dc, PK_MUTED);
                    let pwd_info = if entry.saved_password.is_some() { " | [🔑 Unattended]" } else { "" };
                    let sub_info = format!("{} | {}{}", entry.id, entry.group, pwd_info);
                    let mut sub_txt: Vec<u16> = sub_info.encode_utf16().collect();
                    let mut sub_rc = RECT {
                        left: item_x + 36,
                        top: item_y + 24,
                        right: item_x + item_w - 105,
                        bottom: item_y + item_h - 4,
                    };
                    let _ = DrawTextW(mem_dc, &mut sub_txt, &mut sub_rc, DT_SINGLELINE | DT_VCENTER);

                    let pill_btn_w = 66;
                    let pill_btn_h = 28;
                    let pill_btn_x = item_x + item_w - pill_btn_w - 28;
                    let pill_btn_y = item_y + (item_h - pill_btn_h) / 2;
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

                    let del_btn_w = 20;
                    let del_btn_x = item_x + item_w - del_btn_w - 4;
                    let del_btn_y = pill_btn_y;
                    draw_rounded_box(mem_dc, del_btn_x, del_btn_y, del_btn_w, pill_btn_h, PK_CARD, PK_CARD_BORDER, 4);
                    SetTextColor(mem_dc, PK_MUTED);
                    let mut del_txt: Vec<u16> = "×".encode_utf16().collect();
                    let mut del_rc = RECT {
                        left: del_btn_x,
                        top: del_btn_y,
                        right: del_btn_x + del_btn_w,
                        bottom: del_btn_y + pill_btn_h,
                    };
                    let _ = DrawTextW(mem_dc, &mut del_txt, &mut del_rc, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
                }
            }
        } else {
            // Render Recent Sessions View
            let recent_sessions = crate::identity::get_recent_sessions();
            if recent_sessions.is_empty() {
                let _ = SelectObject(mem_dc, HGDIOBJ(font_console.0));
                SetTextColor(mem_dc, PK_MUTED);
                let mut empty_msg1: Vec<u16> = "No recent sessions yet.".encode_utf16().collect();
                let mut empty_rc1 = RECT {
                    left: l.col1_x + 20,
                    top: l.col_y + 48,
                    right: l.col1_x + l.col_w - 20,
                    bottom: l.col_y + 70,
                };
                let _ = DrawTextW(mem_dc, &mut empty_msg1, &mut empty_rc1, DT_SINGLELINE | DT_VCENTER);

                let mut empty_msg2: Vec<u16> =
                    "Remote desks you connect to will appear here for fast 1-click reconnect."
                        .encode_utf16()
                        .collect();
                let mut empty_rc2 = RECT {
                    left: l.col1_x + 20,
                    top: l.col_y + 72,
                    right: l.col1_x + l.col_w - 20,
                    bottom: l.col_y + 112,
                };
                let _ = DrawTextW(mem_dc, &mut empty_msg2, &mut empty_rc2, DT_WORDBREAK);
            } else {
                let item_h = (36.0 * l.font_scale) as i32;
                let item_w = l.col_w - 36;
                for (idx, session) in recent_sessions.iter().take(2).enumerate() {
                    let item_x = l.col1_x + 18;
                    let item_y = l.col_y + 46 + (idx as i32 * (item_h + 8));

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
        }

        // 4. Card 4: SECURITY & SERVER (Bottom Right)
        draw_rounded_box(mem_dc, l.col2_x, l.col_y, l.col_w, l.col_h, PK_CARD, PK_CARD_BORDER, 12);

        let tag4_w = (184.0 * l.font_scale) as i32;
        let tag4_h = (22.0 * l.font_scale) as i32;
        draw_rounded_box(mem_dc, l.col2_x + 18, l.col_y + 14, tag4_w, tag4_h, PK_INPUT_BG, PK_CARD_BORDER, 6);
        let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
        SetTextColor(mem_dc, PK_MUTED);
        let mut tag4: Vec<u16> = "CONNECTION & ACTIVITY LOGS".encode_utf16().collect();
        let mut tag4_rect = RECT {
            left: l.col2_x + 18,
            top: l.col_y + 14,
            right: l.col2_x + 18 + tag4_w,
            bottom: l.col_y + 14 + tag4_h,
        };
        let _ = DrawTextW(mem_dc, &mut tag4, &mut tag4_rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER);

        let console_box_x = l.col2_x + 18;
        let console_box_y = l.col_y + 46;
        let console_box_w = l.col_w - 36;
        let console_box_h = l.col_h - 58;

        draw_rounded_box(
            mem_dc,
            console_box_x,
            console_box_y,
            console_box_w,
            console_box_h,
            COLORREF(0x00F8FAF8),
            PK_CARD_BORDER,
            8,
        );

        let _ = SelectObject(mem_dc, HGDIOBJ(font_console.0));
        SetTextColor(mem_dc, PK_TEXT);

        let logs = STATUS_LOGS.lock().unwrap();
        let line_height = (18.0 * l.font_scale) as i32;
        let max_visible_lines = ((console_box_h - 16) / line_height).max(1) as usize;

        let display_logs: Vec<String> = if logs.len() > max_visible_lines {
            logs[logs.len() - max_visible_lines..].to_vec()
        } else {
            logs.clone()
        };

        for (i, log) in display_logs.iter().enumerate() {
            let mut log_wide: Vec<u16> = log.encode_utf16().collect();
            let mut line_rect = RECT {
                left: console_box_x + 12,
                top: console_box_y + 8 + (i as i32 * line_height),
                right: console_box_x + console_box_w - 12,
                bottom: console_box_y + 8 + ((i + 1) as i32 * line_height),
            };
            let _ = DrawTextW(
                mem_dc,
                &mut log_wide,
                &mut line_rect,
                DT_SINGLELINE | DT_VCENTER,
            );
        }

    let _ = DeleteObject(HGDIOBJ(font_sub.0));
    let _ = DeleteObject(HGDIOBJ(font_badge.0));
    let _ = DeleteObject(HGDIOBJ(font_id.0));
    let _ = DeleteObject(HGDIOBJ(font_btn.0));
    let _ = DeleteObject(HGDIOBJ(font_console.0));
}

/// Handles mouse clicks on the Homepage view
pub unsafe fn handle_homepage_click(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) -> bool {
    let l = get_responsive_layout(w, h);

    // 2. Copy Button Clicked
    if x >= l.copy_btn_x
        && x <= l.copy_btn_x + l.copy_btn_w
        && y >= l.copy_btn_y
        && y <= l.copy_btn_y + l.copy_btn_h
    {
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
            append_status_log(&format!("Desky ID '{}' copied to Windows Clipboard!", id_to_copy));
        }
        return true;
    }

    // 3. Connect Button Clicked
    if x >= l.conn_btn_x
        && x <= l.conn_btn_x + l.conn_btn_w
        && y >= l.conn_btn_y
        && y <= l.conn_btn_y + l.conn_btn_h
    {
        handle_homepage_connect(hwnd);
        return true;
    }

    // 4. Remote Desk Input Box Clicked (Focus Edit control)
    if x >= l.input_box_x
        && x <= l.input_box_x + l.input_box_w
        && y >= l.input_box_y
        && y <= l.input_box_y + l.input_box_h
    {
        super::remote_desk_edit::focus_remote_desk_edit(HWND_EDIT_TARGET);
        return true;
    }

    // 4. Card 3 Header Tabs: RECENTS vs CONTACTS vs LAN RADAR
    let contacts = crate::identity::load_address_book();
    let lan_peers = crate::p2p::get_lan_peers();
    let (t0, t1, t2) = get_card3_tab_rects(
        l.col1_x,
        l.col_y,
        l.font_scale,
        contacts.len(),
        lan_peers.len(),
    );

    let (t0_x, t0_y, t0_w, t0_h, _) = t0;
    let (t1_x, _, t1_w, _, _) = t1;
    let (t2_x, _, t2_w, _, _) = t2;
    let card3_tab = BOTTOM_CARD3_TAB.load(Ordering::Relaxed);

    if y >= t0_y && y <= t0_y + t0_h {
        if x >= t0_x && x <= t0_x + t0_w {
            BOTTOM_CARD3_TAB.store(0, Ordering::Relaxed);
            BOTTOM_TAB_RADAR.store(false, Ordering::Relaxed);
            let _ = InvalidateRect(Some(hwnd), None, false);
            return true;
        }
        if x >= t1_x && x <= t1_x + t1_w {
            BOTTOM_CARD3_TAB.store(1, Ordering::Relaxed);
            BOTTOM_TAB_RADAR.store(false, Ordering::Relaxed);
            let _ = InvalidateRect(Some(hwnd), None, false);
            return true;
        }
        if x >= t2_x && x <= t2_x + t2_w {
            BOTTOM_CARD3_TAB.store(2, Ordering::Relaxed);
            BOTTOM_TAB_RADAR.store(true, Ordering::Relaxed);
            trigger_lan_radar_scan();
            let _ = InvalidateRect(Some(hwnd), None, false);
            return true;
        }
        if card3_tab == 2 {
            let is_scanning = is_lan_scan_active();
            let scan_w = if is_scanning { 74 } else { 54 };
            let scan_x = l.col1_x + l.col_w - scan_w - 18;
            if x >= scan_x && x <= scan_x + scan_w {
                trigger_lan_radar_scan();
                let _ = InvalidateRect(Some(hwnd), None, false);
                return true;
            }
        } else if card3_tab == 1 {
            let add_w = 54;
            let add_x = l.col1_x + l.col_w - add_w - 18;
            if x >= add_x && x <= add_x + add_w {
                let mut buffer = [0u16; 128];
                let len = GetWindowTextLengthW(HWND_EDIT_TARGET) as usize;
                let raw_input = if len > 0 {
                    let _ = GetWindowTextW(HWND_EDIT_TARGET, &mut buffer);
                    String::from_utf16_lossy(&buffer[..len])
                } else {
                    String::new()
                };
                let digits: String = raw_input.chars().filter(|c| c.is_ascii_digit()).collect();
                if digits.len() == 9 {
                    let alias = format!("Desk {}", &digits[6..9]);
                    crate::identity::add_or_update_contact(&digits, &alias, "Favorites", None);
                    append_status_log(&format!("Saved ID {} to Address Book!", &digits));
                } else {
                    append_status_log("[!] Enter a 9-digit Desky ID above, then click [+ Add]");
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                return true;
            }
        }
    }

    // 5. Card 3 Content: LAN Radar peers, Address Book, or Recent Sessions
    if card3_tab == 2 {
        let lan_peers = crate::p2p::get_lan_peers();
        let item_h = (44.0 * l.font_scale) as i32;
        let item_w = l.col_w - 36;
        for (idx, peer) in lan_peers.iter().take(2).enumerate() {
            let item_x = l.col1_x + 18;
            let item_y = l.col_y + 46 + (idx as i32 * (item_h + 8));

            if x >= item_x && x <= item_x + item_w && y >= item_y && y <= item_y + item_h {
                let wide_id: Vec<u16> = peer.id.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = SetWindowTextW(HWND_EDIT_TARGET, PCWSTR(wide_id.as_ptr()));

                let pill_btn_w = 72;
                let pill_btn_x = item_x + item_w - pill_btn_w - 6;
                if x >= pill_btn_x {
                    let target_display = peer.id.clone();
                    info!("Initiating direct LAN P2P session to: {} ({})", target_display, peer.hostname);
                    crate::identity::save_recent_session(&target_display);
                    append_status_log(&format!(
                        "Initiating LAN connection to {} ({}) at {}...",
                        peer.hostname,
                        target_display,
                        peer.ip.map(|i| i.to_string()).unwrap_or_default()
                    ));
                    super::viewport::spawn_or_add_tab_viewport(&target_display);
                } else {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return true;
            }
        }
    } else if card3_tab == 1 {
        let contacts = crate::identity::load_address_book();
        let item_h = (44.0 * l.font_scale) as i32;
        let item_w = l.col_w - 36;
        for (idx, entry) in contacts.iter().take(2).enumerate() {
            let item_x = l.col1_x + 18;
            let item_y = l.col_y + 46 + (idx as i32 * (item_h + 8));

            if x >= item_x && x <= item_x + item_w && y >= item_y && y <= item_y + item_h {
                let wide_id: Vec<u16> = entry.id.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = SetWindowTextW(HWND_EDIT_TARGET, PCWSTR(wide_id.as_ptr()));

                let pill_btn_w = 66;
                let pill_btn_x = item_x + item_w - pill_btn_w - 28;
                let del_btn_w = 20;
                let del_btn_x = item_x + item_w - del_btn_w - 4;

                if x >= del_btn_x {
                    crate::identity::delete_contact(&entry.id);
                    append_status_log(&format!("Removed {} from Address Book", entry.alias));
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return true;
                }

                if x >= pill_btn_x {
                    let target_display = entry.id.clone();
                    info!("Initiating session to Address Book contact: {} ({})", entry.alias, target_display);
                    crate::identity::save_recent_session(&target_display);
                    append_status_log(&format!("Connecting to {} ({}) from Address Book...", entry.alias, target_display));
                    super::viewport::spawn_or_add_tab_viewport(&target_display);
                } else {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return true;
            }
        }
    } else {
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
                    let target_display = session.clone();
                    info!("Initiating P2P session to remote Desky ID: {}", target_display);
                    crate::identity::save_recent_session(&target_display);
                    append_status_log(&format!(
                        "Requesting session with Remote Desky ID: {}...",
                        target_display
                    ));

                    super::viewport::spawn_or_add_tab_viewport(&target_display);
                } else {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return true;
            }
        }
    }

    false
}

/// Validates entered remote Desky ID and initiates connection in the unified window
pub unsafe fn handle_homepage_connect(hwnd: HWND) {
    let mut buffer = [0u16; 128];
    let len = GetWindowTextLengthW(HWND_EDIT_TARGET) as usize;
    let raw_input = if len > 0 {
        let _ = GetWindowTextW(HWND_EDIT_TARGET, &mut buffer);
        String::from_utf16_lossy(&buffer[..len])
    } else {
        String::new()
    };

    let raw_trimmed = raw_input.trim();
    let direct_addr_opt = crate::p2p::parse_direct_target(raw_trimmed);

    let (target_connect_id, target_display) = if let Some(addr) = direct_addr_opt {
        (format!("{}", addr), format!("Direct IP: {}", addr))
    } else {
        let digits: String = raw_input.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            append_status_log("[!] Please enter a 9-digit remote Desky ID or IP (e.g. 800 258 756 or 192.168.1.50:21117)");
            let _ = InvalidateRect(Some(hwnd), None, false);
            return;
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
            append_status_log("[!] That is THIS computer's own ID! Please enter the remote computer's ID.");
            let _ = InvalidateRect(Some(hwnd), None, false);
            return;
        }

        let disp = if digits.len() == 9 {
            format!("{} {} {}", &digits[0..3], &digits[3..6], &digits[6..9])
        } else {
            digits.clone()
        };
        (disp.clone(), disp)
    };

    info!("Initiating P2P session to remote target: {}", target_display);
    crate::identity::save_recent_session(&target_display);
    append_status_log(&format!(
        "Requesting session with Remote Target: {}...",
        target_display
    ));

    super::viewport::spawn_or_add_tab_viewport(&target_connect_id);
}

/// Tracks hover state for Homepage buttons and changes cursor
pub unsafe fn handle_homepage_mousemove(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
    let l = get_responsive_layout(w, h);
    let (t0, t1, t2) = get_card3_tab_rects(l.col1_x, l.col_y, l.font_scale, 0, 0);
    let (t0_x, t0_y, t0_w, t0_h, _) = t0;
    let (t1_x, _, t1_w, _, _) = t1;
    let (t2_x, _, t2_w, _, _) = t2;

    let new_hover = if x >= l.copy_btn_x
        && x <= l.copy_btn_x + l.copy_btn_w
        && y >= l.copy_btn_y
        && y <= l.copy_btn_y + l.copy_btn_h
    {
        1
    } else if x >= l.conn_btn_x
        && x <= l.conn_btn_x + l.conn_btn_w
        && y >= l.conn_btn_y
        && y <= l.conn_btn_y + l.conn_btn_h
    {
        2
    } else if y >= t0_y && y <= t0_y + t0_h {
        if x >= t0_x && x <= t0_x + t0_w {
            10
        } else if x >= t1_x && x <= t1_x + t1_w {
            11
        } else if x >= t2_x && x <= t2_x + t2_w {
            12
        } else {
            0
        }
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
