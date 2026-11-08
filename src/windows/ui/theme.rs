//! pkasoft.com White Theme Styling & Win32 GDI Graphics Helpers

#![allow(unsafe_op_in_unsafe_fn)]

use windows::core::w;
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, Rectangle, SelectObject, FONT_CHARSET,
    FONT_CLIP_PRECISION, FONT_OUTPUT_PRECISION, FONT_QUALITY, FONT_WEIGHT, HBRUSH, HDC, HFONT,
    HGDIOBJ, PS_SOLID,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GHND};

// pkasoft.com White Theme color palette (GDI COLORREF little-endian: 0x00BBGGRR)
pub const PK_BG: COLORREF = COLORREF(0x00F5F7F5);          // #f5f7f5 (--paper soft clean canvas)
pub const PK_CARD: COLORREF = COLORREF(0x00FFFFFF);        // #ffffff (pure crisp white card)
pub const PK_CARD_BORDER: COLORREF = COLORREF(0x00E3E7E2); // #e2e7e3 (--line border)
pub const PK_GREEN: COLORREF = COLORREF(0x004F6A2D);       // #2d6a4f (--green signature emerald)
pub const PK_GREEN_DARK: COLORREF = COLORREF(0x0032431B);  // #1b4332 (--green-dark pine hover)
pub const PK_GREEN_TINT: COLORREF = COLORREF(0x00EEF5F0);  // #f0f5ee (subtle green pill fill)
pub const PK_TEXT: COLORREF = COLORREF(0x0020241D);        // #1d2420 (--ink deep crisp charcoal)
pub const PK_MUTED: COLORREF = COLORREF(0x00636A5D);       // #5d6a63 (--muted slate sage)
pub const PK_INPUT_BG: COLORREF = COLORREF(0x00FAFCFA);    // #fafcfa (soft clean input background)
pub const PK_BTN_HOVER: COLORREF = COLORREF(0x00E5EDE7);   // #e7ede5 (hover outline button)
pub const PK_RED: COLORREF = COLORREF(0x003030C5);         // #c53030 (crimson decline button)

pub unsafe fn create_segoe_font(height: i32, weight: FONT_WEIGHT) -> HFONT {
    let safe_height = height.max(12);
    unsafe {
        CreateFontW(
            -safe_height,
            0,
            0,
            0,
            weight.0 as i32,
            0,
            0,
            0,
            FONT_CHARSET(1), // DEFAULT_CHARSET
            FONT_OUTPUT_PRECISION(0),
            FONT_CLIP_PRECISION(0),
            FONT_QUALITY(6), // CLEARTYPE_NATURAL_QUALITY
            0,
            w!("Segoe UI"),
        )
    }
}

pub unsafe fn create_mono_font(height: i32, weight: FONT_WEIGHT) -> HFONT {
    let safe_height = height.max(12);
    unsafe {
        CreateFontW(
            -safe_height,
            0,
            0,
            0,
            weight.0 as i32,
            0,
            0,
            0,
            FONT_CHARSET(1),
            FONT_OUTPUT_PRECISION(0),
            FONT_CLIP_PRECISION(0),
            FONT_QUALITY(6), // CLEARTYPE_NATURAL_QUALITY
            0,
            w!("Consolas"),
        )
    }
}

pub unsafe fn draw_rounded_box(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    fill_color: COLORREF,
    border_color: COLORREF,
    _radius: i32,
) {
    unsafe {
        let brush: HBRUSH = CreateSolidBrush(fill_color);
        let pen = CreatePen(PS_SOLID, 1, border_color);
        let old_brush = SelectObject(hdc, HGDIOBJ(brush.0));
        let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));

        let _ = Rectangle(hdc, x, y, x + w, y + h);

        let _ = SelectObject(hdc, old_brush);
        let _ = SelectObject(hdc, old_pen);
        let _ = DeleteObject(HGDIOBJ(brush.0));
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

pub fn copy_to_clipboard(text: &str) {
    unsafe {
        if OpenClipboard(None).is_ok() {
            let _ = EmptyClipboard();
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes_len = wide.len() * 2;

            if let Ok(handle) = GlobalAlloc(GHND, bytes_len) {
                let ptr = GlobalLock(handle);
                if !ptr.is_null() {
                    std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr as *mut u8, bytes_len);
                    let _ = GlobalUnlock(handle);
                    let _ = SetClipboardData(13, Some(windows::Win32::Foundation::HANDLE(handle.0)));
                }
            }
            let _ = CloseClipboard();
        }
    }
}

pub fn format_bitrate(kbps: u32) -> String {
    if kbps >= 1000 {
        format!("{:.1} Mbps", kbps as f32 / 1000.0)
    } else if kbps > 0 {
        format!("{} kbps", kbps)
    } else {
        "Measuring...".to_string()
    }
}
