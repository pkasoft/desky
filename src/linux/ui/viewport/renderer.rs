//! Dedicated Double-Buffered Remote Viewport Canvas Renderer (Linux X11)
//!
//! Mirrors `src/windows/ui/viewport/renderer.rs` 1:1 by compositing the top tab bar,
//! fullscreen settings modal, homepage cards, admin view, session status alerts,
//! scaled BGRA video frames, whiteboard annotations, chat HUD, file transfer HUD,
//! quick actions popup, and overflow menu onto a `PixelCanvas`.

use std::sync::atomic::{AtomicU32, Ordering};

use super::chat_hud::draw_chat_hud;
use super::file_hud::draw_file_transfer_hud;
use super::overflow_menu::draw_overflow_menu;
use super::quick_actions::draw_quick_actions_popup;
use super::session_status_view::draw_session_status_view;
use super::settings_modal::{draw_fullscreen_settings_view, DISPLAY_MODE, SHOW_SETTINGS_MODAL};
use super::tabs::{
    get_session_tabs, ACTIVE_TAB_INDEX, ADMIN_HOVER_STATE, VIEWPORT_IS_ADMIN_MODE,
};
use super::toolbar::{draw_tab_bar, get_more_x, get_zap_x, TAB_HOVER_STATE, TOOLBAR_HEIGHT};
use super::whiteboard::{draw_whiteboard_overlay, draw_whiteboard_palette};
use crate::linux::ui::admin::draw_admin_view;
use crate::linux::ui::launcher::draw_homepage_view;
use crate::linux::ui::theme::{PixelCanvas, PK_BG, PK_DARK_VIEWPORT};

pub static REMOTE_ORIG_W: AtomicU32 = AtomicU32::new(0);
pub static REMOTE_ORIG_H: AtomicU32 = AtomicU32::new(0);

pub fn render_viewport_frame(
    canvas: &mut PixelCanvas,
    win_w: u16,
    win_h: u16,
    remote_frame: Option<&crate::p2p::state::RemoteFrame>,
) {
    let client_w = win_w as i32;
    let client_h = win_h as i32;
    let tb_h = TOOLBAR_HEIGHT as i32;

    canvas.clear(PK_BG);

    let tabs = get_session_tabs();
    let active_tab = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
    let modal_open = SHOW_SETTINGS_MODAL.load(Ordering::Relaxed);
    let hover_state = TAB_HOVER_STATE.load(Ordering::Relaxed);

    if let Some(f) = remote_frame {
        REMOTE_ORIG_W.store(f.orig_width, Ordering::Relaxed);
        REMOTE_ORIG_H.store(f.orig_height, Ordering::Relaxed);
    }
    let orig_w = REMOTE_ORIG_W.load(Ordering::Relaxed);
    let orig_h = REMOTE_ORIG_H.load(Ordering::Relaxed);

    if modal_open {
        draw_fullscreen_settings_view(canvas, client_w, client_h, tb_h, orig_w, orig_h);
        draw_tab_bar(canvas, win_w, &tabs, active_tab);
        return;
    }

    if active_tab == 0 {
        draw_homepage_view(canvas, win_w, win_h);
        draw_tab_bar(canvas, win_w, &tabs, 0);
        return;
    }

    // Active Remote Session Tab
    let target_id = tabs
        .get(active_tab)
        .map(|t| t.title.clone())
        .unwrap_or_else(|| "Remote".to_string());

    if VIEWPORT_IS_ADMIN_MODE.load(Ordering::Relaxed) {
        draw_admin_view(
            canvas,
            win_w,
            win_h,
            ADMIN_HOVER_STATE.load(Ordering::Relaxed),
        );
    } else {
        let status = crate::p2p::get_session_status();
        let is_err = matches!(status, crate::p2p::SessionStatus::Error(_));

        if !is_err {
            if let Some(frame) = remote_frame {
                render_video_stream(canvas, win_w, win_h, frame);
            } else {
                draw_session_status_view(canvas, win_w, win_h, &target_id, &status);
            }
        } else {
            draw_session_status_view(canvas, win_w, win_h, &target_id, &status);
        }
    }

    // Overlays on top of remote stream
    draw_file_transfer_hud(canvas, client_w, client_h);
    draw_chat_hud(canvas, client_w, client_h, tb_h);
    draw_whiteboard_palette(canvas, client_w, tb_h, hover_state);

    let zap_x = get_zap_x(client_w, tabs.len(), active_tab);
    draw_quick_actions_popup(canvas, zap_x, 32, tb_h, hover_state);

    if let Some(more_x) = get_more_x(client_w, tabs.len(), active_tab) {
        draw_overflow_menu(canvas, more_x, 32, tb_h, hover_state);
    }

    // Draw top toolbar last so its tooltips render over the viewport
    draw_tab_bar(canvas, win_w, &tabs, active_tab);
}

