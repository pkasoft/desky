//! DirectX 11 & DXGI Desktop Duplication Screen Capture Engine
//!
//! Captures the Windows desktop directly from GPU VRAM in < 1ms per frame,
//! bypassing CPU GDI BitBlt bottlenecks for true 60–120 FPS high-refresh streaming.

#![allow(unsafe_op_in_unsafe_fn)]

use anyhow::{anyhow, Result};
use std::sync::Mutex;
use std::time::Instant;
use tracing::{debug, info, warn};

use windows::core::Interface;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
    D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Dxgi::{
    IDXGIAdapter, IDXGIDevice, IDXGIOutput, IDXGIOutput1, IDXGIOutputDuplication,
    DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DrawIconEx, GetCursorInfo, GetIconInfo, CURSORINFO, CURSOR_SHOWING, DI_NORMAL, HCURSOR, HICON,
    ICONINFO,
};

pub struct DxgiCapturer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    staging_texture: Option<ID3D11Texture2D>,
    width: u32,
    height: u32,
    screen_x: i32,
    screen_y: i32,
    cached_frame: Vec<u8>,
    last_capture_time: Instant,
    last_gdi_refresh: Instant,
    pub monitor_index: u32,
    has_acquired_gpu_frame: bool,
}

static DXGI_ENGINE: Mutex<Option<DxgiCapturer>> = Mutex::new(None);
static TARGET_MONITOR_INDEX: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static ACTIVE_MONITOR_OFFSET_X: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
static ACTIVE_MONITOR_OFFSET_Y: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

