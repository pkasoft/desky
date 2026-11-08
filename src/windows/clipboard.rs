//! Windows Platform Win32 Clipboard Operations

#![allow(unsafe_op_in_unsafe_fn)]

use std::path::PathBuf;
use std::sync::Mutex;
use windows::Win32::Foundation::POINT;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GHND};
use windows::Win32::UI::Shell::{DragQueryFileW, DROPFILES, HDROP};

const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;

static LAST_SYNCED_CLIPBOARD: Mutex<String> = Mutex::new(String::new());

pub fn mark_clipboard_synced(text: &str) {
    if let Ok(mut guard) = LAST_SYNCED_CLIPBOARD.lock() {
        *guard = text.to_string();
    }
}

pub fn is_clipboard_recently_synced(text: &str) -> bool {
    if let Ok(guard) = LAST_SYNCED_CLIPBOARD.lock() {
        if *guard == text {
            return true;
        }
    }
    false
}

/// Writes an array of file paths to the Windows clipboard in `CF_HDROP` format
pub fn write_clipboard_files(paths: &[PathBuf]) {
    if paths.is_empty() {
        return;
    }
    unsafe {
        if OpenClipboard(None).is_ok() {
            let _ = EmptyClipboard();

            let mut wide_bytes = Vec::new();
            for p in paths {
                let wide: Vec<u16> = p
                    .to_string_lossy()
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                for w in wide {
                    wide_bytes.extend_from_slice(&w.to_le_bytes());
                }
            }
            // Double null-terminated
            wide_bytes.extend_from_slice(&0u16.to_le_bytes());

            let dropfiles_size = std::mem::size_of::<DROPFILES>();
            let total_size = dropfiles_size + wide_bytes.len();

            if let Ok(handle) = GlobalAlloc(GHND, total_size) {
                let ptr = GlobalLock(handle);
                if !ptr.is_null() {
                    let df = DROPFILES {
                        pFiles: dropfiles_size as u32,
                        pt: POINT { x: 0, y: 0 },
                        fNC: false.into(),
                        fWide: true.into(),
                    };
                    std::ptr::copy_nonoverlapping(
                        &df as *const _ as *const u8,
                        ptr as *mut u8,
                        dropfiles_size,
                    );
                    std::ptr::copy_nonoverlapping(
                        wide_bytes.as_ptr(),
                        (ptr as *mut u8).add(dropfiles_size),
                        wide_bytes.len(),
                    );
                    let _ = GlobalUnlock(handle);
                    let _ = SetClipboardData(
                        CF_HDROP,
                        Some(windows::Win32::Foundation::HANDLE(handle.0)),
                    );
                }
            }
            let _ = CloseClipboard();
        }
    }
}

/// Writes Unicode text to the Windows clipboard in `CF_UNICODETEXT` format
pub fn write_clipboard_text(text: &str) {
    mark_clipboard_synced(text);
    unsafe {
        if OpenClipboard(None).is_ok() {
            let _ = EmptyClipboard();
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes_len = wide.len() * 2;
            if let Ok(handle) = GlobalAlloc(GHND, bytes_len) {
                let ptr = GlobalLock(handle);
                if !ptr.is_null() {
                    std::ptr::copy_nonoverlapping(
                        wide.as_ptr() as *const u8,
                        ptr as *mut u8,
                        bytes_len,
                    );
                    let _ = GlobalUnlock(handle);
                    let _ = SetClipboardData(
                        CF_UNICODETEXT,
                        Some(windows::Win32::Foundation::HANDLE(handle.0)),
                    );
                }
            }
            let _ = CloseClipboard();
        }
    }
}

/// Reads Unicode text from the Windows clipboard
pub fn read_clipboard_text() -> Option<String> {
    unsafe {
        if OpenClipboard(None).is_ok() {
            let res = if let Ok(h) = GetClipboardData(CF_UNICODETEXT) {
                if !h.0.is_null() {
                    let hglob = windows::Win32::Foundation::HGLOBAL(h.0);
                    let max_bytes = GlobalSize(hglob);
                    if max_bytes >= 2 {
                        let ptr = GlobalLock(hglob) as *const u16;
                        if !ptr.is_null() {
                            let max_len = (max_bytes / 2) as usize;
                            let mut len = 0;
                            while len < max_len && *ptr.add(len) != 0 {
                                len += 1;
                            }
                            let slice = std::slice::from_raw_parts(ptr, len);
                            let s = String::from_utf16_lossy(slice);
                            let _ = GlobalUnlock(hglob);
                            Some(s)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let _ = CloseClipboard();
            res
        } else {
            None
        }
    }
}

/// Reads dropped/copied file paths from the Windows clipboard via `CF_HDROP`
pub fn read_clipboard_files() -> Vec<PathBuf> {
    unsafe {
        let mut files = Vec::new();
        if IsClipboardFormatAvailable(CF_HDROP).is_ok() {
            if OpenClipboard(None).is_ok() {
                if let Ok(h) = GetClipboardData(CF_HDROP) {
                    if !h.0.is_null() {
                        let hdrop = HDROP(h.0);
                        let count = DragQueryFileW(hdrop, 0xFFFFFFFF, None);
                        for i in 0..count {
                            let mut buf = [0u16; 512];
                            let len = DragQueryFileW(hdrop, i, Some(&mut buf));
                            if len > 0 {
                                let path_str = String::from_utf16_lossy(&buf[..len as usize]);
                                files.push(PathBuf::from(path_str));
                            }
                        }
                    }
                }
                let _ = CloseClipboard();
            }
        }
        files
    }
}
