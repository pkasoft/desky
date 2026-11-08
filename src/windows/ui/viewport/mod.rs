#![allow(dead_code)]
//! Dedicated Viewport Remote Stream Canvas Window & AnyDesk-Style Tabbed Session Manager
//! Modular Architecture with Interactive Whiteboard, Fast Session Recording, and Quick Actions.

#![allow(unsafe_op_in_unsafe_fn)]

pub mod file_hud;
pub mod input_injector;
pub mod quick_actions;
pub mod settings_modal;
pub mod toolbar;
pub mod whiteboard;

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tracing::info;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC,
    DeleteDC, DeleteObject, EndPaint, FillRect, GetDC, InvalidateRect,
    ReleaseDC, ScreenToClient, SelectObject, SetBrushOrgEx, SetStretchBltMode,
    StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, COLORONCOLOR, DIB_RGB_COLORS,
    HALFTONE, HBRUSH, HDC, HGDIOBJ, PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, RemoveClipboardFormatListener,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetCursorPos,
    GetMessageW, IsWindow, LoadCursorW, PostQuitMessage, RegisterClassExW,
    SetCursor, SetForegroundWindow, SetWindowDisplayAffinity, SetWindowTextW, ShowWindow,
    TranslateMessage, IDC_ARROW, IDC_CROSS, IDC_HAND, MSG, SW_RESTORE, SW_SHOW,
    WDA_EXCLUDEFROMCAPTURE, WINDOW_EX_STYLE, WM_CHAR, WM_CLIPBOARDUPDATE,
    WM_DESTROY, WM_DROPFILES, WM_ERASEBKGND, WM_KEYDOWN, WM_KEYUP,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SIZE,
    WNDCLASSEXW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

use super::theme;
pub use input_injector::*;
pub use quick_actions::*;
pub use settings_modal::*;
pub use toolbar::*;
pub use whiteboard::*;

pub use viewport_wnd_proc as viewport_wndproc;

pub(crate) static mut VIEWPORT_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());
pub(crate) static VIEWPORT_RUNNING: AtomicBool = AtomicBool::new(false);

pub static REMOTE_ORIG_W: AtomicU32 = AtomicU32::new(0);
pub static REMOTE_ORIG_H: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Debug, PartialEq)]
pub struct SessionTab {
    pub id: String,
    pub title: String,
}

static SESSION_TABS: OnceLock<Mutex<Vec<SessionTab>>> = OnceLock::new();
pub static ACTIVE_TAB_INDEX: AtomicUsize = AtomicUsize::new(0);
static HWND_VIEWPORT: AtomicIsize = AtomicIsize::new(0);
static DRAGGING_TAB_INDEX: AtomicI32 = AtomicI32::new(-1);
static DRAG_START_POS: Mutex<Option<(i32, i32)>> = Mutex::new(None);
pub static ACTIVE_REMOTE_MONITOR: AtomicU32 = AtomicU32::new(0);

// Toolbar visibility: true = shown, false = collapsed/hidden
pub static SHOW_TOOLBAR: AtomicBool = AtomicBool::new(true);

// Silent Admin Mode toggle: false = normal desktop stream, true = background admin manager
pub static VIEWPORT_IS_ADMIN_MODE: AtomicBool = AtomicBool::new(false);
static ADMIN_HOVER_STATE: AtomicI32 = AtomicI32::new(-1);
pub static REMOTE_PRIVACY_MODE: AtomicBool = AtomicBool::new(false);

pub fn init_session_tabs() {
    let store = SESSION_TABS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut tabs) = store.lock() {
        tabs.clear();
        tabs.push(SessionTab {
            id: "home".to_string(),
            title: "Home".to_string(),
        });
    }
    ACTIVE_TAB_INDEX.store(0, Ordering::Relaxed);
}

