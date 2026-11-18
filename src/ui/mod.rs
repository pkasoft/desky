//! Unified UI Framework & Lucide Vector Icon Suite
//! Centralizes resolution-independent design tokens, typography, and pure vector icons.

pub mod icons;

pub use icons::{
    IconCanvas, IconContext,
    render_alert_triangle, render_arrow_right, render_bell, render_check, render_chevron_left, render_chevron_right,
    render_clock, render_copy, render_cpu, render_desky_logo, render_file, render_folder, render_home, render_info,
    render_keyboard_mouse, render_lock, render_message_square, render_mic, render_mic_off, render_monitor, render_monitor_num,
    render_more_horizontal, render_pen, render_plus, render_radio, render_record, render_refresh, render_search,
    render_server, render_settings, render_terminal, render_trash, render_users, render_volume, render_volume_x,
    render_wifi, render_x, render_zap,
};

#[cfg(target_os = "windows")]
pub use icons::{
    draw_alert_triangle, draw_arrow_right, draw_bell, draw_check, draw_chevron_left, draw_chevron_right,
    draw_clock, draw_copy, draw_cpu, draw_desky_logo, draw_file, draw_folder, draw_home, draw_info,
    draw_keyboard_mouse, draw_lock, draw_message_square, draw_mic, draw_mic_off, draw_monitor, draw_monitor_num,
    draw_more_horizontal, draw_pen, draw_plus, draw_radio, draw_record, draw_refresh, draw_search,
    draw_server, draw_settings, draw_terminal, draw_trash, draw_users, draw_volume, draw_volume_x,
    draw_wifi, draw_x, draw_zap,
};
