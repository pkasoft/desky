#![allow(unsafe_op_in_unsafe_fn)]
//! Dedicated Native Win32 Window Procedure & Lifecycle Engine
//!
//! Hosts the primary unified window, creates the responsive Remote Desk input control,
//! routes user mouse/keyboard input to remote machines or local tabs, and manages clipboard synchronization.

use std::sync::atomic::Ordering;
use tracing::info;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, EndPaint, InvalidateRect,
    ScreenToClient, SetBkColor, SetBkMode, SetTextColor, HBRUSH, HDC, OPAQUE, PAINTSTRUCT,
};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, RemoveClipboardFormatListener,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::Shell::{DragFinish, DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos, LoadCursorW, PostQuitMessage,
    SetCursor,
    IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_IBEAM, MINMAXINFO, WM_CHAR, WM_CLIPBOARDUPDATE, WM_CLOSE,
    WM_COMMAND, WM_CREATE, WM_CTLCOLOREDIT, WM_DESTROY, WM_DROPFILES, WM_ERASEBKGND,
    WM_GETMINMAXINFO, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_SETCURSOR, WM_SETFOCUS, WM_SIZE, WM_TIMER,
};

use super::input_injector::{
    inject_remote_key, inject_remote_mouse, inject_remote_mouse_middle, inject_remote_mouse_move,
    inject_remote_wheel, map_client_to_screen,
};
use super::overflow_menu::{
    hit_test_overflow_menu, is_overflow_menu_open, set_overflow_menu_open, toggle_overflow_menu,
};
use super::quick_actions::{
    execute_quick_action, hit_test_quick_actions, is_quick_actions_open, set_quick_actions_open,
    toggle_quick_actions,
};
use super::renderer::{REMOTE_ORIG_H, REMOTE_ORIG_W, VIEWPORT_RUNNING};
use super::settings_modal::{
    close_settings_modal, handle_settings_click, reset_settings_scroll, scroll_settings, CAPTURE_CURSOR, DISPLAY_MODE, SHOW_SETTINGS_MODAL, TARGET_FPS,
};
use super::tabs::{
    close_session_tab, detach_tab, get_session_tabs, switch_session_tab, ACTIVE_REMOTE_MONITOR, ACTIVE_TAB_INDEX,
    ADMIN_HOVER_STATE, DRAGGING_TAB_INDEX, DRAG_START_POS, HWND_VIEWPORT, REMOTE_PRIVACY_MODE,
    VIEWPORT_IS_ADMIN_MODE,
};
use super::toolbar::{
    get_more_x, get_tab_hit_state, get_zap_x, scroll_tabs, TAB_HOVER_STATE, TOOLBAR_HEIGHT,
};
use super::whiteboard::{
    add_stroke_point, clear_strokes, end_stroke, hit_test_whiteboard_palette, is_whiteboard_active,
    set_whiteboard_active, start_stroke, toggle_whiteboard, WHITEBOARD_COLOR_IDX,
};
use crate::windows::ui::launcher::{
    get_responsive_layout, set_edit_target_hwnd, sync_edit_target_visibility,
    set_main_window_hwnd, update_edit_layout, ID_EDIT_TARGET_CODE, WM_APP_BRING_FRONT,
    WM_APP_REDRAW, WM_APP_SHOW_PROGRESS, WM_APP_SYNC_HOST_UI,
};
use crate::windows::ui::remote_desk_edit::create_remote_desk_edit;
use crate::windows::ui::theme;

pub static mut VIEWPORT_BRUSH: HBRUSH = HBRUSH(std::ptr::null_mut());

