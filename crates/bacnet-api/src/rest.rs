/// Axum HTTP management API.

use axum::{routing::get, Router};
use std::net::SocketAddr;

pub async fn serve(addr: SocketAddr) -> std::io::Result<()> {
    let app = Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/metrics", get(metrics_handler));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> &'static str {
    "ok"
}

async fn metrics_handler() -> String {
    crate::metrics::gather()
}
