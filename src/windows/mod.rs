pub mod capture;
pub mod input;
pub mod server2022;
pub mod ui;

pub use ui::set_assigned_desky_id;

pub fn run() -> anyhow::Result<()> {
    // Initialize Windows Server & High-DPI platform compatibility
    server2022::init_server_compatibility();

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

    tracing::info!("Spawning native Win32 window and starting message pump...");
    let app = ui::NativeWin32App::new()?;
    app.run_message_loop();
    Ok(())
}
