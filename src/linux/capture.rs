use anyhow::Result;
use std::sync::{Mutex, OnceLock};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{get_image, query_pointer, ImageFormat};
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

fn capture_x11_root_frame(capture_cursor: bool) -> Result<(u32, u32, Vec<u8>)> {
    let mut guard = get_or_init_x11()?;
    let res = if let Some((conn, screen_num)) = guard.as_ref() {
        let screen = &conn.setup().roots[*screen_num];
        let w = screen.width_in_pixels;
        let h = screen.height_in_pixels;
        let root = screen.root;
        let ptr_cookie = if capture_cursor {
            query_pointer(conn, root).ok()
        } else {
            None
        };
        let img = get_image(conn, ImageFormat::Z_PIXMAP, root, 0, 0, w, h, !0)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| (w as u32, h as u32, reply.data));
        let ptr_pos = ptr_cookie
            .and_then(|c| c.reply().ok())
            .map(|r| (r.root_x as i32, r.root_y as i32));
        img.map(|(fw, fh, mut data)| {
            if let Some((px, py)) = ptr_pos {
                draw_software_cursor(&mut data, fw as i32, fh as i32, px, py);
            }
            (fw, fh, data)
        })
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
    let mut data = reply.data;
    if capture_cursor {
        if let Ok(ptr) = query_pointer(&conn, screen.root)?.reply() {
            draw_software_cursor(&mut data, w as i32, h as i32, ptr.root_x as i32, ptr.root_y as i32);
        }
    }
    *guard = Some((conn, screen_num));
    Ok((w as u32, h as u32, data))
}

/// Renders a crisp 16x16 white arrow cursor with a dark outline onto a 32-bit BGRA frame buffer
fn draw_software_cursor(bgra: &mut [u8], width: i32, height: i32, cx: i32, cy: i32) {
    // 1 = black outline, 2 = white fill
    const CURSOR_BMP: [&[u8; 11]; 16] = [
        b"10000000000",
        b"11000000000",
        b"12100000000",
        b"12210000000",
        b"12221000000",
        b"12222100000",
        b"12222210000",
        b"12222221000",
        b"12222222100",
        b"12222222210",
        b"12222211111",
        b"12212210000",
        b"12101221000",
        b"11001221000",
        b"10000122100",
        b"00000011000",
    ];
    for (dy, row) in CURSOR_BMP.iter().enumerate() {
        let y = cy + dy as i32;
        if y < 0 || y >= height {
            continue;
        }
        let row_offset = (y as usize) * (width as usize) * 4;
        for (dx, &pix) in row.iter().enumerate() {
            if pix == b'0' {
                continue;
            }
            let x = cx + dx as i32;
            if x < 0 || x >= width {
                continue;
            }
            let idx = row_offset + (x as usize) * 4;
            if idx + 3 < bgra.len() {
                if pix == b'1' {
                    bgra[idx] = 20;
                    bgra[idx + 1] = 20;
                    bgra[idx + 2] = 20;
                    bgra[idx + 3] = 255;
                } else {
                    bgra[idx] = 255;
                    bgra[idx + 1] = 255;
                    bgra[idx + 2] = 255;
                    bgra[idx + 3] = 255;
                }
            }
        }
    }
}

/// Captures the local Linux X11 desktop into a top-down BGRA pixel buffer.
pub fn capture_screen_frame(
    capture_cursor: bool,
    scale_pct: u8,
) -> Result<(u32, u32, u32, u32, Vec<u8>)> {
    let (orig_w, orig_h, data) = capture_x11_root_frame(capture_cursor)?;
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

    // High-clarity bilinear downsampling (prevents text/UI line aliasing when scaled)
    let x_ratio = ((orig_w - 1) << 8) / cap_w.max(1);
    let y_ratio = ((orig_h - 1) << 8) / cap_h.max(1);
    let orig_stride = (orig_w as usize) * 4;
    let dst_stride = (cap_w as usize) * 4;

    for y in 0..cap_h {
        let sy_fp = y * y_ratio;
        let sy = (sy_fp >> 8) as usize;
        let fy = (sy_fp & 0xFF) as u32;
        let inv_fy = 256 - fy;
        let sy1 = (sy + 1).min(orig_h as usize - 1);

        let row0 = sy * orig_stride;
        let row1 = sy1 * orig_stride;
        let dst_row = (y as usize) * dst_stride;

        for x in 0..cap_w {
            let sx_fp = x * x_ratio;
            let sx = (sx_fp >> 8) as usize;
            let fx = (sx_fp & 0xFF) as u32;
            let inv_fx = 256 - fx;
            let sx1 = (sx + 1).min(orig_w as usize - 1);

            let w00 = inv_fx * inv_fy;
            let w10 = fx * inv_fy;
            let w01 = inv_fx * fy;
            let w11 = fx * fy;

            let i00 = row0 + sx * 4;
            let i10 = row0 + sx1 * 4;
            let i01 = row1 + sx * 4;
            let i11 = row1 + sx1 * 4;
            let d_idx = dst_row + (x as usize) * 4;

            for c in 0..3 {
                let v = (data[i00 + c] as u32 * w00
                    + data[i10 + c] as u32 * w10
                    + data[i01 + c] as u32 * w01
                    + data[i11 + c] as u32 * w11)
                    >> 16;
                scaled[d_idx + c] = v as u8;
            }
            scaled[d_idx + 3] = 255;
        }
    }

    Ok((orig_w, orig_h, cap_w, cap_h, scaled))
}
