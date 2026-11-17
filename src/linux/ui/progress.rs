//! File Transfer Progress Overlay & Notification Trigger (Linux X11)
//!
//! Mirrors `src/windows/ui/progress.rs`.

#![allow(dead_code)]

use super::launcher::request_main_redraw;

pub fn trigger_transfer_progress_window() {
    request_main_redraw();
}
