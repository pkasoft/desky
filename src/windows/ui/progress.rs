//! Dedicated File Transfer Progress Window (Win32)

#![allow(unsafe_op_in_unsafe_fn)]

use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteObject,
    DrawTextW, EndPaint, FillRect, InvalidateRect, SelectObject, SetBkMode, SetTextColor,
    DT_CENTER, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FW_BOLD, FW_NORMAL, HGDIOBJ, PAINTSTRUCT,
    SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, IsWindow, LoadCursorW, PostMessageW,
    RegisterClassExW, SetForegroundWindow, SetTimer, CW_USEDEFAULT, IDC_ARROW, WM_CLOSE,
    WM_DESTROY, WM_PAINT, WM_TIMER, WNDCLASSEXW, WS_VISIBLE,
};

use super::launcher::{HWND_MAIN_WINDOW, WM_APP_SHOW_PROGRESS};
use super::theme::{
    create_segoe_font, draw_rounded_box, PK_BG, PK_CARD, PK_CARD_BORDER, PK_GREEN, PK_GREEN_TINT,
    PK_INPUT_BG, PK_MUTED, PK_TEXT,
};

static HWND_PROGRESS: AtomicIsize = AtomicIsize::new(0);
static PROGRESS_CLOSE_TICKS: AtomicU32 = AtomicU32::new(0);

