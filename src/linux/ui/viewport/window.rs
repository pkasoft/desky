//! Dedicated Native X11 Window Procedure & Lifecycle Engine
//!
//! Mirrors `src/windows/ui/viewport/window.rs` 1:1 by hosting the unified X11 window,
//! rendering into a `PixelCanvas` with anti-aliased TrueType fonts, and routing
//! mouse/keyboard events to toolbar actions, settings modal, quick actions,
//! overflow menu, whiteboard, chat HUD, admin view, homepage cards, or remote input injection.

use anyhow::Result;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tracing::info;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

use super::chat_hud::{handle_chat_click, handle_chat_key, is_mouse_over_chat_hud};
use super::input_injector::{
    inject_remote_key, inject_remote_mouse_button, inject_remote_mouse_move, map_client_to_screen,
};
use super::overflow_menu::{
    hit_test_overflow_menu, is_overflow_menu_open, set_overflow_menu_open, toggle_overflow_menu,
};
use super::quick_actions::{
    execute_quick_action, hit_test_quick_actions, is_quick_actions_open, set_quick_actions_open,
    toggle_quick_actions,
};
use super::renderer::{render_viewport_frame, REMOTE_ORIG_H, REMOTE_ORIG_W};
use super::session_status_view::{handle_password_key_input, handle_status_view_click};
use super::settings_modal::{
    close_settings_modal, handle_settings_click, SETTINGS_CATEGORY, SHOW_SETTINGS_MODAL,
    TARGET_FPS,
};
use super::tabs::{
    close_session_tab, get_session_tabs, init_session_tabs, switch_session_tab,
    ACTIVE_REMOTE_MONITOR, ACTIVE_TAB_INDEX, REMOTE_PRIVACY_MODE, VIEWPORT_IS_ADMIN_MODE,
};
use super::toolbar::{
    get_more_x, get_tab_hit_state, get_zap_x, scroll_tabs, TAB_HOVER_STATE, TOOLBAR_HEIGHT,
};
use super::whiteboard::{
    add_stroke_point, clear_strokes, end_stroke, hit_test_whiteboard_palette,
    is_whiteboard_active, set_whiteboard_active, start_stroke, toggle_whiteboard,
    WHITEBOARD_COLOR_IDX,
};
use crate::linux::ui::admin::{handle_admin_click, handle_admin_key};
use crate::linux::ui::launcher::{
    append_status_log, handle_homepage_click, handle_homepage_connect, BRING_TO_FRONT,
    NEEDS_REDRAW, RUNNING,
};
use crate::linux::ui::remote_desk_edit::{get_remote_id_raw, handle_remote_desk_edit_key};
use crate::linux::ui::theme::{PixelCanvas, PK_BG};

