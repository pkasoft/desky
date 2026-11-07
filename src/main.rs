pub mod identity;
pub mod p2p;
pub mod protocol;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;

use tracing::info;

fn main() -> anyhow::Result<()> {
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
        info!(" Graphics: Direct3D 11 & DXGI Pipeline");
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
