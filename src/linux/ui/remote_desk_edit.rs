//! Remote Desk 9-Digit ID Input Buffer & Auto-Formatting (Linux X11)
//!
//! Mirrors `src/windows/ui/remote_desk_edit.rs` by managing the Remote Desk
//! input field state, 3-3-3 digit formatting (`800 258 756`), and keyboard/paste handling.

#![allow(dead_code)]

use std::sync::{Mutex, OnceLock};
use x11rb::protocol::xproto::KeyButMask;

static REMOTE_ID_INPUT: OnceLock<Mutex<String>> = OnceLock::new();
pub static INPUT_SELECT_ALL: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn remote_id_store() -> &'static Mutex<String> {
    REMOTE_ID_INPUT.get_or_init(|| Mutex::new(String::new()))
}

pub fn get_remote_id_raw() -> String {
    remote_id_store()
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default()
}

pub fn set_remote_id_raw(val: &str) {
    let digits: String = val.chars().filter(|c| c.is_ascii_digit()).take(9).collect();
    if let Ok(mut g) = remote_id_store().lock() {
        *g = digits;
    }
    // Mark as selected-all so typing a new digit replaces pre-filled IDs immediately!
    INPUT_SELECT_ALL.store(true, std::sync::atomic::Ordering::Relaxed);
}

pub fn clear_remote_id_raw() {
    if let Ok(mut g) = remote_id_store().lock() {
        g.clear();
    }
    INPUT_SELECT_ALL.store(false, std::sync::atomic::Ordering::Relaxed);
}

pub fn format_desky_id_display(raw: &str) -> String {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() <= 3 {
        digits
    } else if digits.len() <= 6 {
        format!("{} {}", &digits[0..3], &digits[3..])
    } else {
        let end = digits.len().min(9);
        format!("{} {} {}", &digits[0..3], &digits[3..6], &digits[6..end])
    }
}

pub fn x11_keycode_to_digit(keycode: u8) -> Option<char> {
    match keycode {
        10 | 87 => Some('1'),
        11 | 88 => Some('2'),
        12 | 89 => Some('3'),
        13 | 83 => Some('4'),
        14 | 84 => Some('5'),
        15 | 85 => Some('6'),
        16 | 79 => Some('7'),
        17 | 80 => Some('8'),
        18 | 81 => Some('9'),
        19 | 90 => Some('0'),
        _ => None,
    }
}

pub fn x11_keycode_to_ascii(keycode: u8, shift: bool) -> Option<char> {
    if let Some(d) = x11_keycode_to_digit(keycode) {
        return Some(d);
    }
    let c = match keycode {
        24 => 'q',
        25 => 'w',
        26 => 'e',
        27 => 'r',
        28 => 't',
        29 => 'y',
        30 => 'u',
        31 => 'i',
        32 => 'o',
        33 => 'p',
        38 => 'a',
        39 => 's',
        40 => 'd',
        41 => 'f',
        42 => 'g',
        43 => 'h',
        44 => 'j',
        45 => 'k',
        46 => 'l',
        52 => 'z',
        53 => 'x',
        54 => 'c',
        55 => 'v',
        56 => 'b',
        57 => 'n',
        58 => 'm',
        20 => if shift { '_' } else { '-' },
        21 => if shift { '+' } else { '=' },
        65 => ' ',
        _ => return None,
    };
    Some(if shift { c.to_ascii_uppercase() } else { c })
}

/// Processes keyboard input when the Home tab is active; returns `true` if Enter was pressed to connect
pub fn handle_remote_desk_edit_key(keycode: u8, state: KeyButMask) -> bool {
    use std::sync::atomic::Ordering;
    let is_ctrl = state.contains(KeyButMask::CONTROL);

    // Ctrl+A selects all so next key replaces the entire ID
    if is_ctrl && keycode == 38 {
        INPUT_SELECT_ALL.store(true, Ordering::Relaxed);
        return false;
    }

    // Ctrl+V paste into Remote Desk ID input
    if is_ctrl && keycode == 55 {
        if let Some(clip) = crate::linux::read_clipboard_text() {
            set_remote_id_raw(&clip);
            INPUT_SELECT_ALL.store(false, Ordering::Relaxed);
        }
        return false;
    }

    match keycode {
        22 | 119 => {
            // Backspace or Delete
            if is_ctrl || INPUT_SELECT_ALL.swap(false, Ordering::Relaxed) {
                if let Ok(mut g) = remote_id_store().lock() {
                    g.clear();
                }
            } else if let Ok(mut g) = remote_id_store().lock() {
                g.pop();
            }
            false
        }
        36 | 104 => {
            // Return or KP_Enter -> trigger connect
            true
        }
        _ => {
            if let Some(digit) = x11_keycode_to_digit(keycode) {
                let replace_all = INPUT_SELECT_ALL.swap(false, Ordering::Relaxed);
                if let Ok(mut g) = remote_id_store().lock() {
                    if replace_all || g.len() >= 9 {
                        g.clear();
                    }
                    g.push(digit);
                }
            }
            false
        }
    }
}