pub unsafe fn execute_toolbar_action(hwnd: HWND, hit: i32) {
    if hit == 9011 {
        super::settings_modal::SETTINGS_CATEGORY.store(2, Ordering::Relaxed);
        reset_settings_scroll();
        SHOW_SETTINGS_MODAL.store(true, Ordering::Relaxed);
        sync_edit_target_visibility();
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 9006 {
        let prev = SHOW_SETTINGS_MODAL.fetch_xor(true, Ordering::Relaxed);
        if !prev {
            reset_settings_scroll();
        }
        sync_edit_target_visibility();
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 9007 {
        if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
            close_settings_modal(hwnd);
        }
        set_overflow_menu_open(false);
        toggle_quick_actions();
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 9020 {
        if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
            close_settings_modal(hwnd);
        }
        set_quick_actions_open(false);
        toggle_overflow_menu();
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 9008 {
        if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
            close_settings_modal(hwnd);
        }
        toggle_whiteboard();
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 9010 {
        if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
            close_settings_modal(hwnd);
        }
        let is_open = crate::p2p::toggle_chat();
        if is_open {
            crate::p2p::mark_chat_read();
        }
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 9009 {
        if crate::recorder::is_recording() {
            if let Some((path, total_bytes, total_frames, dur)) =
                crate::recorder::stop_recording()
            {
                crate::windows::ui::launcher::append_status_log(&format!(
                    "🎥 Session recording saved: {} ({} frames, {} KB, {:.1}s)",
                    path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    total_frames,
                    total_bytes / 1024,
                    dur.as_secs_f32()
                ));
            }
        } else {
            let orig_w = REMOTE_ORIG_W.load(Ordering::Relaxed).max(1280);
            let orig_h = REMOTE_ORIG_H.load(Ordering::Relaxed).max(720);
            let fps = TARGET_FPS.load(Ordering::Relaxed);
            if let Ok(path) = crate::recorder::start_recording(orig_w, orig_h, fps) {
                crate::windows::ui::launcher::append_status_log(&format!(
                    "🔴 Recording started: {}",
                    path.display()
                ));
            }
        }
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 9001 {
        if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
            close_settings_modal(hwnd);
        }
        VIEWPORT_IS_ADMIN_MODE.store(false, Ordering::Relaxed);
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 9002 {
        if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
            close_settings_modal(hwnd);
        }
        VIEWPORT_IS_ADMIN_MODE.store(true, Ordering::Relaxed);
        crate::p2p::send_admin_request(crate::protocol::AdminReq::GetProcesses);
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 9003 {
        let muted = crate::windows::audio::toggle_audio_mute();
        info!("Audio toggle muted: {}", muted);
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 9004 {
        let new_priv = !REMOTE_PRIVACY_MODE.load(Ordering::Relaxed);
        REMOTE_PRIVACY_MODE.store(new_priv, Ordering::Relaxed);
        crate::p2p::send_admin_request(crate::protocol::AdminReq::SetPrivacyMode {
            enabled: new_priv,
        });
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 9005 {
        let is_on = crate::windows::audio::toggle_intercom_voice();
        info!("Intercom voice: {}", is_on);
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 9100 {
        let cur = ACTIVE_REMOTE_MONITOR.load(Ordering::Relaxed);
        let next = (cur + 1) % 3;
        ACTIVE_REMOTE_MONITOR.store(next, Ordering::Relaxed);
        crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor {
            monitor_index: next,
        });
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 9101 {
        ACTIVE_REMOTE_MONITOR.store(0, Ordering::Relaxed);
        crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor {
            monitor_index: 0,
        });
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 9102 {
        ACTIVE_REMOTE_MONITOR.store(1, Ordering::Relaxed);
        crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor {
            monitor_index: 1,
        });
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 9103 {
        ACTIVE_REMOTE_MONITOR.store(2, Ordering::Relaxed);
        crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor {
            monitor_index: 2,
        });
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 997 {
        scroll_tabs(1);
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 998 {
        scroll_tabs(-1);
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if hit == 999 {
        if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
            close_settings_modal(hwnd);
        }
        switch_session_tab(0);
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

pub unsafe extern "system" fn viewport_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            HWND_VIEWPORT.store(hwnd.0 as isize, Ordering::Relaxed);
            set_main_window_hwnd(hwnd);

            let h_module = GetModuleHandleW(None).unwrap_or_default();
            let h_instance: HINSTANCE = h_module.into();

            let l = get_responsive_layout(1024, 680);

            // Create and subclass the Remote Desk Edit Control
            let edit_hwnd = create_remote_desk_edit(hwnd, l, h_instance);
            set_edit_target_hwnd(edit_hwnd);

            // Set refresh timer (every 2s) for status, network, and ping updates
            let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(Some(hwnd), 1001, 2000, None);
            let _ = AddClipboardFormatListener(hwnd);
            LRESULT(0)
        }
        WM_TIMER => {
            sync_edit_target_visibility();
            crate::windows::ui::sync_host_session_ui();
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_SIZE => {
            let w = (lparam.0 & 0xffff) as i16 as i32;
            let h = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            if w > 100 {
                update_edit_layout(w, h);
                sync_edit_target_visibility();
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_CTLCOLOREDIT => {
            let hdc = HDC(wparam.0 as *mut _);
            SetBkMode(hdc, OPAQUE);
            SetBkColor(hdc, theme::PK_INPUT_BG);
            SetTextColor(hdc, theme::PK_TEXT);
            LRESULT(crate::windows::ui::launcher::get_edit_bg_brush().0 as isize)
        }
        WM_COMMAND => {
            let code = (wparam.0 & 0xffff) as usize;
            if code == crate::windows::tray::ID_TRAY_OPEN {
                crate::windows::tray::show_main_window(hwnd);
                return LRESULT(0);
            } else if code == crate::windows::tray::ID_TRAY_QUIT {
                crate::windows::tray::remove_tray_icon(hwnd);
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            } else if code == ID_EDIT_TARGET_CODE {
                let notif = ((wparam.0 >> 16) & 0xffff) as u16;
                if notif == 0x0300 {
                    // EN_CHANGE
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_APP_REDRAW => {
            sync_edit_target_visibility();
            crate::windows::ui::sync_host_session_ui();
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_APP_SYNC_HOST_UI => {
            crate::windows::ui::sync_host_session_ui();
            LRESULT(0)
        }
        WM_APP_BRING_FRONT => {
            sync_edit_target_visibility();
            crate::windows::tray::show_main_window(hwnd);
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_APP_SHOW_PROGRESS => {
            crate::windows::ui::progress::spawn_progress_window();
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            if lparam.0 != 0 {
                let mmi = &mut *(lparam.0 as *mut MINMAXINFO);
                mmi.ptMinTrackSize.x = 720;
                mmi.ptMinTrackSize.y = 540;
            }
            LRESULT(0)
        }
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
                info!(
                    "Clipboard contains files: '{}' (Press Ctrl+V to paste into remote host)",
                    desc
                );
            } else if let Some(text) = crate::p2p::read_clipboard_text() {
                if !crate::platform::is_clipboard_recently_synced(&text) {
                    crate::platform::mark_clipboard_synced(&text);
                    crate::p2p::send_remote_clipboard(&text);
                }
            }
            LRESULT(0)
        }
        WM_SETFOCUS => {
            if let Some(text) = crate::p2p::read_clipboard_text() {
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
            let _ = GetClientRect(hwnd, &mut rect);
            let client_w = rect.right - rect.left;
            let client_h = rect.bottom - rect.top;

            if client_y < TOOLBAR_HEIGHT {
                let hit = get_tab_hit_state(hwnd, client_x, client_y);

                if hit >= 9000 || hit == 997 || hit == 998 {
                    execute_toolbar_action(hwnd, hit);
                    return LRESULT(0);
                } else if hit >= 1000 {
                    if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                        close_settings_modal(hwnd);
                    }
                    let close_idx = (hit - 1000) as usize;
                    close_session_tab(hwnd, close_idx);
                    return LRESULT(0);
                } else if hit == 999 {
                    if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                        close_settings_modal(hwnd);
                    }
                    switch_session_tab(0);
                    return LRESULT(0);
                } else if hit >= 0 {
                    if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                        close_settings_modal(hwnd);
                    }
                    let tab_idx = hit as usize;
                    switch_session_tab(tab_idx);
                    if tab_idx > 0 {
                        DRAGGING_TAB_INDEX.store(hit, Ordering::Relaxed);
                        *DRAG_START_POS.lock().unwrap() = Some((client_x, client_y));
                        let _ = SetCapture(hwnd);
                    }
                    return LRESULT(0);
                }
                return LRESULT(0);
            }

            if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                let _ = handle_settings_click(
                    hwnd,
                    client_w,
                    client_h,
                    client_x,
                    client_y,
                    TOOLBAR_HEIGHT,
                );
                return LRESULT(0);
            }

            if is_whiteboard_active() {
                let wb_hit =
                    hit_test_whiteboard_palette(client_w, TOOLBAR_HEIGHT, client_x, client_y);
                if wb_hit == 9399 {
                    clear_strokes();
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                } else if wb_hit >= 9301 && wb_hit <= 9304 {
                    WHITEBOARD_COLOR_IDX.store((wb_hit - 9301) as u8, Ordering::Relaxed);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                } else if wb_hit == 9300 {
                    return LRESULT(0);
                }
            }

            if is_overflow_menu_open() {
                let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
                let tabs = super::get_session_tabs();
                if let Some(more_x) = get_more_x(client_w, tabs.len(), active) {
                    let of_hit = hit_test_overflow_menu(more_x, 32, TOOLBAR_HEIGHT, client_x, client_y);
                    if of_hit > 0 && of_hit != 9020 {
                        set_overflow_menu_open(false);
                        execute_toolbar_action(hwnd, of_hit);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    } else if of_hit == 9020 {
                        return LRESULT(0);
                    } else {
                        set_overflow_menu_open(false);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            }

            if is_quick_actions_open() {
                let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
                let tabs = super::get_session_tabs();
                let zap_x = get_zap_x(client_w, tabs.len(), active);
                let qa_hit =
                    hit_test_quick_actions(zap_x, 32, TOOLBAR_HEIGHT, client_x, client_y);
                if qa_hit >= 9201 && qa_hit <= 9207 {
                    execute_quick_action(hwnd, qa_hit);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                } else if qa_hit == 9200 {
                    return LRESULT(0);
                } else {
                    set_quick_actions_open(false);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }

            if client_y >= TOOLBAR_HEIGHT {
                let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
                if active == 0 {
                    crate::windows::ui::launcher::handle_homepage_click(
                        hwnd, client_x, client_y, client_w, client_h,
                    );
                    return LRESULT(0);
                } else if is_whiteboard_active() {
                    let screen_w = REMOTE_ORIG_W.load(Ordering::Relaxed) as i32;
                    let screen_h = REMOTE_ORIG_H.load(Ordering::Relaxed) as i32;
                    let mode = DISPLAY_MODE.load(Ordering::Relaxed);
                    if let Some((sx, sy)) = map_client_to_screen(
                        client_x,
                        client_y,
                        client_w,
                        client_h,
                        screen_w,
                        screen_h,
                        TOOLBAR_HEIGHT,
                        mode,
                    ) {
                        let nx = (sx as f32 / screen_w as f32).clamp(0.0, 1.0);
                        let ny = (sy as f32 / screen_h as f32).clamp(0.0, 1.0);
                        start_stroke(nx, ny);
                        let _ = SetCapture(hwnd);
                    }
                    return LRESULT(0);
                } else if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                    if crate::windows::ui::admin::handle_admin_click(
                        client_x, client_y, client_w, client_h,
                    ) {
                        return LRESULT(0);
                    }
                } else {
                    if crate::p2p::is_chat_open() {
                        let chat_hit = super::chat_hud::hit_test_chat_hud(client_x, client_y, client_w, client_h, TOOLBAR_HEIGHT);
                        if chat_hit >= 0 {
                            if chat_hit == super::chat_hud::BTN_CHAT_CLOSE {
                                crate::p2p::set_chat_open(false);
                            } else if chat_hit == super::chat_hud::BTN_CHAT_CLEAR {
                                crate::p2p::clear_chat_history();
                            } else if chat_hit == super::chat_hud::BTN_CHAT_SEND {
                                super::chat_hud::submit_chat_input();
                            }
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        }
                    }

                    let latest_frame = crate::p2p::get_latest_remote_frame();
                    let status = crate::p2p::get_session_status();
                    let is_err = matches!(status, crate::p2p::SessionStatus::Error(_));

                    if is_err || latest_frame.is_none() {
                        let hit = super::session_status_view::hit_test_session_status_view(
                            client_x, client_y, client_w, client_h, TOOLBAR_HEIGHT, &status,
                        );
                        if hit == super::session_status_view::BTN_CANCEL_CONNECTION {
                            crate::p2p::stop_client_session();
                            switch_session_tab(0);
                            return LRESULT(0);
                        } else if hit == super::session_status_view::BTN_SUBMIT_PASSWORD {
                            let pwd = super::session_status_view::get_password_input();
                            if !pwd.is_empty() {
                                crate::p2p::submit_unattended_password(&pwd);
                                super::session_status_view::clear_password_input();
                                let _ = InvalidateRect(Some(hwnd), None, false);
                            }
                            return LRESULT(0);
                        } else if hit == super::session_status_view::BTN_RETRY_CONNECTION {
                            let tabs = get_session_tabs();
                            if let Some(tab) = tabs.get(active) {
                                crate::p2p::start_client_session(
                                    &tab.id,
                                    TARGET_FPS.load(Ordering::Relaxed),
                                    CAPTURE_CURSOR.load(Ordering::Relaxed),
                                );
                            }
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        } else if hit == super::session_status_view::BTN_CLOSE_SESSION {
                            close_session_tab(hwnd, active);
                            return LRESULT(0);
                        } else if hit == super::session_status_view::BTN_RETURN_TO_HOME {
                            switch_session_tab(0);
                            return LRESULT(0);
                        }
                    } else {
                        inject_remote_mouse(hwnd, client_x, client_y, true, false);
                    }
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            if is_whiteboard_active() {
                end_stroke();
                let _ = ReleaseCapture();
                return LRESULT(0);
            }

            let drag = DRAGGING_TAB_INDEX.swap(-1, Ordering::Relaxed);
            if drag >= 0 {
                let _ = ReleaseCapture();
                if let Some((sx, sy)) = DRAG_START_POS.lock().unwrap().take() {
                    let dx = (client_x - sx).abs();
                    let dy = (client_y - sy).abs();
                    let mut rc = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rc);
                    let client_w = rc.right - rc.left;
                    if dx > 80
                        || dy > 45
                        || client_y < -20
                        || client_x < -20
                        || client_x > client_w + 20
                    {
                        let mut pt = POINT {
                            x: client_x,
                            y: client_y,
                        };
                        let _ = ClientToScreen(hwnd, &mut pt);
                        detach_tab(drag as usize, pt.x, pt.y);
                        return LRESULT(0);
                    }
                }
            }

            if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                return LRESULT(0);
            }

            if client_y >= TOOLBAR_HEIGHT {
                let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
                if active == 0 {
                    return LRESULT(0);
                }
                if active > 0 && crate::p2p::is_chat_open() {
                    let mut rect = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rect);
                    let client_w = rect.right - rect.left;
                    let client_h = rect.bottom - rect.top;
                    if super::chat_hud::is_mouse_over_chat_hud(client_x, client_y, client_w, client_h, TOOLBAR_HEIGHT) {
                        return LRESULT(0);
                    }
                }
                inject_remote_mouse(hwnd, client_x, client_y, false, false);
            }
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                return LRESULT(0);
            }
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            if is_whiteboard_active() {
                clear_strokes();
                let _ = InvalidateRect(Some(hwnd), None, false);
                return LRESULT(0);
            }

            if client_y >= TOOLBAR_HEIGHT && !VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
                if active == 0 {
                    return LRESULT(0);
                }
                if active > 0 && crate::p2p::is_chat_open() {
                    let mut rect = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rect);
                    let client_w = rect.right - rect.left;
                    let client_h = rect.bottom - rect.top;
                    if super::chat_hud::is_mouse_over_chat_hud(client_x, client_y, client_w, client_h, TOOLBAR_HEIGHT) {
                        return LRESULT(0);
                    }
                }
                inject_remote_mouse(hwnd, client_x, client_y, true, true);
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                return LRESULT(0);
            }
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            if client_y >= TOOLBAR_HEIGHT && !VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
                if active == 0 {
                    return LRESULT(0);
                }
                if active > 0 && crate::p2p::is_chat_open() {
                    let mut rect = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rect);
                    let client_w = rect.right - rect.left;
                    let client_h = rect.bottom - rect.top;
                    if super::chat_hud::is_mouse_over_chat_hud(client_x, client_y, client_w, client_h, TOOLBAR_HEIGHT) {
                        return LRESULT(0);
                    }
                }
                inject_remote_mouse(hwnd, client_x, client_y, false, true);
            }
            LRESULT(0)
        }
        WM_MBUTTONDOWN => {
            if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                return LRESULT(0);
            }
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            if client_y >= TOOLBAR_HEIGHT && !VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
                if active == 0 {
                    return LRESULT(0);
                }
                if active > 0 && crate::p2p::is_chat_open() {
                    let mut rect = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rect);
                    let client_w = rect.right - rect.left;
                    let client_h = rect.bottom - rect.top;
                    if super::chat_hud::is_mouse_over_chat_hud(client_x, client_y, client_w, client_h, TOOLBAR_HEIGHT) {
                        return LRESULT(0);
                    }
                }
                inject_remote_mouse_middle(hwnd, client_x, client_y, true);
            }
            LRESULT(0)
        }
        WM_MBUTTONUP => {
            if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                return LRESULT(0);
            }
            let client_x = (lparam.0 & 0xffff) as i16 as i32;
            let client_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;

            if client_y >= TOOLBAR_HEIGHT && !VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
                if active == 0 {
                    return LRESULT(0);
                }
                if active > 0 && crate::p2p::is_chat_open() {
                    let mut rect = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rect);
                    let client_w = rect.right - rect.left;
                    let client_h = rect.bottom - rect.top;
                    if super::chat_hud::is_mouse_over_chat_hud(client_x, client_y, client_w, client_h, TOOLBAR_HEIGHT) {
                        return LRESULT(0);
                    }
                }
                inject_remote_mouse_middle(hwnd, client_x, client_y, false);
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
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                let cursor_name = if hit >= 0 { IDC_HAND } else { IDC_ARROW };
                let c = LoadCursorW(None, cursor_name).unwrap_or_default();
                SetCursor(Some(c));
                return LRESULT(0);
            }

            let prev = TAB_HOVER_STATE.load(Ordering::Relaxed);
            if prev != -1 {
                TAB_HOVER_STATE.store(-1, Ordering::Relaxed);
                let _ = InvalidateRect(Some(hwnd), None, false);
            }

            if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let client_w = rect.right - rect.left;
                let client_h = rect.bottom - rect.top;
                super::settings_modal::handle_settings_mousemove(
                    hwnd,
                    client_w,
                    client_h,
                    client_x,
                    client_y,
                    TOOLBAR_HEIGHT,
                );
                return LRESULT(0);
            }
            if is_overflow_menu_open() {
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let client_w = rect.right - rect.left;
                let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
                let tabs = super::get_session_tabs();
                if let Some(more_x) = get_more_x(client_w, tabs.len(), active) {
                    let of_hit = hit_test_overflow_menu(more_x, 32, TOOLBAR_HEIGHT, client_x, client_y);
                    if of_hit > 0 {
                        let prev = TAB_HOVER_STATE.load(Ordering::Relaxed);
                        if prev != of_hit {
                            TAB_HOVER_STATE.store(of_hit, Ordering::Relaxed);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                        let c = LoadCursorW(None, IDC_HAND).unwrap_or_default();
                        SetCursor(Some(c));
                        return LRESULT(0);
                    }
                }
            }

            if is_quick_actions_open() {
                let c = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
                SetCursor(Some(c));
                return LRESULT(0);
            }

            if is_whiteboard_active() {
                let screen_w = REMOTE_ORIG_W.load(Ordering::Relaxed) as i32;
                let screen_h = REMOTE_ORIG_H.load(Ordering::Relaxed) as i32;
                let mode = DISPLAY_MODE.load(Ordering::Relaxed);
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let client_w = rect.right - rect.left;
                let client_h = rect.bottom - rect.top;
                if let Some((sx, sy)) = map_client_to_screen(
                    client_x,
                    client_y,
                    client_w,
                    client_h,
                    screen_w,
                    screen_h,
                    TOOLBAR_HEIGHT,
                    mode,
                ) {
                    let nx = (sx as f32 / screen_w as f32).clamp(0.0, 1.0);
                    let ny = (sy as f32 / screen_h as f32).clamp(0.0, 1.0);
                    add_stroke_point(nx, ny);
                }
            }

            let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
            if active > 0 && crate::p2p::is_chat_open() {
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let client_w = rect.right - rect.left;
                let client_h = rect.bottom - rect.top;
                super::chat_hud::handle_chat_mousemove(
                    hwnd, client_x, client_y, client_w, client_h, TOOLBAR_HEIGHT,
                );
                if super::chat_hud::is_mouse_over_chat_hud(
                    client_x, client_y, client_w, client_h, TOOLBAR_HEIGHT,
                ) {
                    let hit = super::chat_hud::hit_test_chat_hud(
                        client_x, client_y, client_w, client_h, TOOLBAR_HEIGHT,
                    );
                    let cursor_name = if hit == super::chat_hud::AREA_CHAT_INPUT {
                        IDC_IBEAM
                    } else if hit == super::chat_hud::BTN_CHAT_SEND
                        || hit == super::chat_hud::BTN_CHAT_CLOSE
                        || hit == super::chat_hud::BTN_CHAT_CLEAR
                    {
                        IDC_HAND
                    } else {
                        IDC_ARROW
                    };
                    let c = LoadCursorW(None, cursor_name).unwrap_or_default();
                    SetCursor(Some(c));
                    return LRESULT(0);
                }
            }
            if active == 0 {
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let client_w = rect.right - rect.left;
                let client_h = rect.bottom - rect.top;
                crate::windows::ui::launcher::handle_homepage_mousemove(
                    hwnd, client_x, client_y, client_w, client_h,
                );
                return LRESULT(0);
            } else if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let client_w = rect.right - rect.left;
                let client_h = rect.bottom - rect.top;
                let hover = crate::windows::ui::admin::get_admin_hover_state(
                    client_x, client_y, client_w, client_h,
                );
                ADMIN_HOVER_STATE.store(hover, Ordering::Relaxed);
            } else if !is_whiteboard_active() {
                let latest_frame = crate::p2p::get_latest_remote_frame();
                let status = crate::p2p::get_session_status();
                let is_err = matches!(status, crate::p2p::SessionStatus::Error(_));

                if is_err || latest_frame.is_none() {
                    let mut rect = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rect);
                    let client_w = rect.right - rect.left;
                    let client_h = rect.bottom - rect.top;
                    super::session_status_view::handle_status_mousemove(
                        hwnd, client_x, client_y, client_w, client_h, TOOLBAR_HEIGHT, &status,
                    );
                } else {
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
                    if crate::p2p::is_chat_open() {
                        let mut rect = RECT::default();
                        let _ = GetClientRect(hwnd, &mut rect);
                        let client_w = rect.right - rect.left;
                        let client_h = rect.bottom - rect.top;
                        if super::chat_hud::is_mouse_over_chat_hud(
                            cursor_pt.x, cursor_pt.y, client_w, client_h, TOOLBAR_HEIGHT,
                        ) {
                            let hit = super::chat_hud::hit_test_chat_hud(
                                cursor_pt.x, cursor_pt.y, client_w, client_h, TOOLBAR_HEIGHT,
                            );
                            let cursor_name = if hit == super::chat_hud::AREA_CHAT_INPUT {
                                IDC_IBEAM
                            } else if hit == super::chat_hud::BTN_CHAT_SEND
                                || hit == super::chat_hud::BTN_CHAT_CLOSE
                                || hit == super::chat_hud::BTN_CHAT_CLEAR
                            {
                                IDC_HAND
                            } else {
                                IDC_ARROW
                            };
                            let c = LoadCursorW(None, cursor_name).unwrap_or_default();
                            SetCursor(Some(c));
                            return LRESULT(1);
                        }
                    }

                    let latest_frame = crate::p2p::get_latest_remote_frame();
                    let status = crate::p2p::get_session_status();
                    let is_err = matches!(status, crate::p2p::SessionStatus::Error(_));

                    if is_err || latest_frame.is_none() {
                        let status_hover = super::session_status_view::get_status_hover_state();
                        let cursor_name = if status_hover > 0 { IDC_HAND } else { IDC_ARROW };
                        let c = LoadCursorW(None, cursor_name).unwrap_or_default();
                        SetCursor(Some(c));
                        return LRESULT(1);
                    } else {
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
            if is_quick_actions_open() || is_overflow_menu_open() {
                return LRESULT(0);
            }
            let delta = ((wparam.0 >> 16) & 0xffff) as i16 as i32;
            let screen_x = (lparam.0 & 0xffff) as i16 as i32;
            let screen_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let mut pt = POINT { x: screen_x, y: screen_y };
            let _ = ScreenToClient(hwnd, &mut pt);

            if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                scroll_settings(delta);
                let _ = InvalidateRect(Some(hwnd), None, false);
                return LRESULT(0);
            }

            if pt.y < TOOLBAR_HEIGHT {
                scroll_tabs(delta);
                let _ = InvalidateRect(Some(hwnd), None, false);
                return LRESULT(0);
            }

            let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
            if active == 0 {
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let l = get_responsive_layout(rect.right - rect.left, rect.bottom - rect.top);
                if pt.x >= l.col1_x && pt.x <= l.col1_x + l.col_w && pt.y >= l.col_y && pt.y <= l.col_y + l.col_h {
                    crate::windows::ui::launcher::scroll_card3(delta);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return LRESULT(0);
            }
            if active > 0 && crate::p2p::is_chat_open() {
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let client_w = rect.right - rect.left;
                let client_h = rect.bottom - rect.top;
                if super::chat_hud::is_mouse_over_chat_hud(pt.x, pt.y, client_w, client_h, TOOLBAR_HEIGHT) {
                    super::chat_hud::scroll_chat_hud(delta);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
            }

            if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                crate::windows::ui::admin::handle_admin_scroll(delta);
            } else {
                inject_remote_wheel(delta);
            }
            LRESULT(0)
        }
        WM_CHAR => {
            if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                return LRESULT(0);
            }
            let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
            if active == 0 {
                return LRESULT(0);
            }
            if let Some(c) = char::from_u32(wparam.0 as u32) {
                if crate::p2p::is_chat_open() {
                    if c == '\r' || c == '\n' {
                        super::chat_hud::submit_chat_input();
                    } else if c == '\x08' {
                        super::chat_hud::pop_chat_char();
                    } else if !c.is_control() {
                        super::chat_hud::append_chat_char(c);
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                let status = crate::p2p::get_session_status();
                if matches!(status, crate::p2p::SessionStatus::PasswordRequired(_)) {
                    if c == '\r' || c == '\n' {
                        let pwd = super::session_status_view::get_password_input();
                        if !pwd.is_empty() {
                            crate::p2p::submit_unattended_password(&pwd);
                            super::session_status_view::clear_password_input();
                        }
                    } else if c == '\x08' {
                        super::session_status_view::pop_password_char();
                    } else if !c.is_control() {
                        super::session_status_view::append_password_char(c);
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                    let _ = crate::windows::ui::admin::handle_admin_char(c);
                    return LRESULT(0);
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let active = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
            if active == 0 {
                if wparam.0 == 0x0D {
                    crate::windows::ui::launcher::handle_homepage_connect(hwnd);
                }
                return LRESULT(0);
            }
            if crate::p2p::is_chat_open() && wparam.0 != 0x1B {
                return LRESULT(0);
            }
            if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                let _ = crate::windows::ui::admin::handle_admin_key(wparam.0 as u32);
                return LRESULT(0);
            }
            if wparam.0 == 0x1B {
                if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                    super::settings_modal::close_settings_modal(hwnd);
                } else if crate::p2p::is_chat_open() {
                    crate::p2p::set_chat_open(false);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                } else if is_whiteboard_active() {
                    set_whiteboard_active(false);
                } else if is_quick_actions_open() {
                    set_quick_actions_open(false);
                } else {
                    inject_remote_key(0x1B, true);
                }
            } else if wparam.0 == 0x09 {
                let prev = SHOW_SETTINGS_MODAL.fetch_xor(true, Ordering::Relaxed);
                if !prev {
                    reset_settings_scroll();
                }
                sync_edit_target_visibility();
                info!("Full-Screen Settings Page toggled: {}", !prev);
            } else if !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                let vk = wparam.0 as u32;
                let is_ctrl =
                    (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x11) as i16) < 0;
                let is_alt =
                    (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x12) as i16) < 0;

                // Hotkeys: Ctrl + Alt + 1/2/3
                if is_ctrl && is_alt {
                    if vk == 0x31 || vk == 0x61 {
                        ACTIVE_REMOTE_MONITOR.store(0, Ordering::Relaxed);
                        crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor {
                            monitor_index: 0,
                        });
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    } else if vk == 0x32 || vk == 0x62 {
                        ACTIVE_REMOTE_MONITOR.store(1, Ordering::Relaxed);
                        crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor {
                            monitor_index: 1,
                        });
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    } else if vk == 0x33 || vk == 0x63 {
                        ACTIVE_REMOTE_MONITOR.store(2, Ordering::Relaxed);
                        crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor {
                            monitor_index: 2,
                        });
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                }

                // Seamless Explorer Clipboard File Paste & Text Sync (Ctrl + V)
                if is_ctrl && (vk == 0x56 || vk == 0x76) {
                    let files = crate::p2p::read_clipboard_files();
                    if !files.is_empty() {
                        for f in files {
                            info!("Clipboard Pasting file: {:?}", f);
                            crate::p2p::send_file_transfer(f);
                        }
                        return LRESULT(0);
                    } else if let Some(text) = crate::p2p::read_clipboard_text() {
                        if !text.is_empty() {
                            crate::p2p::send_remote_clipboard(&text);
                        }
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
            if wparam.0 != 0x1B
                && wparam.0 != 0x09
                && !SHOW_SETTINGS_MODAL.load(Ordering::Relaxed)
            {
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
        crate::windows::tray::WM_TRAYICON => {
            let event = (lparam.0 & 0xffff) as u32;
            match event {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                    crate::windows::tray::show_main_window(hwnd);
                }
                WM_RBUTTONUP => {
                    crate::windows::tray::show_tray_context_menu(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if crate::identity::load_preferences().close_to_tray {
                crate::windows::tray::hide_main_window(hwnd);
                LRESULT(0)
            } else {
                crate::windows::tray::remove_tray_icon(hwnd);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_DESTROY => {
            crate::windows::tray::remove_tray_icon(hwnd);
            VIEWPORT_RUNNING.store(false, Ordering::SeqCst);
            let _ = RemoveClipboardFormatListener(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

