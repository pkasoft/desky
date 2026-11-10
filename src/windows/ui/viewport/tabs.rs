#![allow(unsafe_op_in_unsafe_fn)]
//! Dedicated Multi-Tab Session Manager & Window Detachment
//!
//! AnyDesk-style tabbed multi-session management across remote computers,
//! allowing dynamic addition, closing, switching, and drag-and-drop window detaching.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use tracing::info;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::DataExchange::AddClipboardFormatListener;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::DragAcceptFiles;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, IsIconic, IsWindow, SetForegroundWindow, SetWindowDisplayAffinity, SetWindowTextW,
    ShowWindow, SW_RESTORE, SW_SHOW, WDA_EXCLUDEFROMCAPTURE, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
    WS_VISIBLE,
};

use super::settings_modal::{CAPTURE_CURSOR, TARGET_FPS};

#[derive(Clone, Debug, PartialEq)]
pub struct SessionTab {
    pub id: String,
    pub title: String,
}

static SESSION_TABS: OnceLock<Mutex<Vec<SessionTab>>> = OnceLock::new();
pub static ACTIVE_TAB_INDEX: AtomicUsize = AtomicUsize::new(0);
pub(crate) static HWND_VIEWPORT: AtomicIsize = AtomicIsize::new(0);
pub(crate) static DRAGGING_TAB_INDEX: AtomicI32 = AtomicI32::new(-1);
pub(crate) static DRAG_START_POS: Mutex<Option<(i32, i32)>> = Mutex::new(None);
pub static ACTIVE_REMOTE_MONITOR: AtomicU32 = AtomicU32::new(0);

// Toolbar visibility: true = shown, false = collapsed/hidden
pub static SHOW_TOOLBAR: AtomicBool = AtomicBool::new(true);

// Silent Admin Mode toggle: false = normal desktop stream, true = background admin manager
pub static VIEWPORT_IS_ADMIN_MODE: AtomicBool = AtomicBool::new(false);
pub(crate) static ADMIN_HOVER_STATE: AtomicI32 = AtomicI32::new(-1);
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
    super::settings_modal::SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
    let store = SESSION_TABS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(tabs) = store.lock() {
        if let Some(tab) = tabs.get(tab_idx) {
            ACTIVE_TAB_INDEX.store(tab_idx, Ordering::Relaxed);
            let hwnd_raw = HWND_VIEWPORT.load(Ordering::Relaxed);
            if hwnd_raw != 0 {
                let hwnd = HWND(hwnd_raw as *mut _);
                unsafe {
                    if tab_idx == 0 {
                        crate::windows::ui::launcher::sync_edit_target_visibility();
                        let _ = SetWindowTextW(hwnd, w!("Desky | pkasoft.com"));
                    } else {
                        crate::windows::ui::launcher::set_edit_target_visibility(false);
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

pub fn detach_tab(tab_idx: usize, drop_screen_x: i32, drop_screen_y: i32) {
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
            crate::windows::ui::launcher::set_edit_target_visibility(false);
            let _ = SetWindowTextW(hwnd, PCWSTR(wide_title.as_ptr()));
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
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

pub unsafe fn spawn_detached_viewport_window(target: &str, screen_x: i32, screen_y: i32) {
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
