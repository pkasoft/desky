use anyhow::Result;
use crate::windows::WindowsPlatform;

/// Captures the local Windows desktop screen into a top-down BGRA pixel buffer.
/// Dynamically routes to the detected Windows platform family (Win 10/11, Server 2022, Server 2012, etc.)
/// ensuring 100% full-screen coverage and hardware-calibrated coordinates without edge clipping.
pub fn capture_screen_frame(
    capture_cursor: bool,
    scale_pct: u8,
) -> Result<(u32, u32, u32, u32, Vec<u8>)> {
    // 1. Hardware-accelerated GPU capture via DirectX 11 Desktop Duplication API (DXGI DDA)
    // Grabs frames directly from VRAM in < 1ms, eliminating CPU GDI BitBlt bottlenecks.
    if let Ok(Some(dxgi_frame)) = super::dxgi_capture::try_dxgi_capture(capture_cursor, scale_pct) {
        return Ok(dxgi_frame);
    }

    // 2. Seamless fallback to GDI BitBlt on legacy OS or during secure desktop / mode switches
    match super::get_platform() {
        WindowsPlatform::Win10_11 => super::win10_11::capture::capture_screen_frame(capture_cursor, scale_pct),
        WindowsPlatform::Server2022 => super::server_2022::capture::capture_screen_frame(capture_cursor, scale_pct),
        WindowsPlatform::Server2016_2019 => super::server_2016_2019::capture::capture_screen_frame(capture_cursor, scale_pct),
        WindowsPlatform::Server2012 => super::server_2012::capture::capture_screen_frame(capture_cursor, scale_pct),
        WindowsPlatform::Win7_8 => super::win7_8::capture::capture_screen_frame(capture_cursor, scale_pct),
    }
}