pub fn execute_toolbar_action(hit: i32) {
    if hit == 9011 {
        SETTINGS_CATEGORY.store(2, Ordering::Relaxed);
        SHOW_SETTINGS_MODAL.store(true, Ordering::Relaxed);
    } else if hit == 9006 {
        SHOW_SETTINGS_MODAL.fetch_xor(true, Ordering::Relaxed);
    } else if hit == 9007 {
        if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
            close_settings_modal();
        }
        set_overflow_menu_open(false);
        toggle_quick_actions();
    } else if hit == 9020 {
        if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
            close_settings_modal();
        }
        set_quick_actions_open(false);
        toggle_overflow_menu();
    } else if hit == 9008 {
        if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
            close_settings_modal();
        }
        toggle_whiteboard();
    } else if hit == 9010 {
        if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
            close_settings_modal();
        }
        let is_open = crate::p2p::toggle_chat();
        if is_open {
            crate::p2p::mark_chat_read();
        }
    } else if hit == 9009 {
        if crate::recorder::is_recording() {
            if let Some((path, total_bytes, total_frames, dur)) = crate::recorder::stop_recording() {
                append_status_log(&format!(
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
                append_status_log(&format!("🔴 Recording started: {}", path.display()));
            }
        }
    } else if hit == 9001 {
        if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
            close_settings_modal();
        }
        VIEWPORT_IS_ADMIN_MODE.store(false, Ordering::Relaxed);
    } else if hit == 9002 {
        if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
            close_settings_modal();
        }
        VIEWPORT_IS_ADMIN_MODE.store(true, Ordering::Relaxed);
        crate::p2p::send_admin_request(crate::protocol::AdminReq::GetProcesses);
    } else if hit == 9004 {
        let new_priv = !REMOTE_PRIVACY_MODE.load(Ordering::Relaxed);
        REMOTE_PRIVACY_MODE.store(new_priv, Ordering::Relaxed);
        crate::p2p::send_admin_request(crate::protocol::AdminReq::SetPrivacyMode {
            enabled: new_priv,
        });
    } else if hit == 9100 {
        let cur = ACTIVE_REMOTE_MONITOR.load(Ordering::Relaxed);
        let next = (cur + 1) % 3;
        ACTIVE_REMOTE_MONITOR.store(next, Ordering::Relaxed);
        crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor {
            monitor_index: next,
        });
    } else if hit == 9101 {
        ACTIVE_REMOTE_MONITOR.store(0, Ordering::Relaxed);
        crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor {
            monitor_index: 0,
        });
    } else if hit == 9102 {
        ACTIVE_REMOTE_MONITOR.store(1, Ordering::Relaxed);
        crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor {
            monitor_index: 1,
        });
    } else if hit == 9103 {
        ACTIVE_REMOTE_MONITOR.store(2, Ordering::Relaxed);
        crate::p2p::send_admin_request(crate::protocol::AdminReq::SwitchMonitor {
            monitor_index: 2,
        });
    } else if hit == 997 {
        scroll_tabs(1);
    } else if hit == 998 {
        scroll_tabs(-1);
    } else if hit == 999 {
        if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
            close_settings_modal();
        }
        switch_session_tab(0);
    }
}

