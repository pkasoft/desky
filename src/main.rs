#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
use resky::linux;
#[cfg(target_os = "windows")]
use resky::windows;
use tracing::info;

fn main() -> anyhow::Result<()> {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("RESKY_PANIC: {}\nLocation: {:?}\nBacktrace: {:?}", info, info.location(), std::backtrace::Backtrace::capture());
        eprintln!("{}", msg);
        let _ = std::fs::write("resky_panic.log", &msg);
    }));

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "desky=info".into()),
        )
        .init();

    info!("========================================");
    info!(" Resky v{} - Native Remote Desktop", env!("CARGO_PKG_VERSION"));
    #[cfg(target_os = "windows")]
    {
        if std::env::args().any(|a| a == "--service") {
            return windows::service::run_windows_service();
        }
        if !windows::desktop::is_current_process_elevated() {
            info!("Process is not elevated; requesting Administrator privileges via UAC...");
            let _ = windows::desktop::elevate_process();
            return Ok(());
        }
        if std::env::args().any(|a| a == "--uninstall") {
            windows::install::uninstall_from_system();
            return Ok(());
        }
        if std::env::args().any(|a| a == "--install-service") {
            return windows::service::install_windows_service();
        }
        if std::env::args().any(|a| a == "--uninstall-service") {
            return windows::service::uninstall_windows_service();
        }
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

    info!("Resky shutdown cleanly.");
    Ok(())
}
