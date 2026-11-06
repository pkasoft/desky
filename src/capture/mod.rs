/// Display capture module (DirectX DXGI Desktop Duplication on Windows)
pub struct DisplayInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

pub struct CaptureEngine {
    pub current_display: DisplayInfo,
}

impl CaptureEngine {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            current_display: DisplayInfo {
                name: "Primary Display".to_string(),
                width: 1920,
                height: 1080,
                is_primary: true,
            },
        })
    }
}