pub fn run_viewport_x11_window() -> Result<()> {
    init_session_tabs();

    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let root_depth = screen.root_depth;
    let root_visual = screen.root_visual;
    let root_win = screen.root;

    let win_id = conn.generate_id()?;
    let mut win_w = 1024u16;
    let mut win_h = 680u16;

    let win_aux = CreateWindowAux::new()
        .background_pixel(PK_BG)
        .event_mask(
            EventMask::EXPOSURE
                | EventMask::KEY_PRESS
                | EventMask::KEY_RELEASE
                | EventMask::BUTTON_PRESS
                | EventMask::BUTTON_RELEASE
                | EventMask::POINTER_MOTION
                | EventMask::STRUCTURE_NOTIFY,
        );

    conn.create_window(
        root_depth,
        win_id,
        root_win,
        80,
        60,
        win_w,
        win_h,
        0,
        WindowClass::INPUT_OUTPUT,
        root_visual,
        &win_aux,
    )?;

    let title = "Desky | pkasoft.com";
    conn.change_property8(
        PropMode::REPLACE,
        win_id,
        AtomEnum::WM_NAME,
        AtomEnum::STRING,
        title.as_bytes(),
    )?;

    let wm_protocols = conn.intern_atom(false, b"WM_PROTOCOLS")?.reply()?.atom;
    let wm_delete_window = conn.intern_atom(false, b"WM_DELETE_WINDOW")?.reply()?.atom;
    conn.change_property32(
        PropMode::REPLACE,
        win_id,
        wm_protocols,
        AtomEnum::ATOM,
        &[wm_delete_window],
    )?;

    conn.map_window(win_id)?;
    let _ = conn.set_input_focus(InputFocus::PARENT, win_id, x11rb::CURRENT_TIME);
    conn.flush()?;

    info!(
        "Native X11 pkasoft window created and mapped (Window ID: 0x{:x})",
        win_id
    );

    let gc_blit = conn.generate_id()?;
    conn.create_gc(gc_blit, win_id, &CreateGCAux::new().graphics_exposures(0))?;

    let mut canvas = PixelCanvas::new(win_w, win_h);
    let mut last_frame_id: u32 = 0;
    let mut last_status_tick = Instant::now();

    NEEDS_REDRAW.store(true, Ordering::Relaxed);

    while RUNNING.load(Ordering::Relaxed) {
        if BRING_TO_FRONT.swap(false, Ordering::Relaxed) {
            let _ = conn.configure_window(
                win_id,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            );
            let _ = conn.set_input_focus(InputFocus::PARENT, win_id, x11rb::CURRENT_TIME);
            let _ = conn.flush();
        }

        while let Some(event) = conn.poll_for_event()? {
            match event {
                x11rb::protocol::Event::Expose(ex) => {
                    if ex.count == 0 {
                        NEEDS_REDRAW.store(true, Ordering::Relaxed);
                    }
                }
                x11rb::protocol::Event::ConfigureNotify(cfg) => {
                    if cfg.width > 100
                        && cfg.height > 100
                        && (cfg.width != win_w || cfg.height != win_h)
                    {
                        win_w = cfg.width;
                        win_h = cfg.height;
                        NEEDS_REDRAW.store(true, Ordering::Relaxed);
                    }
                }
                x11rb::protocol::Event::ButtonPress(bp) => {
                    let _ = conn.set_input_focus(InputFocus::PARENT, win_id, x11rb::CURRENT_TIME);
                    dispatch_mouse_button(win_w, win_h, bp.detail, bp.event_x, bp.event_y, true);
                    NEEDS_REDRAW.store(true, Ordering::Relaxed);
                }
                x11rb::protocol::Event::ButtonRelease(br) => {
                    dispatch_mouse_button(win_w, win_h, br.detail, br.event_x, br.event_y, false);
                }
                x11rb::protocol::Event::MotionNotify(mn) => {
                    dispatch_mouse_motion(win_w, win_h, mn.event_x, mn.event_y);
                }
                x11rb::protocol::Event::KeyPress(kp) => {
                    dispatch_key_event(kp.detail, kp.state, true);
                    NEEDS_REDRAW.store(true, Ordering::Relaxed);
                }
                x11rb::protocol::Event::KeyRelease(kr) => {
                    dispatch_key_event(kr.detail, kr.state, false);
                }
                x11rb::protocol::Event::ClientMessage(cm) => {
                    if cm.data.as_data32()[0] == wm_delete_window {
                        info!("WM_DELETE_WINDOW received, closing Desky window.");
                        RUNNING.store(false, Ordering::Relaxed);
                        break;
                    }
                }
                x11rb::protocol::Event::DestroyNotify(_) => {
                    info!("X11 Window destroyed, shutting down.");
                    RUNNING.store(false, Ordering::Relaxed);
                    break;
                }
                _ => {}
            }
        }

        if !RUNNING.load(Ordering::Relaxed) {
            break;
        }

        let active_tab = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
        let latest_frame = if active_tab > 0 {
            crate::p2p::get_latest_remote_frame()
        } else {
            None
        };
        let cur_fid = latest_frame.as_ref().map(|f| f.frame_id).unwrap_or(0);
        if cur_fid != last_frame_id {
            last_frame_id = cur_fid;
            NEEDS_REDRAW.store(true, Ordering::Relaxed);
        }

        if last_status_tick.elapsed() >= Duration::from_millis(500) {
            last_status_tick = Instant::now();
            NEEDS_REDRAW.store(true, Ordering::Relaxed);
        }

        if NEEDS_REDRAW.swap(false, Ordering::Relaxed) {
            canvas.resize(win_w, win_h);
            render_viewport_frame(&mut canvas, win_w, win_h, latest_frame.as_deref());
            canvas.present_to_x11(&conn, win_id, gc_blit, root_depth);
            let _ = conn.flush();
        }

        std::thread::sleep(Duration::from_millis(12));
    }

    Ok(())
}