pub fn get_active_monitor_offset_x() -> i32 {
    ACTIVE_MONITOR_OFFSET_X.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn get_active_monitor_offset_y() -> i32 {
    ACTIVE_MONITOR_OFFSET_Y.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn count_available_monitors() -> u32 {
    unsafe {
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        let mut feature_level: D3D_FEATURE_LEVEL = D3D_FEATURE_LEVEL_11_0;
        let feature_levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0];
        if D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            Some(&feature_levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            Some(&mut feature_level),
            Some(&mut context),
        ).is_err() {
            return 1;
        }
        if let Some(device) = device {
            if let Ok(dxgi_device) = device.cast::<IDXGIDevice>() {
                if let Ok(adapter) = dxgi_device.GetAdapter() {
                    let mut count = 0;
                    while adapter.EnumOutputs(count).is_ok() {
                        count += 1;
                    }
                    return count.max(1);
                }
            }
        }
        1
    }
}

pub fn set_target_monitor(index: u32) {
    TARGET_MONITOR_INDEX.store(index, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut guard) = DXGI_ENGINE.lock() {
        *guard = None; // Reset so next capture frame initializes the selected monitor!
    }
}

pub fn get_target_monitor() -> u32 {
    TARGET_MONITOR_INDEX.load(std::sync::atomic::Ordering::Relaxed)
}

impl DxgiCapturer {
    pub fn new(monitor_index: u32) -> Result<Self> {
        unsafe {
            let mut device: Option<ID3D11Device> = None;
            let mut context: Option<ID3D11DeviceContext> = None;
            let mut feature_level: D3D_FEATURE_LEVEL = D3D_FEATURE_LEVEL_11_0;

            let feature_levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0];

            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_FLAG(0),
                Some(&feature_levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                Some(&mut feature_level),
                Some(&mut context),
            )?;

            let device = device.ok_or_else(|| anyhow!("Failed to create D3D11 device"))?;
            let context = context.ok_or_else(|| anyhow!("Failed to create D3D11 context"))?;

            let dxgi_device: IDXGIDevice = device.cast()?;
            let adapter: IDXGIAdapter = dxgi_device.GetAdapter()?;
            let output: IDXGIOutput = match adapter.EnumOutputs(monitor_index) {
                Ok(o) => o,
                Err(_) => adapter.EnumOutputs(0)?,
            };
            let output1: IDXGIOutput1 = output.cast()?;

            let duplication: IDXGIOutputDuplication = output1.DuplicateOutput(&device)?;

            let desc = output.GetDesc()?;
            let screen_x = desc.DesktopCoordinates.left;
            let screen_y = desc.DesktopCoordinates.top;
            let width = (desc.DesktopCoordinates.right - desc.DesktopCoordinates.left).abs() as u32;
            let height = (desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top).abs() as u32;

            ACTIVE_MONITOR_OFFSET_X.store(screen_x, std::sync::atomic::Ordering::Relaxed);
            ACTIVE_MONITOR_OFFSET_Y.store(screen_y, std::sync::atomic::Ordering::Relaxed);

            info!("DXGI GPU Desktop Duplication initialized: Output #{} ({}x{}) @ Feature Level {:?}", monitor_index, width, height, feature_level);

            let mut cached_frame = vec![0u8; (width * height * 4) as usize];
            seed_gdi_frame(&mut cached_frame, width, height, screen_x, screen_y);

            Ok(Self {
                device,
                context,
                duplication,
                staging_texture: None,
                width,
                height,
                screen_x,
                screen_y,
                cached_frame,
                last_capture_time: Instant::now(),
                last_gdi_refresh: Instant::now(),
                monitor_index,
                has_acquired_gpu_frame: false,
            })
        }
    }

    fn ensure_staging_texture(&mut self) -> Result<&ID3D11Texture2D> {
        if self.staging_texture.is_none() {
            unsafe {
                let desc = D3D11_TEXTURE2D_DESC {
                    Width: self.width,
                    Height: self.height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_STAGING,
                    BindFlags: 0,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    MiscFlags: 0,
                };

                let mut staging: Option<ID3D11Texture2D> = None;
                self.device.CreateTexture2D(&desc, None, Some(&mut staging))?;
                self.staging_texture = staging;
            }
        }
        self.staging_texture
            .as_ref()
            .ok_or_else(|| anyhow!("Staging texture allocation failed"))
    }

    pub fn capture_frame(&mut self) -> Result<(u32, u32, &[u8])> {
        unsafe {
            let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource = None;

            // 12ms acquisition timeout to keep 60-120 FPS cadence smooth
            let acquire_res = self.duplication.AcquireNextFrame(12, &mut frame_info, &mut resource);

            match acquire_res {
                Ok(_) => {
                    let desktop_resource = resource.ok_or_else(|| anyhow!("No desktop resource in acquired frame"))?;
                    let desktop_texture: ID3D11Texture2D = desktop_resource.cast()?;

                    self.ensure_staging_texture()?;
                    let staging = self.staging_texture.as_ref().unwrap();

                    self.context.CopyResource(staging, &desktop_texture);

                    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                    if self.context.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).is_ok() {
                        let src_ptr = mapped.pData as *const u8;
                        let row_pitch = mapped.RowPitch as usize;
                        let row_bytes = (self.width * 4) as usize;

                        if row_pitch == row_bytes {
                            // Direct full-buffer copy in <0.3ms
                            let total_bytes = row_bytes * self.height as usize;
                            std::ptr::copy_nonoverlapping(src_ptr, self.cached_frame.as_mut_ptr(), total_bytes);
                        } else {
                            // Scanline-by-scanline copy respecting GPU pitch alignment
                            for y in 0..self.height as usize {
                                let src_row = src_ptr.add(y * row_pitch);
                                let dst_row = self.cached_frame.as_mut_ptr().add(y * row_bytes);
                                std::ptr::copy_nonoverlapping(src_row, dst_row, row_bytes);
                            }
                        }

                        self.context.Unmap(staging, 0);
                        self.has_acquired_gpu_frame = true;
                    }

                    // Release frame ONLY after GPU copy and Map/Unmap are complete!
                    let _ = self.duplication.ReleaseFrame();

                    self.last_capture_time = Instant::now();
                    Ok((self.width, self.height, &self.cached_frame))
                }
                Err(err) => {
                    if err.code() == DXGI_ERROR_WAIT_TIMEOUT {
                        // If no GPU frame has arrived yet or screen has been static for >100ms, refresh via GDI
                        if !self.has_acquired_gpu_frame || self.last_gdi_refresh.elapsed() >= std::time::Duration::from_millis(100) {
                            seed_gdi_frame(&mut self.cached_frame, self.width, self.height, self.screen_x, self.screen_y);
                            self.last_gdi_refresh = Instant::now();
                        }
                        Ok((self.width, self.height, &self.cached_frame))
                    } else if err.code() == DXGI_ERROR_ACCESS_LOST {
                        warn!("DXGI Desktop Duplication access lost (mode switch/UAC/Lock screen). Resetting capturer...");
                        super::desktop::sync_thread_desktop_to_input();
                        Err(anyhow!("DXGI_ERROR_ACCESS_LOST"))
                    } else {
                        Err(anyhow!("DXGI acquire error: {:?}", err))
                    }
                }
            }
        }
    }
}

struct CachedCursorPixels {
    hcursor: isize,
    hot_x: i32,
    hot_y: i32,
    pixels: [u8; 32 * 32 * 4],
}

