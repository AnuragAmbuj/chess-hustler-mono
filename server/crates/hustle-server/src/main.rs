//! hustle-server — composition root (ADR-001). Boots REST; WS lands in Phase 1
//! on the same process/port (ADR-002/ADR-004).

use hustle_api::{router, AppState};
use hustle_kernel::{telemetry, ServerConfig};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ServerConfig::from_env();
    telemetry::init(&config.env);
    tracing::info!(
        env = %config.env,
        port = config.port,
        version = config.version,
        "starting hustle-server"
    );

    let host = config.host.clone();
    let port = config.port;
    let state = AppState { config };
    let app = router(state);

    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(addr = %addr, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install ctrl_c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = async {
        std::future::pending::<()>().await;
    };

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received; draining");
}