fn dispatch_mouse_button(win_w: u16, win_h: u16, button: u8, mx: i16, my: i16, is_down: bool) {
    let client_x = mx as i32;
    let client_y = my as i32;
    let client_w = win_w as i32;
    let client_h = win_h as i32;
    let tb_h = TOOLBAR_HEIGHT as i32;

    if !is_down && button == 1 && is_whiteboard_active() {
        end_stroke();
        NEEDS_REDRAW.store(true, Ordering::Relaxed);
    }

    // 1. Top Toolbar / Tab Bar
    if client_y < tb_h {
        if is_down && button == 1 {
            let hit = get_tab_hit_state(client_w, client_x, client_y);
            if hit >= 1000 && hit < 2000 {
                close_session_tab((hit - 1000) as usize);
            } else if hit >= 997 {
                execute_toolbar_action(hit);
            } else if hit >= 0 {
                if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
                    close_settings_modal();
                }
                switch_session_tab(hit as usize);
            }
        }
        return;
    }

    // 2. Fullscreen Settings Modal
    if SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
        if is_down && button == 1 {
            let _ = handle_settings_click(client_x, client_y, client_w, tb_h);
        }
        return;
    }

    let active_tab = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);

    // 3. Homepage View
    if active_tab == 0 {
        if is_down && button == 1 {
            let _ = handle_homepage_click(mx, my, win_w, win_h);
        }
        return;
    }

    // 4. Quick Actions & Overflow Dropdowns
    if is_down && button == 1 {
        let tabs = get_session_tabs();
        if is_quick_actions_open() {
            let zap_x = get_zap_x(client_w, tabs.len(), active_tab);
            let qa_hit = hit_test_quick_actions(zap_x, 32, tb_h, client_x, client_y);
            if qa_hit > 9200 {
                execute_quick_action(qa_hit);
                return;
            } else if qa_hit < 0 {
                set_quick_actions_open(false);
                return;
            }
        }
        if is_overflow_menu_open() {
            if let Some(more_x) = get_more_x(client_w, tabs.len(), active_tab) {
                let of_hit = hit_test_overflow_menu(more_x, 32, tb_h, client_x, client_y);
                if of_hit > 0 && of_hit != 9020 {
                    set_overflow_menu_open(false);
                    execute_toolbar_action(of_hit);
                    return;
                } else if of_hit < 0 {
                    set_overflow_menu_open(false);
                    return;
                }
            }
        }
    }

    // 5. Chat HUD
    if crate::p2p::is_chat_open() && is_mouse_over_chat_hud(client_x, client_y, client_w, client_h, tb_h) {
        if is_down && button == 1 {
            let _ = handle_chat_click(client_x, client_y, client_w, client_h, tb_h);
        }
        return;
    }

    // 6. Whiteboard Mode
    if is_whiteboard_active() {
        if is_down && button == 3 {
            clear_strokes();
            return;
        }
        if is_down && button == 1 {
            let wb_hit = hit_test_whiteboard_palette(client_w, tb_h, client_x, client_y);
            if (9301..=9304).contains(&wb_hit) {
                WHITEBOARD_COLOR_IDX.store((wb_hit - 9301) as u8, Ordering::Relaxed);
                return;
            } else if wb_hit == 9399 {
                clear_strokes();
                return;
            }
            let sw = REMOTE_ORIG_W.load(Ordering::Relaxed).max(1);
            let sh = REMOTE_ORIG_H.load(Ordering::Relaxed).max(1);
            if let Some((sx, sy)) = map_client_to_screen(mx, my, win_w, win_h, sw, sh) {
                start_stroke(
                    (sx as f32 / sw as f32).clamp(0.0, 1.0),
                    (sy as f32 / sh as f32).clamp(0.0, 1.0),
                );
            }
        }
        return;
    }

    // 7. Admin View
    if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
        if is_down && button == 1 {
            let _ = handle_admin_click(mx, my, win_w, win_h);
        }
        return;
    }

    // 8. Remote Session Tab (Status View vs Active Video Stream)
    let status = crate::p2p::get_session_status();
    let latest = crate::p2p::get_latest_remote_frame();
    let is_err = matches!(status, crate::p2p::SessionStatus::Error(_));

    if is_err || latest.is_none() {
        if is_down && button == 1 {
            let _ = handle_status_view_click(mx, my, win_w, win_h);
        }
        return;
    }

    if let Some(frame) = latest {
        inject_remote_mouse_button(
            mx,
            my,
            win_w,
            win_h,
            frame.orig_width,
            frame.orig_height,
            button,
            is_down,
        );
    }
}

