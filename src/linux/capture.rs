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
