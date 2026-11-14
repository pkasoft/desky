#![cfg_attr(windows, windows_subsystem = "windows")]

mod identity;
mod p2p;
mod protocol;
pub mod media;
pub use media::recorder;
pub mod codec;

#[cfg(target_os = "windows")]
pub mod ui;

#[cfg(target_os = "windows")]
pub(crate) mod windows;
#[cfg(target_os = "windows")]
pub(crate) use windows as platform;

#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux as platform;

use tracing::info;

fn main() -> anyhow::Result<()> {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("DESKY_PANIC: {}\nLocation: {:?}\nBacktrace: {:?}", info, info.location(), std::backtrace::Backtrace::capture());
        eprintln!("{}", msg);
        let _ = std::fs::write("desky_panic.log", &msg);
    }));

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "desky=info".into()),
        )
        .init();

    info!("========================================");
    info!(" Desky v{} - Native Remote Desktop", env!("CARGO_PKG_VERSION"));
    #[cfg(target_os = "windows")]
    {
        info!(" Operating System: Windows 64-bit");
        info!(" UI Architecture: Direct Win32 pkasoft.com White Theme");
        info!(" Graphics: High-Speed GDI & 4:4:4 Pipeline");
        windows::run()?;
    }
    #[cfg(target_os = "linux")]
    {
        info!(" Operating System: Linux (Ubuntu / Lubuntu)");
        info!(" UI Architecture: Native X11 Protocol");
        info!(" Graphics: Pure Rust X11 Pipeline & XTest Input");
        linux::run()?;
    }
    info!("========================================");

    info!("Desky shutdown cleanly.");
    Ok(())
}
