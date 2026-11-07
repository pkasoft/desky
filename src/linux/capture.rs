use anyhow::Result;
use std::sync::{Mutex, OnceLock};
use tracing::info;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{get_image, ImageFormat};
use x11rb::rust_connection::RustConnection;

static X11_CAP_CONN: OnceLock<Mutex<Option<(RustConnection, usize)>>> = OnceLock::new();

fn get_or_init_x11() -> Result<std::sync::MutexGuard<'static, Option<(RustConnection, usize)>>> {
    let lock = X11_CAP_CONN.get_or_init(|| Mutex::new(None));
    let mut guard = lock.lock().map_err(|_| anyhow::anyhow!("X11 mutex poisoned"))?;
    if guard.is_none() {
        let (conn, screen_num) = x11rb::connect(None)?;
        *guard = Some((conn, screen_num));
    }
    Ok(guard)
}

/// Probes and logs display information from the local X11 server
pub fn diagnose_x11() -> Result<()> {
    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    info!(
        "X11 Display Connected: Screen [{}] ({}x{}, Depth: {} bpp, Root Window: 0x{:x})",
        screen_num, screen.width_in_pixels, screen.height_in_pixels, screen.root_depth, screen.root
    );
    Ok(())
}

pub struct LinuxX11Capture;

impl LinuxX11Capture {
    pub fn new() -> Self {
        Self
    }

    /// Captures the full desktop frame directly from the X11 root window using a persistent connection
    pub fn capture_frame() -> Result<(u32, u32, Vec<u8>)> {
        let mut guard = get_or_init_x11()?;
        let res = if let Some((conn, screen_num)) = guard.as_ref() {
            let screen = &conn.setup().roots[*screen_num];
            let w = screen.width_in_pixels;
            let h = screen.height_in_pixels;
            get_image(conn, ImageFormat::Z_PIXMAP, screen.root, 0, 0, w, h, !0)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .map(|reply| (w as u32, h as u32, reply.data))
        } else {
            None
        };

        if let Some(frame) = res {
            return Ok(frame);
        }

        // Reconnect once if X11 connection was reset
        *guard = None;
        let (conn, screen_num) = x11rb::connect(None)?;
        let screen = &conn.setup().roots[screen_num];
        let w = screen.width_in_pixels;
        let h = screen.height_in_pixels;
        let reply = get_image(&conn, ImageFormat::Z_PIXMAP, screen.root, 0, 0, w, h, !0)?.reply()?;
        let data = reply.data;
        *guard = Some((conn, screen_num));
        Ok((w as u32, h as u32, data))
    }
}

/// Captures the local Linux X11 desktop into a top-down BGRA pixel buffer.
/// `scale_pct` (50..=100) controls capture resolution relative to native desktop size.
/// Returns `(orig_width, orig_height, capture_width, capture_height, bgra_bytes)`.
pub fn capture_screen_frame(
    _capture_cursor: bool,
    scale_pct: u8,
) -> Result<(u32, u32, u32, u32, Vec<u8>)> {
    let (orig_w, orig_h, data) = LinuxX11Capture::capture_frame()?;
    if orig_w == 0 || orig_h == 0 {
        anyhow::bail!("X11 returned empty screen dimensions");
    }

    let expected_len = (orig_w as usize) * (orig_h as usize) * 4;
    if data.len() < expected_len {
        anyhow::bail!("X11 frame buffer shorter than 32-bit BGRA");
    }

    let clamped_scale = (scale_pct as u32).clamp(50, 100);
    if clamped_scale >= 100 {
        return Ok((orig_w, orig_h, orig_w, orig_h, data));
    }

    let cap_w = ((orig_w * clamped_scale) / 100).max(2) & !1;
    let cap_h = ((orig_h * clamped_scale) / 100).max(2) & !1;
    let mut scaled = vec![0u8; (cap_w as usize) * (cap_h as usize) * 4];

    for y in 0..cap_h {
        let src_yf = (y as f32) * (orig_h as f32 - 1.0) / (cap_h as f32 - 1.0).max(1.0);
        let y0 = (src_yf as u32).min(orig_h - 1) as usize;
        let y1 = (y0 + 1).min((orig_h - 1) as usize);
        let wy = ((src_yf - y0 as f32) * 256.0) as u32;
        let iwy = 256 - wy;

        let row0 = y0 * (orig_w as usize) * 4;
        let row1 = y1 * (orig_w as usize) * 4;
        let dst_row = (y as usize) * (cap_w as usize) * 4;

        for x in 0..cap_w {
            let src_xf = (x as f32) * (orig_w as f32 - 1.0) / (cap_w as f32 - 1.0).max(1.0);
            let x0 = (src_xf as u32).min(orig_w - 1) as usize;
            let x1 = (x0 + 1).min((orig_w - 1) as usize);
            let wx = ((src_xf - x0 as f32) * 256.0) as u32;
            let iwx = 256 - wx;

            let i00 = row0 + x0 * 4;
            let i10 = row0 + x1 * 4;
            let i01 = row1 + x0 * 4;
            let i11 = row1 + x1 * 4;
            let d_idx = dst_row + (x as usize) * 4;

            for c in 0..3 {
                let top = data[i00 + c] as u32 * iwx + data[i10 + c] as u32 * wx;
                let bot = data[i01 + c] as u32 * iwx + data[i11 + c] as u32 * wx;
                scaled[d_idx + c] = ((top * iwy + bot * wy) >> 16) as u8;
            }
            scaled[d_idx + 3] = 255;
        }
    }

    Ok((orig_w, orig_h, cap_w, cap_h, scaled))
}