pub fn get_session_tabs() -> Vec<SessionTab> {
    let store = SESSION_TABS.get_or_init(|| Mutex::new(Vec::new()));
    store.lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn switch_session_tab(tab_idx: usize) {
    let store = SESSION_TABS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(tabs) = store.lock() {
        if let Some(tab) = tabs.get(tab_idx) {
            ACTIVE_TAB_INDEX.store(tab_idx, Ordering::Relaxed);
            let hwnd_raw = HWND_VIEWPORT.load(Ordering::Relaxed);
            if hwnd_raw != 0 {
                let hwnd = HWND(hwnd_raw as *mut _);
                unsafe {
                    if tab_idx == 0 {
                        super::launcher::set_edit_target_visibility(true);
                        let _ = SetWindowTextW(hwnd, w!("Desky | pkasoft.com"));
                    } else {
                        super::launcher::set_edit_target_visibility(false);
                        let title = format!("Desky | [{}]", tab.title);
                        let wide_title: Vec<u16> =
                            title.encode_utf16().chain(std::iter::once(0)).collect();
                        let _ = SetWindowTextW(hwnd, PCWSTR(wide_title.as_ptr()));

                        let current_target = crate::p2p::get_active_session_target();
                        let is_active = crate::p2p::is_client_session_active();
                        let norm_tab_target = crate::p2p::normalize_id(&tab.id);
                        let matches_active = current_target
                            .as_deref()
                            .map(|t| crate::p2p::normalize_id(t) == norm_tab_target)
                            .unwrap_or(false);

                        if !is_active || !matches_active {
                            crate::p2p::start_client_session(
                                &tab.id,
                                TARGET_FPS.load(Ordering::Relaxed),
                                CAPTURE_CURSOR.load(Ordering::Relaxed),
                            );
                        }
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
        }
    }
}

pub fn close_session_tab(hwnd: HWND, tab_idx: usize) {
    if tab_idx == 0 {
        return; // Home tab is permanent
    }
    let store = SESSION_TABS.get_or_init(|| Mutex::new(Vec::new()));
    let new_active = if let Ok(mut tabs) = store.lock() {
        if tab_idx < tabs.len() {
            tabs.remove(tab_idx);
        }
        let cur = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
        if cur >= tabs.len() {
            tabs.len() - 1
        } else if cur == tab_idx {
            tab_idx - 1
        } else {
            cur
        }
    } else {
        0
    };

    crate::p2p::stop_client_session();
    ACTIVE_TAB_INDEX.store(new_active, Ordering::Relaxed);
    switch_session_tab(new_active);
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn detach_tab(tab_idx: usize, drop_screen_x: i32, drop_screen_y: i32) {
    if tab_idx == 0 {
        return;
    }
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
        let mut idx = 1;
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
        let wide_title: Vec<u16> =
            window_title.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            super::launcher::set_edit_target_visibility(false);
            let _ = SetWindowTextW(hwnd, PCWSTR(wide_title.as_ptr()));
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
            let _ = InvalidateRect(Some(hwnd), None, false);
        }

        let current_target = crate::p2p::get_active_session_target();
        let is_active = crate::p2p::is_client_session_active();
        let norm_target = crate::p2p::normalize_id(target);
        let matches_active = current_target
            .as_deref()
            .map(|t| crate::p2p::normalize_id(t) == norm_target)
            .unwrap_or(false);

        if !is_active || !matches_active {
            crate::p2p::start_client_session(
                target,
                TARGET_FPS.load(Ordering::Relaxed),
                CAPTURE_CURSOR.load(Ordering::Relaxed),
            );
        }
    }
}

unsafe fn spawn_detached_viewport_window(target: &str, screen_x: i32, screen_y: i32) {
    let h_module = GetModuleHandleW(None).unwrap_or_default();
    let h_instance: HINSTANCE = h_module.into();

    let window_title = format!("Desky | [{}]", target);
    let wide_title: Vec<u16> =
        window_title.encode_utf16().chain(std::iter::once(0)).collect();

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

pub fn start_viewport_render_thread(hwnd: HWND) {
    if VIEWPORT_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    HWND_VIEWPORT.store(hwnd.0 as isize, Ordering::Relaxed);
    REMOTE_ORIG_W.store(0, Ordering::Relaxed);
    REMOTE_ORIG_H.store(0, Ordering::Relaxed);

    let hwnd_raw = hwnd.0 as isize;
    std::thread::spawn(move || {
        info!("Dedicated Viewport Render Thread Started.");
        let target_hwnd = HWND(hwnd_raw as *mut _);

        let mut last_rendered_frame_id: u32 = 0;
        let mut last_tab_idx: usize = 0;
        let mut last_hover: i32 = -1;
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

        let mut cached_mem_dc: Option<HDC> = None;
        let mut cached_mem_bmp: Option<windows::Win32::Graphics::Gdi::HBITMAP> = None;
        let mut cached_old_bmp: Option<HGDIOBJ> = None;
        let mut cached_bmp_w: i32 = 0;
        let mut cached_bmp_h: i32 = 0;

        while VIEWPORT_RUNNING.load(Ordering::Relaxed) {
            unsafe {
                if !IsWindow(Some(target_hwnd)).as_bool() {
                    break;
                }

                let mut rect = RECT::default();
                let _ = GetClientRect(target_hwnd, &mut rect);
                let client_w = rect.right - rect.left;
                let client_h = rect.bottom - rect.top;

                if client_w <= 0 || client_h <= 0 {
                    std::thread::sleep(Duration::from_millis(16));
                    continue;
                }

                let active_idx = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
                let hover_state = TAB_HOVER_STATE.load(Ordering::Relaxed);
                let mode = DISPLAY_MODE.load(Ordering::Relaxed);
                let q_mode = crate::p2p::get_quality_mode();
                let target_fps = TARGET_FPS.load(Ordering::Relaxed);
                let cursor_on = CAPTURE_CURSOR.load(Ordering::Relaxed);
                let toolbar_visible = SHOW_TOOLBAR.load(Ordering::Relaxed);
                let modal_open = SHOW_SETTINGS_MODAL.load(Ordering::Relaxed);
                let cat_val = SETTINGS_CATEGORY.load(Ordering::Relaxed);
                let tabs = get_session_tabs();

                let latest_frame = crate::p2p::get_latest_remote_frame();
                let cur_frame_id = latest_frame.as_ref().map(|f| f.frame_id).unwrap_or(0);

                let frame_changed = cur_frame_id != last_rendered_frame_id;
                let ui_changed = active_idx != last_tab_idx
                    || hover_state != last_hover
                    || client_w != last_w
                    || client_h != last_h
                    || mode != last_mode
                    || q_mode != last_q_mode
                    || target_fps != last_fps
                    || cursor_on != last_cur
                    || toolbar_visible != last_toolbar
                    || modal_open != last_modal
                    || cat_val != last_cat;

                let periodic_refresh = last_ui_refresh.elapsed() >= Duration::from_millis(50);

                if frame_changed || ui_changed || periodic_refresh {
                    last_rendered_frame_id = cur_frame_id;
                    last_tab_idx = active_idx;
                    last_hover = hover_state;
                    last_w = client_w;
                    last_h = client_h;
                    last_mode = mode;
                    last_q_mode = q_mode;
                    last_fps = target_fps;
                    last_cur = cursor_on;
                    last_toolbar = toolbar_visible;
                    last_modal = modal_open;
                    last_cat = cat_val;
                    last_ui_refresh = Instant::now();

                    let toolbar_h = if toolbar_visible { TOOLBAR_HEIGHT } else { 0 };

                    if client_w > 50 && client_h > 50 {
                        let hdc_window = GetDC(Some(target_hwnd));

                        if cached_mem_dc.is_none() {
                            cached_mem_dc = Some(CreateCompatibleDC(Some(hdc_window)));
                        }
                        let mem_dc = cached_mem_dc.unwrap();

                        if cached_mem_bmp.is_none()
                            || cached_bmp_w != client_w
                            || cached_bmp_h != client_h
                        {
                            if let Some(old) = cached_old_bmp.take() {
                                let _ = SelectObject(mem_dc, old);
                            }
                            if let Some(bmp) = cached_mem_bmp.take() {
                                let _ = DeleteObject(HGDIOBJ(bmp.0));
                            }
                            let new_bmp = CreateCompatibleBitmap(hdc_window, client_w, client_h);
                            cached_old_bmp = Some(SelectObject(mem_dc, HGDIOBJ(new_bmp.0)));
                            cached_mem_bmp = Some(new_bmp);
                            cached_bmp_w = client_w;
                            cached_bmp_h = client_h;
                        }

                        let orig_w = REMOTE_ORIG_W.load(Ordering::Relaxed);
                        let orig_h = REMOTE_ORIG_H.load(Ordering::Relaxed);

                        if modal_open {
                            super::launcher::set_edit_target_visibility(false);
                            draw_fullscreen_settings_view(
                                mem_dc,
                                client_w,
                                client_h,
                                mode,
                                target_fps,
                                TOOLBAR_HEIGHT,
                                orig_w,
                                orig_h,
                            );
                            draw_tab_bar(mem_dc, client_w, TOOLBAR_HEIGHT, &tabs, active_idx, hover_state);
                        } else if active_idx == 0 {
                            let hp_hover = super::launcher::get_hover_button();
                            super::launcher::draw_homepage_view(mem_dc, client_w, client_h, hp_hover);
                            draw_tab_bar(mem_dc, client_w, TOOLBAR_HEIGHT, &tabs, 0, hover_state);
                        } else {
                            if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                                super::admin::draw_admin_view(
                                    mem_dc,
                                    client_w,
                                    client_h - toolbar_h,
                                    ADMIN_HOVER_STATE.load(Ordering::Relaxed),
                                );
                            } else if let Some(ref remote_frame) = latest_frame {
                                REMOTE_ORIG_W.store(remote_frame.orig_width, Ordering::Relaxed);
                                REMOTE_ORIG_H.store(remote_frame.orig_height, Ordering::Relaxed);

                                let bmi = BITMAPINFO {
                                    bmiHeader: BITMAPINFOHEADER {
                                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                                        biWidth: remote_frame.width as i32,
                                        biHeight: -(remote_frame.height as i32),
                                        biPlanes: 1,
                                        biBitCount: 32,
                                        biCompression: BI_RGB.0,
                                        biSizeImage: 0,
                                        biXPelsPerMeter: 0,
                                        biYPelsPerMeter: 0,
                                        biClrUsed: 0,
                                        biClrImportant: 0,
                                    },
                                    bmiColors: [windows::Win32::Graphics::Gdi::RGBQUAD::default()],
                                };

                                let view_h = client_h - toolbar_h;
                                let mut render_x = 0;
                                let mut render_y = toolbar_h;
                                let mut render_w = client_w;
                                let mut render_h = view_h;

                                match mode {
                                    0 => {
                                        let scale_x = client_w as f32 / remote_frame.orig_width as f32;
                                        let scale_y = view_h as f32 / remote_frame.orig_height as f32;
                                        let scale = scale_x.min(scale_y);
                                        render_w = (remote_frame.orig_width as f32 * scale) as i32;
                                        render_h = (remote_frame.orig_height as f32 * scale) as i32;
                                        render_x = (client_w - render_w) / 2;
                                        render_y = toolbar_h + (view_h - render_h) / 2;

                                        let top_rc = RECT { left: 0, top: toolbar_h, right: client_w, bottom: render_y };
                                        let bot_rc = RECT { left: 0, top: render_y + render_h, right: client_w, bottom: client_h };
                                        let lft_rc = RECT { left: 0, top: render_y, right: render_x, bottom: render_y + render_h };
                                        let rgt_rc = RECT { left: render_x + render_w, top: render_y, right: client_w, bottom: render_y + render_h };
                                        let bg_b = windows::Win32::Graphics::Gdi::CreateSolidBrush(theme::PK_BG);
                                        if top_rc.bottom > top_rc.top { FillRect(mem_dc, &top_rc, bg_b); }
                                        if bot_rc.bottom > bot_rc.top { FillRect(mem_dc, &bot_rc, bg_b); }
                                        if lft_rc.right > lft_rc.left { FillRect(mem_dc, &lft_rc, bg_b); }
                                        if rgt_rc.right > rgt_rc.left { FillRect(mem_dc, &rgt_rc, bg_b); }
                                        let _ = DeleteObject(HGDIOBJ(bg_b.0));

                                        let _ = SetStretchBltMode(mem_dc, HALFTONE);
                                        let _ = SetBrushOrgEx(mem_dc, 0, 0, None);
                                        let _ = StretchDIBits(
                                            mem_dc,
                                            render_x,
                                            render_y,
                                            render_w,
                                            render_h,
                                            0,
                                            0,
                                            remote_frame.width as i32,
                                            remote_frame.height as i32,
                                            Some(remote_frame.bgra.as_ptr() as *const _),
                                            &bmi,
                                            DIB_RGB_COLORS,
                                            SRCCOPY,
                                        );
                                    }
                                    1 => {
                                        let _ = SetStretchBltMode(mem_dc, COLORONCOLOR);
                                        let _ = StretchDIBits(
                                            mem_dc,
                                            0,
                                            toolbar_h,
                                            client_w,
                                            view_h,
                                            0,
                                            0,
                                            remote_frame.width as i32,
                                            remote_frame.height as i32,
                                            Some(remote_frame.bgra.as_ptr() as *const _),
                                            &bmi,
                                            DIB_RGB_COLORS,
                                            SRCCOPY,
                                        );
                                    }
                                    _ => {
                                        let offset_x = (client_w - remote_frame.width as i32) / 2;
                                        let offset_y = toolbar_h + (view_h - remote_frame.height as i32) / 2;
                                        render_x = offset_x;
                                        render_y = offset_y;
                                        render_w = remote_frame.width as i32;
                                        render_h = remote_frame.height as i32;
                                        let _ = SetStretchBltMode(mem_dc, COLORONCOLOR);
                                        let _ = StretchDIBits(
                                            mem_dc,
                                            offset_x,
                                            offset_y,
                                            remote_frame.width as i32,
                                            remote_frame.height as i32,
                                            0,
                                            0,
                                            remote_frame.width as i32,
                                            remote_frame.height as i32,
                                            Some(remote_frame.bgra.as_ptr() as *const _),
                                            &bmi,
                                            DIB_RGB_COLORS,
                                            SRCCOPY,
                                        );
                                    }
                                }

                                draw_whiteboard_overlay(mem_dc, render_x, render_y, render_w, render_h);
                            }

                            if toolbar_visible {
                                draw_tab_bar(mem_dc, client_w, TOOLBAR_HEIGHT, &tabs, active_idx, hover_state);
                                let settings_x = client_w - 32 - 12;
                                let zap_x = settings_x - (32 + 5) * 7;
                                draw_quick_actions_popup(mem_dc, zap_x, 32, TOOLBAR_HEIGHT, hover_state);
                                draw_whiteboard_palette(mem_dc, client_w, TOOLBAR_HEIGHT, hover_state);
                            }
                            file_hud::draw_file_transfer_hud(mem_dc, client_w, client_h);
                        }

                        let _ = BitBlt(hdc_window, 0, 0, client_w, client_h, Some(mem_dc), 0, 0, SRCCOPY);
                        let _ = ReleaseDC(Some(target_hwnd), hdc_window);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(8));
        }

        unsafe {
            if let Some(old) = cached_old_bmp.take() {
                if let Some(dc) = cached_mem_dc {
                    let _ = SelectObject(dc, old);
                }
            }
            if let Some(bmp) = cached_mem_bmp.take() {
                let _ = DeleteObject(HGDIOBJ(bmp.0));
            }
            if let Some(dc) = cached_mem_dc.take() {
                let _ = DeleteDC(dc);
            }
        }
    });
}

pub unsafe extern "system" fn viewport_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_DROPFILES => {
            let hdrop = HDROP(wparam.0 as *mut _);
            let count = DragQueryFileW(hdrop, 0xFFFFFFFF, None);
            for i in 0..count {
                let mut buf = [0u16; 512];
                let len = DragQueryFileW(hdrop, i, Some(&mut buf));
                if len > 0 {
                    let path = String::from_utf16_lossy(&buf[..len as usize]);
                    crate::p2p::send_file_transfer(std::path::PathBuf::from(path));
                }
            }
            DragFinish(hdrop);
            LRESULT(0)
        }
        WM_CLIPBOARDUPDATE => {
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
                info!("Clipboard contains files: '{}' (Press Ctrl+V to paste into remote host)", desc);
            } else if let Some(text) = crate::p2p::read_clipboard_text() {
                if !crate::platform::is_clipboard_recently_synced(&text) {
                    crate::platform::mark_clipboard_synced(&text);
                    crate::p2p::send_remote_clipboard(&text);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            let mut rect = RECT::default();
            unsafe { let _ = GetClientRect(hwnd, &mut rect); }
            let client_w = rect.right - rect.left;
            let client_h = rect.bottom - rect.top;

            if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                if handle_settings_click(hwnd, client_w, client_h, client_x, client_y, TOOLBAR_HEIGHT) {
                    return LRESULT(0);
                }
            }

            if is_whiteboard_active() {
                let wb_hit = hit_test_whiteboard_palette(client_w, TOOLBAR_HEIGHT, client_x, client_y);
                if wb_hit == 9399 {
                    clear_strokes();
                    unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                    return LRESULT(0);
                } else if wb_hit >= 9301 && wb_hit <= 9304 {
                    WHITEBOARD_COLOR_IDX.store((wb_hit - 9301) as u8, Ordering::Relaxed);
                    unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                    return LRESULT(0);
                } else if wb_hit == 9300 {
                    return LRESULT(0);
                }
            }

            if is_quick_actions_open() {
                let settings_x = client_w - 32 - 12;
                let zap_x = settings_x - (32 + 5) * 7;
                let qa_hit = hit_test_quick_actions(zap_x, 32, TOOLBAR_HEIGHT, client_x, client_y);
                if qa_hit >= 9201 && qa_hit <= 9207 {
                    execute_quick_action(hwnd, qa_hit);
                    unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                    return LRESULT(0);
                } else if qa_hit == 9200 {
                    return LRESULT(0);
                } else {
                    set_quick_actions_open(false);
                }
            }

            let hit = get_tab_hit_state(hwnd, client_x, client_y);

            if hit == 9006 {
                let prev = SHOW_SETTINGS_MODAL.fetch_xor(true, Ordering::Relaxed);
                let has_pending = crate::p2p::get_pending_approval().is_some();
                let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
                super::launcher::set_edit_target_visibility(active == 0 && !has_pending && prev);
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                return LRESULT(0);
            } else if hit == 9007 {
                toggle_quick_actions();
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                return LRESULT(0);
            } else if hit == 9008 {
                toggle_whiteboard();
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                return LRESULT(0);
            } else if hit == 9009 {
                if crate::windows::recorder::is_recording() {
                    if let Some((path, total_bytes, total_frames, dur)) = crate::windows::recorder::stop_recording() {
                        super::launcher::append_status_log(&format!(
                            "🎥 Session recording saved: {} ({} frames, {} KB, {:.1}s)",
                            path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                            total_frames,
                            total_bytes / 1024,
                            dur.as_secs_f32()
                        ));
                    }
                } else {
                    let orig_w = REMOTE_ORIG_W.load(Ordering::Relaxed).max(1280);
                    let orig_h = REMOTE_ORIG_H.load(Ordering::Relaxed).max(720);
                    let fps = TARGET_FPS.load(Ordering::Relaxed);
                    if let Ok(path) = crate::windows::recorder::start_recording(orig_w, orig_h, fps) {
                        super::launcher::append_status_log(&format!(
                            "🔴 Recording started: {}",
                            path.display()
                        ));
                    }
                }
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                return LRESULT(0);
            } else if hit == 9001 {
                VIEWPORT_IS_ADMIN_MODE.store(false, Ordering::Relaxed);
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                return LRESULT(0);
            } else if hit == 9002 {
                VIEWPORT_IS_ADMIN_MODE.store(true, Ordering::Relaxed);
                crate::p2p::send_admin_request(crate::protocol::AdminReq::GetProcesses);
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                return LRESULT(0);
            } else if hit == 9003 {
                let muted = crate::windows::audio::toggle_audio_mute();
                info!("Audio toggle muted: {}", muted);
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                return LRESULT(0);
            } else if hit == 9004 {
                let new_priv = !REMOTE_PRIVACY_MODE.load(Ordering::Relaxed);
                REMOTE_PRIVACY_MODE.store(new_priv, Ordering::Relaxed);
                crate::p2p::send_admin_request(crate::protocol::AdminReq::SetPrivacyMode { enabled: new_priv });
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                return LRESULT(0);
            } else if hit == 9005 {
                let is_on = crate::windows::audio::toggle_intercom_voice();
                info!("Intercom voice: {}", is_on);
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                return LRESULT(0);
            } else if hit == 9101 {
                ACTIVE_REMOTE_MONITOR.store(0, Ordering::Relaxed);
                crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor { monitor_index: 0 });
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                return LRESULT(0);
            } else if hit == 9102 {
                ACTIVE_REMOTE_MONITOR.store(1, Ordering::Relaxed);
                crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor { monitor_index: 1 });
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                return LRESULT(0);
            } else if hit == 9103 {
                ACTIVE_REMOTE_MONITOR.store(2, Ordering::Relaxed);
                crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor { monitor_index: 2 });
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                return LRESULT(0);
            } else if hit >= 1000 {
                let close_idx = (hit - 1000) as usize;
                close_session_tab(hwnd, close_idx);
                return LRESULT(0);
            } else if hit == 999 {
                switch_session_tab(0);
                return LRESULT(0);
            } else if hit >= 0 {
                let tab_idx = hit as usize;
                switch_session_tab(tab_idx);
                if tab_idx > 0 {
                    DRAGGING_TAB_INDEX.store(hit, Ordering::Relaxed);
                    *DRAG_START_POS.lock().unwrap() = Some((client_x, client_y));
                    unsafe { let _ = SetCapture(hwnd); }
                }
                return LRESULT(0);
            }

            if client_y >= TOOLBAR_HEIGHT {
                let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
                if active == 0 {
                    super::launcher::handle_homepage_click(hwnd, client_x, client_y, client_w, client_h);
                    return LRESULT(0);
                } else if is_whiteboard_active() {
                    let screen_w = REMOTE_ORIG_W.load(Ordering::Relaxed) as i32;
                    let screen_h = REMOTE_ORIG_H.load(Ordering::Relaxed) as i32;
                    let mode = DISPLAY_MODE.load(Ordering::Relaxed);
                    if let Some((sx, sy)) = map_client_to_screen(client_x, client_y, client_w, client_h, screen_w, screen_h, TOOLBAR_HEIGHT, mode) {
                        let nx = (sx as f32 / screen_w as f32).clamp(0.0, 1.0);
                        let ny = (sy as f32 / screen_h as f32).clamp(0.0, 1.0);
                        start_stroke(nx, ny);
                        unsafe { let _ = SetCapture(hwnd); }
                    }
                    return LRESULT(0);
                } else if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                    if super::admin::handle_admin_click(client_x, client_y, client_w, client_h) {
                        return LRESULT(0);
                    }
                } else {
                    inject_remote_mouse(hwnd, client_x, client_y, true, false);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            if is_whiteboard_active() {
                end_stroke();
                unsafe { let _ = ReleaseCapture(); }
                return LRESULT(0);
            }

            let drag = DRAGGING_TAB_INDEX.swap(-1, Ordering::Relaxed);
            if drag >= 0 {
                unsafe { let _ = ReleaseCapture(); }
                if let Some((sx, sy)) = DRAG_START_POS.lock().unwrap().take() {
                    let dx = (client_x - sx).abs();
                    let dy = (client_y - sy).abs();
                    let mut rc = RECT::default();
                    unsafe { let _ = GetClientRect(hwnd, &mut rc); }
                    let client_w = rc.right - rc.left;
                    if dx > 80 || dy > 45 || client_y < -20 || client_x < -20 || client_x > client_w + 20 {
                        let mut pt = POINT { x: client_x, y: client_y };
                        unsafe { let _ = ClientToScreen(hwnd, &mut pt); }
                        detach_tab(drag as usize, pt.x, pt.y);
                        return LRESULT(0);
                    }
                }
            }

            if client_y >= TOOLBAR_HEIGHT {
                inject_remote_mouse(hwnd, client_x, client_y, false, false);
            }
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            if is_whiteboard_active() {
                clear_strokes();
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                return LRESULT(0);
            }

            if client_y >= TOOLBAR_HEIGHT && !VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                inject_remote_mouse(hwnd, client_x, client_y, true, true);
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            if client_y >= TOOLBAR_HEIGHT && !VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                inject_remote_mouse(hwnd, client_x, client_y, false, true);
            }
            LRESULT(0)
        }
        WM_MBUTTONDOWN => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            if client_y >= TOOLBAR_HEIGHT && !VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                inject_remote_mouse_middle(hwnd, client_x, client_y, true);
            }
            LRESULT(0)
        }
        WM_MBUTTONUP => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            if client_y >= TOOLBAR_HEIGHT && !VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                inject_remote_mouse_middle(hwnd, client_x, client_y, false);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            if is_whiteboard_active() {
                let screen_w = REMOTE_ORIG_W.load(Ordering::Relaxed) as i32;
                let screen_h = REMOTE_ORIG_H.load(Ordering::Relaxed) as i32;
                let mode = DISPLAY_MODE.load(Ordering::Relaxed);
                let mut rect = RECT::default();
                unsafe { let _ = GetClientRect(hwnd, &mut rect); }
                let client_w = rect.right - rect.left;
                let client_h = rect.bottom - rect.top;
                if let Some((sx, sy)) = map_client_to_screen(client_x, client_y, client_w, client_h, screen_w, screen_h, TOOLBAR_HEIGHT, mode) {
                    let nx = (sx as f32 / screen_w as f32).clamp(0.0, 1.0);
                    let ny = (sy as f32 / screen_h as f32).clamp(0.0, 1.0);
                    add_stroke_point(nx, ny);
                }
            }

            if client_y < TOOLBAR_HEIGHT {
                let prev = TAB_HOVER_STATE.load(Ordering::Relaxed);
                let hit = get_tab_hit_state(hwnd, client_x, client_y);
                if prev != hit {
                    TAB_HOVER_STATE.store(hit, Ordering::Relaxed);
                    unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                }
            } else {
                let prev = TAB_HOVER_STATE.load(Ordering::Relaxed);
                if prev != -1 {
                    TAB_HOVER_STATE.store(-1, Ordering::Relaxed);
                    unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                }
                let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
                if active == 0 {
                    let mut rect = RECT::default();
                    let _ = unsafe { GetClientRect(hwnd, &mut rect) };
                    let client_w = rect.right - rect.left;
                    let client_h = rect.bottom - rect.top;
                    super::launcher::handle_homepage_mousemove(hwnd, client_x, client_y, client_w, client_h);
                } else if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                    let mut rect = RECT::default();
                    let _ = unsafe { GetClientRect(hwnd, &mut rect) };
                    let client_w = rect.right - rect.left;
                    let client_h = rect.bottom - rect.top;
                    let hover = super::admin::get_admin_hover_state(client_x, client_y, client_w, client_h);
                    ADMIN_HOVER_STATE.store(hover, Ordering::Relaxed);
                } else if !is_whiteboard_active() {
                    let shape_id = crate::p2p::get_remote_cursor_shape();
                    let c = crate::windows::get_hcursor_for_shape(shape_id);
                    SetCursor(Some(c));
                    inject_remote_mouse_move(hwnd, client_x, client_y);
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
                    } else if is_whiteboard_active() {
                        let c = LoadCursorW(None, IDC_CROSS).unwrap_or_default();
                        SetCursor(Some(c));
                        return LRESULT(1);
                    } else if !VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed)
                        && ACTIVE_TAB_INDEX.load(Ordering::Relaxed) > 0
                    {
                        let shape_id = crate::p2p::get_remote_cursor_shape();
                        let c = crate::windows::get_hcursor_for_shape(shape_id);
                        SetCursor(Some(c));
                        return LRESULT(1);
                    }
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xffff) as i16 as i32;
            if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                super::admin::handle_admin_scroll(delta);
            } else {
                inject_remote_wheel(delta);
            }
            LRESULT(0)
        }
        WM_CHAR => {
            let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
            if active == 0 {
                return LRESULT(0);
            }
            if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                if let Some(c) = char::from_u32(wparam.0 as u32) {
                    let _ = super::admin::handle_admin_char(c);
                }
                return LRESULT(0);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
            if active == 0 {
                if wparam.0 == 0x0D {
                    super::launcher::handle_homepage_connect(hwnd);
                }
                return LRESULT(0);
            }
            if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                let _ = super::admin::handle_admin_key(wparam.0 as u32);
                return LRESULT(0);
            }
            if wparam.0 == 0x1B {
                if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                    SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
                } else if is_whiteboard_active() {
                    set_whiteboard_active(false);
                } else if is_quick_actions_open() {
                    set_quick_actions_open(false);
                } else {
                    inject_remote_key(0x1B, true);
                }
            } else if wparam.0 == 0x09 {
                let prev = SHOW_SETTINGS_MODAL.fetch_xor(true, Ordering::Relaxed);
                info!("Full-Screen Settings Page toggled: {}", !prev);
            } else if !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                let vk = wparam.0 as u32;
                let is_ctrl = (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x11) as i16) < 0;
                let is_alt = (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x12) as i16) < 0;

                // Hotkeys: Ctrl + Alt + 1/2/3
                if is_ctrl && is_alt {
                    if vk == 0x31 || vk == 0x61 {
                        ACTIVE_REMOTE_MONITOR.store(0, Ordering::Relaxed);
                        crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor { monitor_index: 0 });
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    } else if vk == 0x32 || vk == 0x62 {
                        ACTIVE_REMOTE_MONITOR.store(1, Ordering::Relaxed);
                        crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor { monitor_index: 1 });
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    } else if vk == 0x33 || vk == 0x63 {
                        ACTIVE_REMOTE_MONITOR.store(2, Ordering::Relaxed);
                        crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor { monitor_index: 2 });
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                }

                // Seamless Explorer Clipboard File Paste (Ctrl + V)
                if is_ctrl && (vk == 0x56 || vk == 0x76) {
                    let files = crate::p2p::read_clipboard_files();
                    if !files.is_empty() {
                        for f in files {
                            info!("Clipboard Pasting file: {:?}", f);
                            crate::p2p::send_file_transfer(f);
                        }
                        return LRESULT(0);
                    }
                }

                inject_remote_key(vk, true);
            }
            LRESULT(0)
        }
        WM_KEYUP => {
            let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
            if active == 0 {
                return LRESULT(0);
            }
            if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                return LRESULT(0);
            }
            if wparam.0 != 0x1B && wparam.0 != 0x09 && !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                inject_remote_key(wparam.0 as u32, false);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut ps);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_SIZE => {
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_DESTROY => {
            VIEWPORT_RUNNING.store(false, Ordering::SeqCst);
            let _ = RemoveClipboardFormatListener(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub unsafe fn launch_fullscreen_viewport(initial_target: Option<&str>) {
    let h_module = GetModuleHandleW(None).unwrap_or_default();
    let h_instance: HINSTANCE = h_module.into();

    let bg_color = theme::PK_BG;
    VIEWPORT_BRUSH = windows::Win32::Graphics::Gdi::CreateSolidBrush(bg_color);

    let wnd_class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: windows::Win32::UI::WindowsAndMessaging::CS_HREDRAW
            | windows::Win32::UI::WindowsAndMessaging::CS_VREDRAW,
        lpfnWndProc: Some(viewport_wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: h_instance,
        hIcon: windows::Win32::UI::WindowsAndMessaging::HICON(std::ptr::null_mut()),
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        hbrBackground: VIEWPORT_BRUSH,
        lpszMenuName: PCWSTR::null(),
        lpszClassName: w!("DeskyViewportClass"),
        hIconSm: windows::Win32::UI::WindowsAndMessaging::HICON(std::ptr::null_mut()),
    };

    let _ = RegisterClassExW(&wnd_class);

    init_session_tabs();
    if let Some(target) = initial_target {
        let store = SESSION_TABS.get_or_init(|| Mutex::new(Vec::new()));
        if let Ok(mut tabs) = store.lock() {
            tabs.push(SessionTab {
                id: target.to_string(),
                title: target.to_string(),
            });
        }
        ACTIVE_TAB_INDEX.store(1, Ordering::Relaxed);
    }

    let default_title = if let Some(target) = initial_target {
        format!("Desky | [{}]", target)
    } else {
        "Desky | pkasoft.com".to_string()
    };
    let wide_title: Vec<u16> =
        default_title.encode_utf16().chain(std::iter::once(0)).collect();

    let viewport_hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        w!("DeskyViewportClass"),
        PCWSTR(wide_title.as_ptr()),
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        100,
        100,
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
        info!("Modern Desky Viewport Window Launched! (HWND: {:?})", hwnd);

        start_viewport_render_thread(hwnd);

        if let Some(target) = initial_target {
            crate::p2p::start_client_session(
                target,
                TARGET_FPS.load(Ordering::Relaxed),
                CAPTURE_CURSOR.load(Ordering::Relaxed),
            );
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
