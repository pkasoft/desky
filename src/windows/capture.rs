use anyhow::Result;

/// Captures the local Windows desktop screen into a top-down BGRA pixel buffer.
/// Routes through the dedicated Windows Server & High-DPI engine (`server2022`)
/// to guarantee 100% full-screen coverage without clipping on Windows Server / RDP / Nexus VPS.
pub fn capture_screen_frame(
    capture_cursor: bool,
    scale_pct: u8,
) -> Result<(u32, u32, u32, u32, Vec<u8>)> {
    crate::windows::server2022::capture_server_screen_frame(capture_cursor, scale_pct)
}
