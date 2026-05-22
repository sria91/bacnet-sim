use std::net::SocketAddr;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Structured logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    info!(
        version = env!("CARGO_PKG_VERSION"),
        "bacnet-sim starting"
    );

    // Start management API
    let api_addr: SocketAddr = "0.0.0.0:8080".parse()?;
    tokio::spawn(async move {
        if let Err(e) = bacnet_api::rest::serve(api_addr).await {
            tracing::error!("REST API error: {e}");
        }
    });

    info!("Management API listening on http://0.0.0.0:8080");
    info!("BACnet/IP transport not yet started — run Phase 1 implementation");

    // Park the main task
    tokio::signal::ctrl_c().await?;
    info!("Shutting down");
    Ok(())
}
