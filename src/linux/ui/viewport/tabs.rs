//! Dedicated Multi-Tab Session Manager (Linux X11)
//!
//! Mirrors `src/windows/ui/viewport/tabs.rs` for managing `[ Home ]`,
//! active remote computer session tabs, monitor selection, and admin mode state.

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use super::settings_modal::{CAPTURE_CURSOR, SHOW_SETTINGS_MODAL, TARGET_FPS};
use crate::linux::ui::launcher::request_main_redraw;

#[derive(Clone, Debug, PartialEq)]
pub struct SessionTab {
    pub id: String,
    pub title: String,
}

static SESSION_TABS: OnceLock<Mutex<Vec<SessionTab>>> = OnceLock::new();
pub static ACTIVE_TAB_INDEX: AtomicUsize = AtomicUsize::new(0);
pub static ACTIVE_REMOTE_MONITOR: AtomicU32 = AtomicU32::new(0);

// Toolbar visibility: true = shown, false = collapsed/hidden
pub static SHOW_TOOLBAR: AtomicBool = AtomicBool::new(true);

// Silent Admin Mode toggle: false = normal desktop stream, true = background admin manager
pub static VIEWPORT_IS_ADMIN_MODE: AtomicBool = AtomicBool::new(false);
pub static ADMIN_HOVER_STATE: AtomicI32 = AtomicI32::new(-1);
pub static REMOTE_PRIVACY_MODE: AtomicBool = AtomicBool::new(false);

fn tabs_store() -> &'static Mutex<Vec<SessionTab>> {
    SESSION_TABS.get_or_init(|| {
        Mutex::new(vec![SessionTab {
            id: "home".to_string(),
            title: "Home".to_string(),
        }])
    })
}

pub fn init_session_tabs() {
    if let Ok(mut tabs) = tabs_store().lock() {
        tabs.clear();
        tabs.push(SessionTab {
            id: "home".to_string(),
            title: "Home".to_string(),
        });
    }
    ACTIVE_TAB_INDEX.store(0, Ordering::Relaxed);
}

pub fn get_session_tabs() -> Vec<SessionTab> {
    tabs_store().lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn switch_session_tab(tab_idx: usize) {
    SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
    let tabs = get_session_tabs();
    if tabs.get(tab_idx).is_some() {
        ACTIVE_TAB_INDEX.store(tab_idx, Ordering::Relaxed);
        request_main_redraw();
    }
}

pub fn close_session_tab(tab_idx: usize) {
    if tab_idx == 0 {
        return; // Home tab is permanent
    }
    let new_active = if let Ok(mut tabs) = tabs_store().lock() {
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

    REMOTE_PRIVACY_MODE.store(false, Ordering::Relaxed);
    crate::p2p::stop_client_session();
    ACTIVE_TAB_INDEX.store(new_active, Ordering::Relaxed);
    switch_session_tab(new_active);
    request_main_redraw();
}

pub fn spawn_or_add_tab_viewport(target: &str) {
    SHOW_SETTINGS_MODAL.store(false, Ordering::Relaxed);
    let mut idx = 1;
    if let Ok(mut tabs) = tabs_store().lock() {
        if tabs.is_empty() {
            tabs.push(SessionTab {
                id: "home".to_string(),
                title: "Home".to_string(),
            });
        }
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
    crate::p2p::start_client_session(
        target,
        TARGET_FPS.load(Ordering::Relaxed),
        CAPTURE_CURSOR.load(Ordering::Relaxed),
    );
    request_main_redraw();
}