static CURSOR_PIXEL_CACHE: Mutex<Option<CachedCursorPixels>> = Mutex::new(None);

unsafe fn extract_cursor_pixels(
    hcursor: HCURSOR,
    cur_handle: isize,
    cache: &mut Option<CachedCursorPixels>,
) -> (i32, i32, [u8; 32 * 32 * 4]) {
    let mut icon_info = ICONINFO::default();
    let (hot_x, hot_y) = if GetIconInfo(HICON(hcursor.0), &mut icon_info).is_ok() {
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

    let mut cursor_pixels = [0u8; 32 * 32 * 4];
    let hdc_screen = GetDC(None);
    if !hdc_screen.0.is_null() {
        let mem_dc = CreateCompatibleDC(Some(hdc_screen));
        let mem_bmp = CreateCompatibleBitmap(hdc_screen, 32, 32);
        let old = SelectObject(mem_dc, HGDIOBJ(mem_bmp.0));

        let _ = DrawIconEx(mem_dc, 0, 0, hcursor.into(), 32, 32, 0, None, DI_NORMAL);

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: 32,
                biHeight: -32,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        GetDIBits(
            mem_dc,
            mem_bmp,
            0,
            32,
            Some(cursor_pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        let _ = SelectObject(mem_dc, old);
        let _ = DeleteObject(HGDIOBJ(mem_bmp.0));
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, hdc_screen);
    }

    *cache = Some(CachedCursorPixels {
        hcursor: cur_handle,
        hot_x,
        hot_y,
        pixels: cursor_pixels,
    });

    (hot_x, hot_y, cursor_pixels)
}

/// Blends the hardware mouse cursor onto the raw BGRA buffer
fn blend_cursor_if_needed(bgra: &mut [u8], width: u32, height: u32) {
    unsafe {
        let mut ci = CURSORINFO {
            cbSize: std::mem::size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        if GetCursorInfo(&mut ci).is_ok()
            && (ci.flags.0 & CURSOR_SHOWING.0 != 0)
            && !ci.hCursor.is_invalid()
        {
            let cur_handle = ci.hCursor.0 as isize;
            let mut cached_opt = CURSOR_PIXEL_CACHE.lock().unwrap();
            let (hot_x, hot_y, cursor_pixels) = if let Some(ref c) = *cached_opt {
                if c.hcursor == cur_handle {
                    (c.hot_x, c.hot_y, c.pixels)
                } else {
                    extract_cursor_pixels(ci.hCursor, cur_handle, &mut *cached_opt)
                }
            } else {
                extract_cursor_pixels(ci.hCursor, cur_handle, &mut *cached_opt)
            };

            let cx = ci.ptScreenPos.x - hot_x;
            let cy = ci.ptScreenPos.y - hot_y;

            for py in 0..32 {
                let ty = cy + py;
                if ty < 0 || ty >= height as i32 {
                    continue;
                }
                for px in 0..32 {
                    let tx = cx + px;
                    if tx < 0 || tx >= width as i32 {
                        continue;
                    }
                    let c_idx = ((py * 32 + px) * 4) as usize;
                    let alpha = cursor_pixels[c_idx + 3] as u32;
                    if alpha > 0 {
                        let dst_idx = ((ty as u32 * width + tx as u32) * 4) as usize;
                        let inv = 255 - alpha;
                        bgra[dst_idx] = ((cursor_pixels[c_idx] as u32 * alpha + bgra[dst_idx] as u32 * inv) / 255) as u8;
                        bgra[dst_idx + 1] = ((cursor_pixels[c_idx + 1] as u32 * alpha + bgra[dst_idx + 1] as u32 * inv) / 255) as u8;
                        bgra[dst_idx + 2] = ((cursor_pixels[c_idx + 2] as u32 * alpha + bgra[dst_idx + 2] as u32 * inv) / 255) as u8;
                    }
                }
            }
        }
    }
}

/// Seeds the buffer with an instant GDI capture of the specified screen region so frames never start black
unsafe fn seed_gdi_frame(buf: &mut [u8], width: u32, height: u32, left: i32, top: i32) {
    let hdc_screen = GetDC(None);
    if hdc_screen.0.is_null() {
        return;
    }
    let mem_dc = CreateCompatibleDC(Some(hdc_screen));
    let mem_bmp = CreateCompatibleBitmap(hdc_screen, width as i32, height as i32);
    let old = SelectObject(mem_dc, HGDIOBJ(mem_bmp.0));

    let _ = BitBlt(mem_dc, 0, 0, width as i32, height as i32, Some(hdc_screen), left, top, SRCCOPY);

    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let _ = GetDIBits(
        mem_dc,
        mem_bmp,
        0,
        height,
        Some(buf.as_mut_ptr() as *mut _),
        &mut bmi,
        DIB_RGB_COLORS,
    );

    let _ = SelectObject(mem_dc, old);
    let _ = DeleteObject(HGDIOBJ(mem_bmp.0));
    let _ = DeleteDC(mem_dc);
    let _ = ReleaseDC(None, hdc_screen);
}

/// Detects if a GPU captured frame is entirely unpopulated or black (e.g. driver issue/Optimus)
fn is_frame_mostly_black(slice: &[u8], width: u32, height: u32) -> bool {
    if slice.len() < 16 {
        return true;
    }
    let step_x = (width / 8).max(1) as usize;
    let step_y = (height / 8).max(1) as usize;
    let mut non_zero = 0;
    for y in 1..8 {
        let row_idx = y * step_y * width as usize * 4;
        for x in 1..8 {
            let px_idx = row_idx + x * step_x * 4;
            if px_idx + 3 <= slice.len() {
                let b = slice[px_idx];
                let g = slice[px_idx + 1];
                let r = slice[px_idx + 2];
                if b > 8 || g > 8 || r > 8 {
                    non_zero += 1;
                }
            }
        }
    }
    non_zero == 0
}

/// Captures a desktop frame using hardware-accelerated DXGI Desktop Duplication.
/// Returns Ok(Some((orig_w, orig_h, cap_w, cap_h, bgra_bytes))) on success,
/// or Ok(None) to fall back to GDI BitBlt if hardware capture is unavailable.
pub fn try_dxgi_capture(
    capture_cursor: bool,
    scale_pct: u8,
) -> Result<Option<(u32, u32, u32, u32, Vec<u8>)>> {
    let mut guard = match DXGI_ENGINE.lock() {
        Ok(g) => g,
        Err(_) => return Ok(None),
    };

    let target_mon = get_target_monitor();
    if let Some(capturer) = guard.as_ref() {
        if capturer.monitor_index != target_mon {
            *guard = None;
        }
    }

    if guard.is_none() {
        match DxgiCapturer::new(target_mon) {
            Ok(capturer) => *guard = Some(capturer),
            Err(e) => {
                debug!("DXGI GPU capture unavailable (fallback to GDI): {}", e);
                return Ok(None);
            }
        }
    }

    let capturer = guard.as_mut().unwrap();
    match capturer.capture_frame() {
        Ok((orig_w, orig_h, bgra_slice)) => {
            if is_frame_mostly_black(bgra_slice, orig_w, orig_h) {
                // If DXGI returned black (e.g. Optimus/hybrid GPU or unpopulated surface), fallback to GDI immediately
                return Ok(None);
            }

            let mut frame_bytes = bgra_slice.to_vec();
            if capture_cursor {
                blend_cursor_if_needed(&mut frame_bytes, orig_w, orig_h);
            }

            let clamped_scale = (scale_pct as i32).clamp(50, 100);
            if clamped_scale >= 100 {
                Ok(Some((orig_w, orig_h, orig_w, orig_h, frame_bytes)))
            } else {
                let cap_w = (((orig_w as i32 * clamped_scale) / 100).max(2) & !1) as u32;
                let cap_h = (((orig_h as i32 * clamped_scale) / 100).max(2) & !1) as u32;
                let scaled = scale_bgra_nearest(&frame_bytes, orig_w, orig_h, cap_w, cap_h);
                Ok(Some((orig_w, orig_h, cap_w, cap_h, scaled)))
            }
        }
        Err(_e) => {
            *guard = None; // Reset capturer on ANY DXGI error to self-heal on next frame
            Ok(None)
        }
    }
}

/// Fast software downscaler when client requests reduced resolution
fn scale_bgra_nearest(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let dst_len = (dst_w * dst_h * 4) as usize;
    let mut dst = Vec::with_capacity(dst_len);
    unsafe {
        dst.set_len(dst_len);
    }
    for dy in 0..dst_h {
        let sy = (dy * src_h) / dst_h;
        let src_row = (sy * src_w * 4) as usize;
        let dst_row = (dy * dst_w * 4) as usize;
        for dx in 0..dst_w {
            let sx = (dx * src_w) / dst_w;
            let src_idx = src_row + (sx * 4) as usize;
            let dst_idx = dst_row + (dx * 4) as usize;
            dst[dst_idx..dst_idx + 4].copy_from_slice(&src[src_idx..src_idx + 4]);
        }
    }
    dst
}
