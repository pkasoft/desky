//! Dedicated Double-Buffered Remote Viewport Canvas Renderer (Linux X11)
//!
//! Mirrors `src/windows/ui/viewport/renderer.rs` by compositing the top tab bar,
//! homepage cards, session status alerts, and scaled BGRA video frames onto a
//! `PixelCanvas` before blitting to the X11 window.

use std::sync::atomic::Ordering;

use super::session_status_view::draw_session_status_view;
use super::tabs::{get_session_tabs, ACTIVE_TAB_INDEX};
use super::toolbar::{draw_tab_bar, TOOLBAR_HEIGHT};
use crate::linux::ui::launcher::draw_homepage_view;
use crate::linux::ui::theme::{PixelCanvas, PK_BG, PK_DARK_VIEWPORT};

pub fn render_viewport_frame(
    canvas: &mut PixelCanvas,
    win_w: u16,
    win_h: u16,
    remote_frame: Option<&crate::p2p::state::RemoteFrame>,
) {
    // 1. Clear full background
    canvas.clear(PK_BG);

    // 2. Top Navigation / Tab Bar
    let tabs = get_session_tabs();
    let active_tab = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
    draw_tab_bar(canvas, win_w, &tabs, active_tab);

    // 3. Main View Area (Home Tab vs Remote Session Tab)
    if active_tab == 0 {
        draw_homepage_view(canvas, win_w, win_h);
    } else {
        let target_id = tabs
            .get(active_tab)
            .map(|t| t.title.clone())
            .unwrap_or_else(|| "Remote".to_string());
        render_remote_stream_or_status(canvas, win_w, win_h, &target_id, remote_frame);
    }
}

fn render_remote_stream_or_status(
    canvas: &mut PixelCanvas,
    win_w: u16,
    win_h: u16,
    target_id: &str,
    remote_frame: Option<&crate::p2p::state::RemoteFrame>,
) {
    let status = crate::p2p::get_session_status();
    let is_err = matches!(status, crate::p2p::SessionStatus::Error(_));

    if !is_err {
        if let Some(frame) = remote_frame {
            let view_y = TOOLBAR_HEIGHT as i32;
            let view_w = win_w as usize;
            let view_h = win_h.saturating_sub(TOOLBAR_HEIGHT) as usize;

            if view_w > 16 && view_h > 16 && frame.width > 0 && frame.height > 0 {
                canvas.fill_rect(0, view_y, view_w as i32, view_h as i32, PK_DARK_VIEWPORT);

                // Scale BGRA frame to fit viewport preserving aspect ratio
                let scale_x = view_w as f32 / frame.width as f32;
                let scale_y = view_h as f32 / frame.height as f32;
                let scale = scale_x.min(scale_y);

                let dst_w = ((frame.width as f32 * scale) as usize).max(2) & !1;
                let dst_h = ((frame.height as f32 * scale) as usize).max(2) & !1;
                let off_x = (view_w.saturating_sub(dst_w)) / 2;
                let off_y = (view_y as usize) + (view_h.saturating_sub(dst_h)) / 2;

                let src_w = frame.width as usize;
                let src_h = frame.height as usize;
                let canvas_w = canvas.width as usize;

                for dy in 0..dst_h {
                    let sy = (dy * src_h / dst_h).min(src_h - 1);
                    let src_row = sy * src_w * 4;
                    let dst_y = off_y + dy;
                    if dst_y >= canvas.height as usize {
                        break;
                    }
                    let dst_row = dst_y * canvas_w * 4;
                    for dx in 0..dst_w {
                        let sx = (dx * src_w / dst_w).min(src_w - 1);
                        let dst_x = off_x + dx;
                        if dst_x >= canvas_w {
                            break;
                        }
                        let s_idx = src_row + sx * 4;
                        let d_idx = dst_row + dst_x * 4;
                        if s_idx + 4 <= frame.bgra.len() && d_idx + 4 <= canvas.data.len() {
                            canvas.data[d_idx..d_idx + 4]
                                .copy_from_slice(&frame.bgra[s_idx..s_idx + 4]);
                        }
                    }
                }
                return;
            }
        }
    }

    draw_session_status_view(canvas, win_w, win_h, target_id, &status);
}
