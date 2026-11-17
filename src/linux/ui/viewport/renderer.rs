//! Dedicated Double-Buffered Remote Viewport Canvas Renderer (Linux X11)
//!
//! Mirrors `src/windows/ui/viewport/renderer.rs` by compositing the top tab bar,
//! homepage cards, session status alerts, and scaled BGRA video frames onto an
//! offscreen X11 Pixmap before copying to the window.

use std::sync::atomic::Ordering;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

use super::session_status_view::draw_session_status_view;
use super::tabs::{get_session_tabs, ACTIVE_TAB_INDEX};
use super::toolbar::{draw_tab_bar, TOOLBAR_HEIGHT};
use crate::linux::ui::launcher::draw_homepage_view;
use crate::linux::ui::theme::X11Palette;

#[allow(clippy::too_many_arguments)]
pub fn render_viewport_frame<C: Connection>(
    conn: &C,
    canvas: Drawable,
    depth: u8,
    win_w: u16,
    win_h: u16,
    pal: &X11Palette,
    remote_frame: Option<&crate::p2p::state::RemoteFrame>,
    scaled_buf: &mut Vec<u8>,
) {
    // 1. Clear full background
    let _ = conn.poly_fill_rectangle(
        canvas,
        pal.gc_bg,
        &[Rectangle {
            x: 0,
            y: 0,
            width: win_w,
            height: win_h,
        }],
    );

    // 2. Top Navigation / Tab Bar
    let tabs = get_session_tabs();
    let active_tab = ACTIVE_TAB_INDEX.load(Ordering::Relaxed);
    draw_tab_bar(conn, canvas, win_w, &tabs, active_tab, pal);

    // 3. Main View Area (Home Tab vs Remote Session Tab)
    if active_tab == 0 {
        draw_homepage_view(conn, canvas, win_w, win_h, pal);
    } else {
        let target_id = tabs
            .get(active_tab)
            .map(|t| t.title.clone())
            .unwrap_or_else(|| "Remote".to_string());
        render_remote_stream_or_status(
            conn,
            canvas,
            depth,
            win_w,
            win_h,
            pal,
            &target_id,
            remote_frame,
            scaled_buf,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_remote_stream_or_status<C: Connection>(
    conn: &C,
    canvas: Drawable,
    depth: u8,
    win_w: u16,
    win_h: u16,
    pal: &X11Palette,
    target_id: &str,
    remote_frame: Option<&crate::p2p::state::RemoteFrame>,
    scaled_buf: &mut Vec<u8>,
) {
    let status = crate::p2p::get_session_status();
    let is_err = matches!(status, crate::p2p::SessionStatus::Error(_));

    if !is_err {
        if let Some(frame) = remote_frame {
            let view_y = TOOLBAR_HEIGHT as i16;
            let view_w = win_w as usize;
            let view_h = win_h.saturating_sub(TOOLBAR_HEIGHT) as usize;

            if view_w > 16 && view_h > 16 && frame.width > 0 && frame.height > 0 {
                let _ = conn.poly_fill_rectangle(
                    canvas,
                    pal.gc_dark_vp,
                    &[Rectangle {
                        x: 0,
                        y: view_y,
                        width: win_w,
                        height: win_h.saturating_sub(TOOLBAR_HEIGHT),
                    }],
                );

                // Scale BGRA frame to fit viewport preserving aspect ratio
                let scale_x = view_w as f32 / frame.width as f32;
                let scale_y = view_h as f32 / frame.height as f32;
                let scale = scale_x.min(scale_y);

                let dst_w = ((frame.width as f32 * scale) as usize).max(2) & !1;
                let dst_h = ((frame.height as f32 * scale) as usize).max(2) & !1;
                let off_x = ((view_w.saturating_sub(dst_w)) / 2) as i16;
                let off_y = view_y + ((view_h.saturating_sub(dst_h)) / 2) as i16;

                let needed = dst_w * dst_h * 4;
                if scaled_buf.len() != needed {
                    scaled_buf.resize(needed, 0);
                }

                let src_w = frame.width as usize;
                let src_h = frame.height as usize;
                for dy in 0..dst_h {
                    let sy = (dy * src_h / dst_h).min(src_h - 1);
                    let src_row = sy * src_w * 4;
                    let dst_row = dy * dst_w * 4;
                    for dx in 0..dst_w {
                        let sx = (dx * src_w / dst_w).min(src_w - 1);
                        let s_idx = src_row + sx * 4;
                        let d_idx = dst_row + dx * 4;
                        if s_idx + 4 <= frame.bgra.len() {
                            scaled_buf[d_idx..d_idx + 4]
                                .copy_from_slice(&frame.bgra[s_idx..s_idx + 4]);
                        }
                    }
                }

                // Send rows in chunks so we never exceed X11 max request size
                let max_rows_per_chunk = (65536 / (dst_w * 4)).max(1);
                let mut row_start = 0;
                while row_start < dst_h {
                    let chunk_h = (dst_h - row_start).min(max_rows_per_chunk);
                    let byte_start = row_start * dst_w * 4;
                    let byte_end = (row_start + chunk_h) * dst_w * 4;
                    let _ = conn.put_image(
                        ImageFormat::Z_PIXMAP,
                        canvas,
                        pal.gc_card,
                        dst_w as u16,
                        chunk_h as u16,
                        off_x,
                        off_y + row_start as i16,
                        0,
                        depth,
                        &scaled_buf[byte_start..byte_end],
                    );
                    row_start += chunk_h;
                }
                return;
            }
        }
    }

    draw_session_status_view(conn, canvas, win_w, win_h, target_id, &status, pal);
}
