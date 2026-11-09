#![allow(dead_code)]
//! Windows OS Platform Modules & Runtime Architecture
//!
//! Subdivided by OS family:
//! - `win10_11`: Windows 10 & Windows 11
//! - `server_2022`: Windows Server 2022 & 2025
//! - `server_2016_2019`: Windows Server 2016 & 2019
//! - `server_2012`: Windows Server 2012 & 2012 R2
//! - `win7_8`: Windows 7, Windows 8 & 8.1

pub mod admin;
pub mod audio;
pub mod capture;
pub mod clipboard;
pub mod cursor;
pub mod dxgi_capture;
pub mod input;
pub mod os_version;
pub use crate::recorder;
pub mod server_2012;
pub mod server_2016_2019;
pub mod server_2022;
pub mod system;
pub mod ui;
pub mod win10_11;
pub mod win7_8;

pub use capture::capture_screen_frame;
pub use clipboard::{
    is_clipboard_recently_synced, mark_clipboard_synced, read_clipboard_files,
    read_clipboard_text, write_clipboard_files, write_clipboard_text,
};
pub use cursor::{get_current_cursor_shape, get_hcursor_for_shape};
pub use input::InputInjector;
pub use os_version::{get_platform, WindowsPlatform};
pub use system::{get_desktop_dir, get_identity_dir, get_machine_id, notify_file_received, trim_process_working_set};
pub use ui::set_assigned_desky_id;

use tracing::info;

pub fn run() -> anyhow::Result<()> {
    let platform = get_platform();
    info!("🚀 Running Desky on Platform Engine: {:?}", platform);

    match platform {
        WindowsPlatform::Win10_11 => win10_11::init(),
        WindowsPlatform::Server2022 => server_2022::init(),
        WindowsPlatform::Server2016_2019 => server_2016_2019::init(),
        WindowsPlatform::Server2012 => server_2012::init(),
        WindowsPlatform::Win7_8 => win7_8::init(),
    }

    let ident = crate::identity::load_or_create_identity();
    tracing::info!(
        "Loaded Client Identity: Machine GUID = {}, Cached ID = {:?}",
        ident.machine_id, ident.desky_id
    );

    let vds_server = std::env::var("DESKY_VDS_SERVER")
        .unwrap_or_else(|_| "0.0.0.0:21116".into());
    tracing::info!("Connecting P2P Engine to VDS Matchmaker: {}", vds_server);

    let p2p = crate::p2p::P2PManager::new(ident.machine_id, ident.desky_id, vds_server);
    p2p.start_host_registration();

    crate::p2p::start_lan_radar(|| {
        if let Ok(g) = ui::LOCAL_DESKY_ID.lock() {
            g.clone()
        } else {
            String::new()
        }
    });

    tracing::info!("Spawning native Win32 window and starting message pump...");
    let app = ui::NativeWin32App::new()?;
    app.run_message_loop();
    Ok(())
}