fn render_video_stream(
    canvas: &mut PixelCanvas,
    win_w: u16,
    win_h: u16,
    frame: &crate::p2p::state::RemoteFrame,
) {
    let view_y = TOOLBAR_HEIGHT as i32;
    let view_w = win_w as i32;
    let view_h = (win_h.saturating_sub(TOOLBAR_HEIGHT)) as i32;

    if view_w <= 16 || view_h <= 16 || frame.width == 0 || frame.height == 0 {
        return;
    }

    canvas.fill_rect(0, view_y, view_w, view_h, PK_DARK_VIEWPORT);

    let mode = DISPLAY_MODE.load(Ordering::Relaxed);
    let fw = frame.width as i32;
    let fh = frame.height as i32;

    let (dst_w, dst_h, off_x, off_y) = match mode {
        0 => {
            let scale_x = view_w as f32 / fw as f32;
            let scale_y = view_h as f32 / fh as f32;
            let scale = scale_x.min(scale_y);
            let rw = ((fw as f32 * scale) as i32).max(2);
            let rh = ((fh as f32 * scale) as i32).max(2);
            let ox = (view_w - rw) / 2;
            let oy = view_y + (view_h - rh) / 2;
            (rw, rh, ox, oy)
        }
        1 => (view_w, view_h, 0, view_y),
        _ => {
            let ox = (view_w - fw) / 2;
            let oy = view_y + (view_h - fh) / 2;
            (fw, fh, ox, oy)
        }
    };

    let src_w = frame.width as usize;
    let src_h = frame.height as usize;
    let canvas_w = canvas.width as usize;
    let canvas_h = canvas.height as usize;

    if dst_w as usize == src_w && dst_h as usize == src_h {
        // Fast 1:1 exact pixel copy
        for dy in 0..dst_h {
            let dst_y = off_y + dy;
            if dst_y < view_y || dst_y >= canvas_h as i32 {
                continue;
            }
            let src_row = (dy as usize) * src_w * 4;
            let dst_row = (dst_y as usize) * canvas_w * 4;
            for dx in 0..dst_w {
                let dst_x = off_x + dx;
                if dst_x < 0 || dst_x >= canvas_w as i32 {
                    continue;
                }
                let s_idx = src_row + (dx as usize) * 4;
                let d_idx = dst_row + (dst_x as usize) * 4;
                if s_idx + 4 <= frame.bgra.len() && d_idx + 4 <= canvas.data.len() {
                    canvas.data[d_idx..d_idx + 4].copy_from_slice(&frame.bgra[s_idx..s_idx + 4]);
                }
            }
        }
    } else {
        // Bilinear interpolation for smooth, crisp scaled remote desktop text
        let x_ratio = ((src_w.saturating_sub(1)) << 8) / (dst_w.max(1) as usize);
        let y_ratio = ((src_h.saturating_sub(1)) << 8) / (dst_h.max(1) as usize);
        let src_stride = src_w * 4;

        for dy in 0..dst_h {
            let dst_y = off_y + dy;
            if dst_y < view_y || dst_y >= canvas_h as i32 {
                continue;
            }
            let sy_fp = (dy as usize) * y_ratio;
            let sy = sy_fp >> 8;
            let fy = (sy_fp & 0xFF) as u32;
            let inv_fy = 256 - fy;
            let sy1 = (sy + 1).min(src_h.saturating_sub(1));

            let row0 = sy * src_stride;
            let row1 = sy1 * src_stride;
            let dst_row = (dst_y as usize) * canvas_w * 4;

            for dx in 0..dst_w {
                let dst_x = off_x + dx;
                if dst_x < 0 || dst_x >= canvas_w as i32 {
                    continue;
                }
                let sx_fp = (dx as usize) * x_ratio;
                let sx = sx_fp >> 8;
                let fx = (sx_fp & 0xFF) as u32;
                let inv_fx = 256 - fx;
                let sx1 = (sx + 1).min(src_w.saturating_sub(1));

                let w00 = inv_fx * inv_fy;
                let w10 = fx * inv_fy;
                let w01 = inv_fx * fy;
                let w11 = fx * fy;

                let i00 = row0 + sx * 4;
                let i10 = row0 + sx1 * 4;
                let i01 = row1 + sx * 4;
                let i11 = row1 + sx1 * 4;
                let d_idx = dst_row + (dst_x as usize) * 4;

                if i11 + 3 < frame.bgra.len() && d_idx + 3 < canvas.data.len() {
                    for c in 0..3 {
                        let v = (frame.bgra[i00 + c] as u32 * w00
                            + frame.bgra[i10 + c] as u32 * w10
                            + frame.bgra[i01 + c] as u32 * w01
                            + frame.bgra[i11 + c] as u32 * w11)
                            >> 16;
                        canvas.data[d_idx + c] = v as u8;
                    }
                    canvas.data[d_idx + 3] = 255;
                }
            }
        }
    }

    draw_whiteboard_overlay(canvas, off_x, off_y, dst_w, dst_h);
}
