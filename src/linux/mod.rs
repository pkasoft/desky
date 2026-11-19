pub mod admin;
pub mod capture;
pub mod clipboard;
pub mod codec;
pub mod input;
pub mod system;
pub mod tray;
pub mod ui;

pub use capture::capture_screen_frame;
#[allow(unused_imports)]
pub use clipboard::{
    is_clipboard_recently_synced, mark_clipboard_synced, read_clipboard_files,
    read_clipboard_text, write_clipboard_files, write_clipboard_text,
};
pub use input::InputInjector;
pub use system::{get_desktop_dir, get_identity_dir, get_machine_id, notify_file_received};
pub use ui::set_assigned_desky_id;

pub fn run() -> anyhow::Result<()> {
    let ident = crate::identity::load_or_create_identity();
    tracing::info!(
        "Loaded Client Identity: Machine GUID = {}, Cached ID = {:?}",
        ident.machine_id,
        ident.desky_id
    );

    let vds_server = std::env::var("DESKY_VDS_SERVER")
        .unwrap_or_else(|_| crate::config::DEFAULT_VDS_SERVER.into());
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

    codec::probe_hardware_codecs();

    ui::append_status_log("Desky Linux X11 Engine initialized.");
    let app = ui::LinuxApp::new()?;
    app.run();
    Ok(())
}