pub fn trigger_transfer_progress_window() {
    unsafe {
        let main_hwnd = HWND_MAIN_WINDOW;
        if !main_hwnd.0.is_null() && IsWindow(Some(main_hwnd)).as_bool() {
            let _ = PostMessageW(Some(main_hwnd), WM_APP_SHOW_PROGRESS, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn spawn_progress_window() {
    let existing = HWND_PROGRESS.load(Ordering::Relaxed);
    if existing != 0 {
        let hwnd = HWND(existing as *mut _);
        if unsafe { IsWindow(Some(hwnd)).as_bool() } {
            let _ = unsafe { SetForegroundWindow(hwnd) };
            return;
        }
    }

    unsafe {
        let h_module = GetModuleHandleW(None).unwrap_or_default();
        let h_instance: HINSTANCE = h_module.into();
        let class_name = w!("ReskyProgressClass");

        let mut wc = WNDCLASSEXW::default();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(progress_wndproc);
        wc.hInstance = h_instance;
        wc.lpszClassName = class_name;
        wc.hbrBackground = CreateSolidBrush(PK_BG);
        wc.hCursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        let _ = RegisterClassExW(&wc);

        PROGRESS_CLOSE_TICKS.store(0, Ordering::Relaxed);

        let pw = 420;
        let ph = 180;
        let hwnd = CreateWindowExW(
            windows::Win32::UI::WindowsAndMessaging::WS_EX_TOPMOST,
            class_name,
            w!("Resky | File Transfer"),
            windows::Win32::UI::WindowsAndMessaging::WS_POPUP
                | windows::Win32::UI::WindowsAndMessaging::WS_CAPTION
                | windows::Win32::UI::WindowsAndMessaging::WS_SYSMENU
                | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            pw,
            ph,
            None,
            None,
            Some(h_instance),
            None,
        );

        if let Ok(h) = hwnd {
            HWND_PROGRESS.store(h.0 as isize, Ordering::Relaxed);
            let _ = SetTimer(Some(h), 3001, 40, None);
        }
    }
}

pub(crate) unsafe extern "system" fn progress_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TIMER => {
            let prog = crate::p2p::get_transfer_progress();
            if prog.is_complete {
                let ticks = PROGRESS_CLOSE_TICKS.fetch_add(1, Ordering::Relaxed);
                if ticks > 50 {
                    // ~2 seconds after completion
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rc = RECT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rc);
            let w = rc.right - rc.left;
            let h = rc.bottom - rc.top;

            if w > 50 && h > 50 {
                let mem_dc = CreateCompatibleDC(Some(hdc));
                let mem_bmp = CreateCompatibleBitmap(hdc, w, h);
                let old_bmp = SelectObject(mem_dc, HGDIOBJ(mem_bmp.0));

                let bg_brush = CreateSolidBrush(PK_BG);
                FillRect(mem_dc, &rc, bg_brush);
                let _ = DeleteObject(HGDIOBJ(bg_brush.0));
                SetBkMode(mem_dc, TRANSPARENT);

                let prog = crate::p2p::get_transfer_progress();

                // Outer Card Box
                draw_rounded_box(mem_dc, 12, 12, w - 24, h - 24, PK_CARD, PK_CARD_BORDER, 8);

                // Top Badge (Sending vs Receiving)
                let (badge_txt, badge_bg, badge_fg) = if prog.is_complete {
                    ("COMPLETED", PK_GREEN_TINT, PK_GREEN)
                } else if prog.is_sending {
                    ("SENDING FILE", PK_GREEN_TINT, PK_GREEN)
                } else {
                    ("RECEIVING FILE", COLORREF(0x00FFF4EB), COLORREF(0x00DF772B))
                };

                let font_badge = create_segoe_font(11, FW_BOLD);
                let font_title = create_segoe_font(15, FW_BOLD);
                let font_info = create_segoe_font(12, FW_NORMAL);

                draw_rounded_box(mem_dc, 24, 22, 110, 20, badge_bg, PK_CARD_BORDER, 4);
                let _ = SelectObject(mem_dc, HGDIOBJ(font_badge.0));
                SetTextColor(mem_dc, badge_fg);
                let mut b_wide: Vec<u16> = badge_txt.encode_utf16().collect();
                let mut b_rc = RECT {
                    left: 24,
                    top: 22,
                    right: 134,
                    bottom: 42,
                };
                let _ = DrawTextW(
                    mem_dc,
                    &mut b_wide,
                    &mut b_rc,
                    DT_SINGLELINE | DT_CENTER | DT_VCENTER,
                );

                // File Name
                let _ = SelectObject(mem_dc, HGDIOBJ(font_title.0));
                SetTextColor(mem_dc, PK_TEXT);
                let display_name = if prog.file_name.is_empty() {
                    "Preparing transfer...".to_string()
                } else {
                    prog.file_name.clone()
                };
                let mut f_wide: Vec<u16> = display_name.encode_utf16().collect();
                let mut f_rc = RECT {
                    left: 24,
                    top: 46,
                    right: w - 24,
                    bottom: 68,
                };
                let _ = DrawTextW(mem_dc, &mut f_wide, &mut f_rc, DT_SINGLELINE | DT_VCENTER);

                // Progress Bar Container (width: w - 48, height: 16)
                let bar_x = 24;
                let bar_y = 74;
                let bar_w = w - 48;
                let bar_h = 16;
                draw_rounded_box(
                    mem_dc,
                    bar_x,
                    bar_y,
                    bar_w,
                    bar_h,
                    PK_INPUT_BG,
                    PK_CARD_BORDER,
                    4,
                );

                // Progress Fill
                let fill_pct = prog.percentage.min(100);
                let fill_w = if prog.is_complete {
                    bar_w
                } else {
                    ((bar_w as f32 * fill_pct as f32) / 100.0) as i32
                };

                if fill_w > 0 {
                    let fill_brush = CreateSolidBrush(PK_GREEN);
                    let fill_rc = RECT {
                        left: bar_x + 1,
                        top: bar_y + 1,
                        right: bar_x + fill_w - 1,
                        bottom: bar_y + bar_h - 1,
                    };
                    FillRect(mem_dc, &fill_rc, fill_brush);
                    let _ = DeleteObject(HGDIOBJ(fill_brush.0));
                }

                // Subtext Row
                let _ = SelectObject(mem_dc, HGDIOBJ(font_info.0));
                SetTextColor(mem_dc, PK_MUTED);

                let cur_mb = prog.current_bytes as f64 / 1_048_576.0;
                let tot_mb = prog.total_bytes as f64 / 1_048_576.0;
                let stats_str = format!("{}% | {:.1} MB / {:.1} MB", fill_pct, cur_mb, tot_mb);
                let mut s_wide: Vec<u16> = stats_str.encode_utf16().collect();
                let mut s_rc = RECT {
                    left: 24,
                    top: 96,
                    right: w / 2 + 60,
                    bottom: 118,
                };
                let _ = DrawTextW(mem_dc, &mut s_wide, &mut s_rc, DT_SINGLELINE | DT_VCENTER);

                let status_str = if prog.is_complete {
                    "Transfer Complete"
                } else if prog.is_sending {
                    "Uploading..."
                } else {
                    "Downloading..."
                };
                if prog.is_complete {
                    crate::ui::draw_check(mem_dc, w - 146, 100, 14, PK_GREEN, 2);
                }
                let mut st_wide: Vec<u16> = status_str.encode_utf16().collect();
                let mut st_rc = RECT {
                    left: w / 2,
                    top: 96,
                    right: w - 24,
                    bottom: 118,
                };
                let _ = DrawTextW(
                    mem_dc,
                    &mut st_wide,
                    &mut st_rc,
                    DT_SINGLELINE | DT_RIGHT | DT_VCENTER,
                );

                let _ = BitBlt(hdc, 0, 0, w, h, Some(mem_dc), 0, 0, SRCCOPY);
                let _ = SelectObject(mem_dc, old_bmp);
                let _ = DeleteObject(HGDIOBJ(mem_bmp.0));
                let _ = windows::Win32::Graphics::Gdi::DeleteDC(mem_dc);
                let _ = DeleteObject(HGDIOBJ(font_badge.0));
                let _ = DeleteObject(HGDIOBJ(font_title.0));
                let _ = DeleteObject(HGDIOBJ(font_info.0));
            }

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            HWND_PROGRESS.store(0, Ordering::Relaxed);
            LRESULT(0)
        }
        WM_DESTROY => {
            HWND_PROGRESS.store(0, Ordering::Relaxed);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}