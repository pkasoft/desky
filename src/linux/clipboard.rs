//! Linux Platform Clipboard Operations

#![allow(dead_code)]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;

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

/// Writes an array of file paths to the Linux clipboard using `text/uri-list`
pub fn write_clipboard_files(paths: &[PathBuf]) {
    if paths.is_empty() {
        return;
    }

    let mut uri_list = String::new();
    for p in paths {
        if let Ok(canon) = p.canonicalize() {
            uri_list.push_str(&format!("file://{}\r\n", canon.display()));
        } else {
            uri_list.push_str(&format!("file://{}\r\n", p.display()));
        }
    }

    // Try xclip first
    if let Ok(mut child) = Command::new("xclip")
        .args(["-selection", "clipboard", "-t", "text/uri-list"])
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(uri_list.as_bytes());
        }
        let _ = child.wait();
        return;
    }

    // Try wl-copy (Wayland)
    if let Ok(mut child) = Command::new("wl-copy")
        .args(["-t", "text/uri-list"])
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(uri_list.as_bytes());
        }
        let _ = child.wait();
    }
}

/// Writes Unicode text to the Linux clipboard
pub fn write_clipboard_text(text: &str) {
    mark_clipboard_synced(text);

    // Try xclip
    if let Ok(mut child) = Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
        return;
    }

    // Try wl-copy
    if let Ok(mut child) = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
        return;
    }

    // Try xsel
    if let Ok(mut child) = Command::new("xsel")
        .args(["--clipboard", "--input"])
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

/// Reads Unicode text from the Linux clipboard
pub fn read_clipboard_text() -> Option<String> {
    // Try xclip
    if let Ok(out) = Command::new("xclip")
        .args(["-selection", "clipboard", "-o"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }

    // Try wl-paste
    if let Ok(out) = Command::new("wl-paste").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }

    // Try xsel
    if let Ok(out) = Command::new("xsel")
        .args(["--clipboard", "--output"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }

    None
}

/// Reads dropped/copied file paths from the Linux clipboard
pub fn read_clipboard_files() -> Vec<PathBuf> {
    let mut uris = Vec::new();

    // Try xclip text/uri-list
    if let Ok(out) = Command::new("xclip")
        .args(["-selection", "clipboard", "-t", "text/uri-list", "-o"])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if let Some(path_str) = trimmed.strip_prefix("file://") {
                    let decoded_path = urlencoding_decode(path_str);
                    let p = PathBuf::from(decoded_path);
                    if p.exists() {
                        uris.push(p);
                    }
                }
            }
            if !uris.is_empty() {
                return uris;
            }
        }
    }

    // Try wl-paste text/uri-list
    if let Ok(out) = Command::new("wl-paste")
        .args(["-t", "text/uri-list"])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if let Some(path_str) = trimmed.strip_prefix("file://") {
                    let decoded_path = urlencoding_decode(path_str);
                    let p = PathBuf::from(decoded_path);
                    if p.exists() {
                        uris.push(p);
                    }
                }
            }
        }
    }

    uris
}

fn urlencoding_decode(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex_val) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                result.push(hex_val);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).to_string()
}