fn dispatch_mouse_motion(win_w: u16, win_h: u16, mx: i16, my: i16) {
    let client_x = mx as i32;
    let client_y = my as i32;
    let client_w = win_w as i32;
    let tb_h = TOOLBAR_HEIGHT as i32;

    if client_y < tb_h {
        let hit = get_tab_hit_state(client_w, client_x, client_y);
        let prev = TAB_HOVER_STATE.swap(hit, Ordering::Relaxed);
        if prev != hit {
            NEEDS_REDRAW.store(true, Ordering::Relaxed);
        }
        return;
    }

    let prev = TAB_HOVER_STATE.swap(-1, Ordering::Relaxed);
    if prev != -1 {
        NEEDS_REDRAW.store(true, Ordering::Relaxed);
    }

    if ACTIVE_TAB_INDEX.load(Ordering::Relaxed) == 0 || SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) {
        return;
    }

    if is_whiteboard_active() {
        let sw = REMOTE_ORIG_W.load(Ordering::Relaxed).max(1);
        let sh = REMOTE_ORIG_H.load(Ordering::Relaxed).max(1);
        if let Some((sx, sy)) = map_client_to_screen(mx, my, win_w, win_h, sw, sh) {
            add_stroke_point(
                (sx as f32 / sw as f32).clamp(0.0, 1.0),
                (sy as f32 / sh as f32).clamp(0.0, 1.0),
            );
            NEEDS_REDRAW.store(true, Ordering::Relaxed);
        }
        return;
    }

    if let Some(frame) = crate::p2p::get_latest_remote_frame() {
        inject_remote_mouse_move(mx, my, win_w, win_h, frame.orig_width, frame.orig_height);
    }
}

fn dispatch_key_event(keycode: u8, state: KeyButMask, is_down: bool) {
    // Tab key (23) toggles Settings Modal from anywhere, Escape (9) closes modals
    if is_down && SHOW_SETTINGS_MODAL.load(Ordering::Relaxed) && keycode == 9 {
        close_settings_modal();
        return;
    }

    let active_tab = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);

    if active_tab == 0 {
        if !is_down {
            return;
        }
        if crate::p2p::get_pending_approval().is_some() {
            if keycode == 36 || keycode == 29 {
                crate::p2p::respond_to_pending_approval(true);
            } else if keycode == 57 || keycode == 9 {
                crate::p2p::respond_to_pending_approval(false);
            }
            return;
        }

        if handle_remote_desk_edit_key(keycode, state) {
            let target = get_remote_id_raw();
            handle_homepage_connect(&target);
        }
        return;
    }

    // Active Remote Session Tab
    if is_down && crate::p2p::is_chat_open() {
        handle_chat_key(keycode, state);
        return;
    }

    if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
        if is_down {
            handle_admin_key(keycode, state);
        }
        return;
    }

    if is_down && keycode == 9 {
        if is_whiteboard_active() {
            set_whiteboard_active(false);
            return;
        } else if is_quick_actions_open() {
            set_quick_actions_open(false);
            return;
        } else if is_overflow_menu_open() {
            set_overflow_menu_open(false);
            return;
        }
    }

    let status = crate::p2p::get_session_status();
    if matches!(status, crate::p2p::SessionStatus::PasswordRequired(_)) {
        if is_down {
            handle_password_key_input(keycode, state);
        }
        return;
    }

    inject_remote_key(keycode, state, is_down);
}
