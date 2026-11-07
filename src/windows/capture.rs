#![allow(unsafe_op_in_unsafe_fn)]
use anyhow::Result;
use tracing::info;
use windows::core::Interface;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput1,
};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, SetStretchBltMode, StretchBlt, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    COLORONCOLOR, DIB_RGB_COLORS, HGDIOBJ, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DrawIconEx, GetCursorInfo, GetIconInfo, GetSystemMetrics, CURSORINFO, CURSOR_SHOWING,
    DI_NORMAL, HICON, ICONINFO, SM_CXSCREEN, SM_CYSCREEN,
};

pub fn diagnose_dxgi() -> Result<()> {
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut adapter_index = 0;

        while let Ok(adapter) = factory.EnumAdapters1(adapter_index) {
            let desc = adapter.GetDesc1()?;
            let gpu_name = String::from_utf16_lossy(
                &desc.Description[..desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len())]
            );
            info!("Adapter [{}]: {} (Dedicated VRAM: {} MB, Flags: {:?})", 
                adapter_index, gpu_name, desc.DedicatedVideoMemory / (1024 * 1024), desc.Flags);

            let mut output_index = 0;
            while let Ok(output) = adapter.EnumOutputs(output_index) {
                let out_desc = output.GetDesc()?;
                let out_name = String::from_utf16_lossy(
                    &out_desc.DeviceName[..out_desc.DeviceName.iter().position(|&c| c == 0).unwrap_or(out_desc.DeviceName.len())]
                );
                let w = (out_desc.DesktopCoordinates.right - out_desc.DesktopCoordinates.left).unsigned_abs();
                let h = (out_desc.DesktopCoordinates.bottom - out_desc.DesktopCoordinates.top).unsigned_abs();
                info!("  -> Output [{}]: {} ({}x{}, AttachedToDesktop: {:?})", 
                    output_index, out_name, w, h, out_desc.AttachedToDesktop);

                let mut device: Option<ID3D11Device> = None;
                let mut context: Option<ID3D11DeviceContext> = None;
                let mut feature_level = Default::default();
                let hr = D3D11CreateDevice(
                    &adapter,
                    D3D_DRIVER_TYPE_UNKNOWN,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    Some(&mut feature_level),
                    Some(&mut context),
                );

                if let Ok(()) = hr {
                    let output1: Result<IDXGIOutput1, _> = output.cast();
                    if let Ok(output1) = output1 {
                        let dev = device.unwrap();
                        match output1.DuplicateOutput(&dev) {
                            Ok(dupl) => {
                                info!("     [SUCCESS] DuplicateOutput acquired successfully!");
                                drop(dupl);
                            }
                            Err(e) => {
                                info!("     [FAIL] DuplicateOutput returned: {} ({:#x})", e.message(), e.code().0);
                            }
                        }
                    }
                }

                output_index += 1;
            }

            adapter_index += 1;
        }
    }
    Ok(())
}

/// Captures the local Windows desktop screen into a top-down BGRA pixel buffer.
/// Returns `(orig_width, orig_height, capture_width, capture_height, bgra_bytes)`.
pub fn capture_screen_frame(capture_cursor: bool) -> Result<(u32, u32, u32, u32, Vec<u8>)> {
    unsafe {
        let orig_w = GetSystemMetrics(SM_CXSCREEN).max(1);
        let orig_h = GetSystemMetrics(SM_CYSCREEN).max(1);

        let (cap_w, cap_h) = if orig_w > 1280 {
            let cw = 1280;
            let ch = ((orig_h * 1280) / orig_w).max(2) & !1;
            (cw, ch)
        } else {
            (orig_w, orig_h)
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
            let _ = SetStretchBltMode(mem_dc, COLORONCOLOR);
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
