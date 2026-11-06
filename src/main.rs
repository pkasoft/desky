pub mod capture;
pub mod input;
pub mod protocol;

use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "desky=debug,info".into()),
        )
        .init();

    info!("----------------------------------------");
    info!(" Desky v{} - Remote Desktop for Rust", env!("CARGO_PKG_VERSION"));
    info!("----------------------------------------");

    let engine = capture::CaptureEngine::new()?;
    info!(
        "Detected display: {} ({}x{})",
        engine.current_display.name,
        engine.current_display.width,
        engine.current_display.height
    );

    info!("Desky host engine initialized successfully.");
    Ok(())
}
