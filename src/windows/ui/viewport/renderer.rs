#![allow(unsafe_op_in_unsafe_fn)]
//! Dedicated Double-Buffered Remote Viewport Canvas Renderer
//!
//! Handles the high-performance 60-120 FPS render thread, StretchDIBits
//! video frame scaling (Auto Fit, Stretch, 1:1), Whiteboard GDI+ rendering,
//! file transfer HUD overlays, and toolbar compositing.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tracing::info;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, FillRect,
    GetDC, ReleaseDC, SelectObject, SetBrushOrgEx, SetStretchBltMode, StretchDIBits, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, COLORONCOLOR, DIB_RGB_COLORS, HALFTONE, HDC, HGDIOBJ, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, IsWindow};

use super::file_hud;
use super::overflow_menu::draw_overflow_menu;
use super::quick_actions::draw_quick_actions_popup;
use super::settings_modal::{
    draw_fullscreen_settings_view, CAPTURE_CURSOR, DISPLAY_MODE, SETTINGS_CATEGORY,
    SHOW_SETTINGS_MODAL, TARGET_FPS,
};
use super::tabs::{
    get_session_tabs, ACTIVE_TAB_INDEX, ADMIN_HOVER_STATE, HWND_VIEWPORT, SHOW_TOOLBAR,
    VIEWPORT_IS_ADMIN_MODE,
};
use super::toolbar::{draw_tab_bar, get_more_x, get_zap_x, TAB_HOVER_STATE, TOOLBAR_HEIGHT};
use super::whiteboard::{draw_whiteboard_overlay, draw_whiteboard_palette};
use crate::windows::ui::theme;

pub static VIEWPORT_RUNNING: AtomicBool = AtomicBool::new(false);
pub static REMOTE_ORIG_W: AtomicU32 = AtomicU32::new(0);
pub static REMOTE_ORIG_H: AtomicU32 = AtomicU32::new(0);

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
        let mut last_session_status = crate::p2p::SessionStatus::Idle;
        let mut last_status_hover: i32 = -1;
        let mut last_chat_open: bool = false;
        let mut last_chat_hover: i32 = -1;
        let mut last_ui_refresh = Instant::now();
        let mut last_edit_wanted: bool = true;
        let mut last_host_session_active: bool = false;

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
                let session_status = crate::p2p::get_session_status();
                let status_hover = super::session_status_view::get_status_hover_state();
                let chat_open = crate::p2p::is_chat_open();
                let chat_hover = super::chat_hud::get_chat_hover_state();
                let tabs = get_session_tabs();

                // Keep the native Remote Desk edit box in sync with overlays (connection
                // request, settings, tab switches). Only posts on transitions, so no flicker.
                let want_edit = crate::windows::ui::launcher::should_show_edit_target();
                if want_edit != last_edit_wanted {
                    last_edit_wanted = want_edit;
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        Some(target_hwnd),
                        crate::windows::ui::launcher::WM_APP_REDRAW,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    );
                }

                let has_host_session = crate::p2p::get_active_host_session_info().is_some();
                if has_host_session != last_host_session_active {
                    last_host_session_active = has_host_session;
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        Some(target_hwnd),
                        crate::windows::ui::launcher::WM_APP_REDRAW,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    );
                }

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
                    || cat_val != last_cat
                    || session_status != last_session_status
                    || status_hover != last_status_hover
                    || chat_open != last_chat_open
                    || chat_hover != last_chat_hover;

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
                    last_session_status = session_status.clone();
                    last_status_hover = status_hover;
                    last_chat_open = chat_open;
                    last_chat_hover = chat_hover;
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
                            let hp_hover = crate::windows::ui::launcher::get_hover_button();
                            crate::windows::ui::launcher::draw_homepage_view(mem_dc, client_w, client_h, hp_hover);
                            draw_tab_bar(mem_dc, client_w, TOOLBAR_HEIGHT, &tabs, 0, hover_state);
                        } else {
                            let is_err = matches!(session_status, crate::p2p::SessionStatus::Error(_));
                            if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
                                crate::windows::ui::admin::draw_admin_view(
                                    mem_dc,
                                    client_w,
                                    client_h - toolbar_h,
                                    ADMIN_HOVER_STATE.load(Ordering::Relaxed),
                                );
                            } else if !is_err && latest_frame.is_some() {
                                let remote_frame = latest_frame.as_ref().unwrap();
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
                            } else {
                                let tab_target = tabs.get(active_idx).map(|t| t.id.as_str()).unwrap_or("Unknown");
                                super::session_status_view::draw_session_status_view(
                                    mem_dc,
                                    client_w,
                                    client_h,
                                    toolbar_h,
                                    tab_target,
                                    &session_status,
                                    status_hover,
                                );
                            }

                            if toolbar_visible {
                                draw_tab_bar(mem_dc, client_w, TOOLBAR_HEIGHT, &tabs, active_idx, hover_state);
                                let zap_x = get_zap_x(client_w, tabs.len(), active_idx);
                                draw_quick_actions_popup(mem_dc, zap_x, 32, TOOLBAR_HEIGHT, hover_state);
                                if let Some(more_x) = get_more_x(client_w, tabs.len(), active_idx) {
                                    draw_overflow_menu(mem_dc, more_x, 32, TOOLBAR_HEIGHT, hover_state);
                                }
                                draw_whiteboard_palette(mem_dc, client_w, TOOLBAR_HEIGHT, hover_state);
                            }
                            file_hud::draw_file_transfer_hud(mem_dc, client_w, client_h);
                            if crate::p2p::is_chat_open() && active_idx > 0 {
                                super::chat_hud::draw_chat_hud(mem_dc, client_w, client_h, toolbar_h, chat_hover);
                            }
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
