//! Hardware Cursor Shape Detection & Zero-Lag Local Synchronization

use std::sync::OnceLock;
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorInfo, LoadCursorW, CURSORINFO, CURSOR_SHOWING, HCURSOR,
    IDC_APPSTARTING, IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_HELP, IDC_IBEAM, IDC_NO,
    IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, IDC_UPARROW, IDC_WAIT,
};

struct SystemCursors {
    ibeam: isize,
    wait: isize,
    cross: isize,
    uparrow: isize,
    sizenwse: isize,
    sizenesw: isize,
    sizewe: isize,
    sizens: isize,
    sizeall: isize,
    no: isize,
    hand: isize,
    appstarting: isize,
    help: isize,
}

static SYSTEM_CURSORS: OnceLock<SystemCursors> = OnceLock::new();

fn get_system_cursors() -> &'static SystemCursors {
    SYSTEM_CURSORS.get_or_init(|| unsafe {
        SystemCursors {
            ibeam: LoadCursorW(None, IDC_IBEAM).map(|h| h.0 as isize).unwrap_or(0),
            wait: LoadCursorW(None, IDC_WAIT).map(|h| h.0 as isize).unwrap_or(0),
            cross: LoadCursorW(None, IDC_CROSS).map(|h| h.0 as isize).unwrap_or(0),
            uparrow: LoadCursorW(None, IDC_UPARROW).map(|h| h.0 as isize).unwrap_or(0),
            sizenwse: LoadCursorW(None, IDC_SIZENWSE).map(|h| h.0 as isize).unwrap_or(0),
            sizenesw: LoadCursorW(None, IDC_SIZENESW).map(|h| h.0 as isize).unwrap_or(0),
            sizewe: LoadCursorW(None, IDC_SIZEWE).map(|h| h.0 as isize).unwrap_or(0),
            sizens: LoadCursorW(None, IDC_SIZENS).map(|h| h.0 as isize).unwrap_or(0),
            sizeall: LoadCursorW(None, IDC_SIZEALL).map(|h| h.0 as isize).unwrap_or(0),
            no: LoadCursorW(None, IDC_NO).map(|h| h.0 as isize).unwrap_or(0),
            hand: LoadCursorW(None, IDC_HAND).map(|h| h.0 as isize).unwrap_or(0),
            appstarting: LoadCursorW(None, IDC_APPSTARTING).map(|h| h.0 as isize).unwrap_or(0),
            help: LoadCursorW(None, IDC_HELP).map(|h| h.0 as isize).unwrap_or(0),
        }
    })
}

/// Detects the active host cursor shape and returns a normalized ID (0..13)
pub fn get_current_cursor_shape() -> u8 {
    unsafe {
        let mut ci = CURSORINFO {
            cbSize: std::mem::size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        if GetCursorInfo(&mut ci).is_ok() {
            if (ci.flags.0 & CURSOR_SHOWING.0) == 0 {
                return 0;
            }
            let h = ci.hCursor.0 as isize;
            let sc = get_system_cursors();
            if h == sc.ibeam {
                1
            } else if h == sc.wait {
                2
            } else if h == sc.cross {
                3
            } else if h == sc.hand {
                4
            } else if h == sc.sizeall {
                5
            } else if h == sc.sizens {
                6
            } else if h == sc.sizewe {
                7
            } else if h == sc.sizenwse {
                8
            } else if h == sc.sizenesw {
                9
            } else if h == sc.no {
                10
            } else if h == sc.appstarting {
                11
            } else if h == sc.help {
                12
            } else if h == sc.uparrow {
                13
            } else {
                0
            }
        } else {
            0
        }
    }
}

/// Translates a normalized cursor shape ID back to an HCURSOR for local rendering
pub fn get_hcursor_for_shape(shape: u8) -> HCURSOR {
    unsafe {
        let idc = match shape {
            1 => IDC_IBEAM,
            2 => IDC_WAIT,
            3 => IDC_CROSS,
            4 => IDC_HAND,
            5 => IDC_SIZEALL,
            6 => IDC_SIZENS,
            7 => IDC_SIZEWE,
            8 => IDC_SIZENWSE,
            9 => IDC_SIZENESW,
            10 => IDC_NO,
            11 => IDC_APPSTARTING,
            12 => IDC_HELP,
            13 => IDC_UPARROW,
            _ => IDC_ARROW,
        };
        LoadCursorW(None, idc).unwrap_or_default()
    }
}
