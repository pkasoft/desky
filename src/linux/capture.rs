use anyhow::Result;
use tracing::info;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{get_image, ImageFormat};

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

    /// Captures the full desktop frame directly from the X11 root window
    pub fn capture_frame() -> Result<(u32, u32, Vec<u8>)> {
        let (conn, screen_num) = x11rb::connect(None)?;
        let screen = &conn.setup().roots[screen_num];
        let w = screen.width_in_pixels;
        let h = screen.height_in_pixels;

        let reply = get_image(
            &conn,
            ImageFormat::Z_PIXMAP,
            screen.root,
            0,
            0,
            w,
            h,
            !0,
        )?
        .reply()?;

        Ok((w as u32, h as u32, reply.data))
    }
}

/// Captures the local Linux X11 desktop into a top-down BGRA pixel buffer.
/// Returns `(orig_width, orig_height, capture_width, capture_height, bgra_bytes)`.
pub fn capture_screen_frame(_capture_cursor: bool) -> Result<(u32, u32, u32, u32, Vec<u8>)> {
    let (orig_w, orig_h, data) = LinuxX11Capture::capture_frame()?;
    if orig_w == 0 || orig_h == 0 {
        anyhow::bail!("X11 returned empty screen dimensions");
    }

    let expected_len = (orig_w as usize) * (orig_h as usize) * 4;
    if data.len() < expected_len {
        anyhow::bail!("X11 frame buffer shorter than 32-bit BGRA");
    }

    if orig_w <= 1280 {
        return Ok((orig_w, orig_h, orig_w, orig_h, data));
    }

    let cap_w = 1280u32;
    let cap_h = ((orig_h * 1280) / orig_w).max(2) & !1;
    let mut scaled = vec![0u8; (cap_w as usize) * (cap_h as usize) * 4];

    for y in 0..cap_h {
        let src_y = ((y * orig_h) / cap_h).min(orig_h - 1) as usize;
        let src_row = src_y * (orig_w as usize) * 4;
        let dst_row = (y as usize) * (cap_w as usize) * 4;
        for x in 0..cap_w {
            let src_x = ((x * orig_w) / cap_w).min(orig_w - 1) as usize;
            let s_idx = src_row + src_x * 4;
            let d_idx = dst_row + (x as usize) * 4;
            scaled[d_idx..d_idx + 4].copy_from_slice(&data[s_idx..s_idx + 4]);
        }
    }

    Ok((orig_w, orig_h, cap_w, cap_h, scaled))
}
