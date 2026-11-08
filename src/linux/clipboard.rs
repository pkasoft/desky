//! Linux Platform Clipboard Operations

use std::path::PathBuf;

/// Writes an array of file paths to the Linux clipboard
pub fn write_clipboard_files(_paths: &[PathBuf]) {
    // Linux clipboard file transfer format
}

/// Writes Unicode text to the Linux clipboard
pub fn write_clipboard_text(_text: &str) {
    // Linux clipboard text format
}

/// Reads Unicode text from the Linux clipboard
pub fn read_clipboard_text() -> Option<String> {
    None
}

/// Reads dropped/copied file paths from the Linux clipboard
pub fn read_clipboard_files() -> Vec<PathBuf> {
    Vec::new()
}
