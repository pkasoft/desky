use anyhow::Result;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, SetBrushOrgEx, SetStretchBltMode, StretchBlt, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HALFTONE, HGDIOBJ, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DrawIconEx, GetCursorInfo, GetIconInfo, GetSystemMetrics, CURSORINFO, CURSOR_SHOWING,
    DI_NORMAL, HICON, ICONINFO, SM_CXSCREEN, SM_CYSCREEN,
};

/// Captures the local Windows desktop screen into a top-down BGRA pixel buffer.
/// `scale_pct` (50..=100) controls capture resolution relative to native desktop size.
/// Returns `(orig_width, orig_height, capture_width, capture_height, bgra_bytes)`.
pub fn capture_screen_frame(
    capture_cursor: bool,
    scale_pct: u8,
) -> Result<(u32, u32, u32, u32, Vec<u8>)> {
    unsafe {
        let orig_w = GetSystemMetrics(SM_CXSCREEN).max(1);
        let orig_h = GetSystemMetrics(SM_CYSCREEN).max(1);

        let clamped_scale = (scale_pct as i32).clamp(50, 100);
        let (cap_w, cap_h) = if clamped_scale >= 100 {
            (orig_w, orig_h)
        } else {
            let cw = ((orig_w * clamped_scale) / 100).max(2) & !1;
            let ch = ((orig_h * clamped_scale) / 100).max(2) & !1;
            (cw, ch)
        };

        let hdc_screen = GetDC(None);
        if hdc_screen.0.is_null() {
            anyhow::bail!("Failed to acquire screen DC");
        }

        let mem_dc = CreateCompatibleDC(Some(hdc_screen));
        let mem_bmp = CreateCompatibleBitmap(hdc_screen, cap_w, cap_h);
        let old_obj = SelectObject(mem_dc, HGDIOBJ(mem_bmp.0));

        if cap_w == orig_w && cap_h == orig_h {
            let _ = BitBlt(mem_dc, 0, 0, cap_w, cap_h, Some(hdc_screen), 0, 0, SRCCOPY);
        } else {
            // HALFTONE interpolation preserves crisp text anti-aliasing when downscaling
            let _ = SetStretchBltMode(mem_dc, HALFTONE);
            let _ = SetBrushOrgEx(mem_dc, 0, 0, None);
            let _ = StretchBlt(
                mem_dc,
                0,
                0,
                cap_w,
                cap_h,
                Some(hdc_screen),
                0,
                0,
                orig_w,
                orig_h,
                SRCCOPY,
            );
        }

        if capture_cursor {
            let mut ci = CURSORINFO {
                cbSize: std::mem::size_of::<CURSORINFO>() as u32,
                ..Default::default()
            };
            if GetCursorInfo(&mut ci).is_ok()
                && (ci.flags.0 & CURSOR_SHOWING.0 != 0)
                && !ci.hCursor.is_invalid()
            {
                let mut icon_info = ICONINFO::default();
                let (hot_x, hot_y) = if GetIconInfo(HICON(ci.hCursor.0), &mut icon_info).is_ok() {
                    if !icon_info.hbmMask.0.is_null() {
                        let _ = DeleteObject(HGDIOBJ(icon_info.hbmMask.0));
                    }
                    if !icon_info.hbmColor.0.is_null() {
                        let _ = DeleteObject(HGDIOBJ(icon_info.hbmColor.0));
                    }
                    (icon_info.xHotspot as i32, icon_info.yHotspot as i32)
                } else {
                    (0, 0)
                };

                let cx = ((ci.ptScreenPos.x - hot_x) * cap_w) / orig_w;
                let cy = ((ci.ptScreenPos.y - hot_y) * cap_h) / orig_h;
                let _ = DrawIconEx(
                    mem_dc,
                    cx,
                    cy,
                    ci.hCursor.into(),
                    0,
                    0,
                    0,
                    None,
                    DI_NORMAL,
                );
            }
        }

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: cap_w,
                biHeight: -cap_h, // Top-down DIB
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bgra = vec![0u8; (cap_w as usize) * (cap_h as usize) * 4];
        let lines = GetDIBits(
            mem_dc,
            mem_bmp,
            0,
            cap_h as u32,
            Some(bgra.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        let _ = SelectObject(mem_dc, old_obj);
        let _ = DeleteObject(HGDIOBJ(mem_bmp.0));
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, hdc_screen);

        if lines == 0 {
            anyhow::bail!("GetDIBits returned 0 scanlines");
        }

        Ok((orig_w as u32, orig_h as u32, cap_w as u32, cap_h as u32, bgra))
    }
}
