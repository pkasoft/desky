//! Dedicated Native X11 Window Procedure & Lifecycle Engine
//!
//! Mirrors `src/windows/ui/viewport/window.rs` by hosting the unified X11 window,
//! rendering into a `PixelCanvas` with anti-aliased TrueType fonts, and routing
//! mouse/keyboard events to tabs, homepage cards, or remote peer input injection.

use anyhow::Result;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tracing::info;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

use super::input_injector::{
    inject_remote_key, inject_remote_mouse_button, inject_remote_mouse_move,
};
use super::renderer::render_viewport_frame;
use super::session_status_view::{handle_password_key_input, handle_status_view_click};
use super::tabs::{init_session_tabs, ACTIVE_TAB_INDEX};
use super::toolbar::{handle_toolbar_click, TOOLBAR_HEIGHT};
use crate::linux::ui::launcher::{
    handle_homepage_click, handle_homepage_connect, BRING_TO_FRONT, NEEDS_REDRAW, RUNNING,
};
use crate::linux::ui::remote_desk_edit::{get_remote_id_raw, handle_remote_desk_edit_key};
use crate::linux::ui::theme::{PixelCanvas, PK_BG};

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

    // Register WM_DELETE_WINDOW for clean window close handling
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
            let _ = conn.flush();
        }

        // Process all pending X11 events
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
    // 1. Top Toolbar / Tab Bar
    if my < TOOLBAR_HEIGHT as i16 {
        if is_down && button == 1 {
            let _ = handle_toolbar_click(mx, my, win_w);
        }
        return;
    }

    let active_tab = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);

    // 2. Homepage View
    if active_tab == 0 {
        if is_down && button == 1 {
            let _ = handle_homepage_click(mx, my, win_w, win_h);
        }
        return;
    }

    // 3. Remote Session Tab (Status View vs Active Video Stream)
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
    if ACTIVE_TAB_INDEX.load(Ordering::Relaxed) == 0 || my < TOOLBAR_HEIGHT as i16 {
        return;
    }
    if let Some(frame) = crate::p2p::get_latest_remote_frame() {
        inject_remote_mouse_move(mx, my, win_w, win_h, frame.orig_width, frame.orig_height);
    }
}

fn dispatch_key_event(keycode: u8, state: KeyButMask, is_down: bool) {
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
    let status = crate::p2p::get_session_status();
    if matches!(status, crate::p2p::SessionStatus::PasswordRequired(_)) {
        if is_down {
            handle_password_key_input(keycode, state);
        }
        return;
    }

    inject_remote_key(keycode, state, is_down);
}
