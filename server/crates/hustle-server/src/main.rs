//! hustle-server — composition root (ADR-001). Boots REST, WebSocket gateway,
//! migrations, and background workers (matchmaking sweeper & tournament advancer).

use hustle_api::{router, AppState};
use hustle_kernel::{telemetry, ServerConfig};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::watch;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ServerConfig::from_env();
    telemetry::init(&config.env);

    if let Err(reason) = config.validate_for_production() {
        anyhow::bail!("production configuration check failed: {reason}");
    }

    tracing::info!(
        env = %config.env,
        port = config.port,
        version = config.version,
        "starting hustle-server"
    );

    let host = config.host.clone();
    let port = config.port;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(16)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;
    tracing::info!("postgres pool ready");

    if config.run_migrations {
        sqlx::migrate!("../../migrations").run(&pool).await?;
        tracing::info!("database migrations up to date");
    }

    let state = AppState::build(config, pool);

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let sweeper_handle = spawn_matchmaking_sweeper(state.clone(), shutdown_rx.clone());
    let tournament_handle = spawn_tournament_advancer(state.clone(), shutdown_rx);

    let app = router(state);

    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(addr = %addr, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            let _ = shutdown_tx.send(true);
        })
        .await?;

    let _ = tokio::join!(sweeper_handle, tournament_handle);
    tracing::info!("server shutdown complete");
    Ok(())
}

fn spawn_matchmaking_sweeper(
    state: AppState,
    mut shutdown: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(500));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
                _ = ticker.tick() => {
                    match state.matchmaking.sweep_all().await {
                        Ok(matches) if !matches.is_empty() => {
                            tracing::info!(matched = matches.len(), "matchmaking sweep paired games");
                        }
                        Ok(_) => {}
                        Err(err) => {
                            tracing::warn!(error = %err, "matchmaking sweep failed");
                        }
                    }
                }
            }
        }
    })
}

fn spawn_tournament_advancer(
    state: AppState,
    mut shutdown: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(15));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
                _ = ticker.tick() => {
                    let active_ids: Result<Vec<i64>, _> = sqlx::query_scalar(
                        "SELECT id FROM tournaments WHERE status IN ('registration', 'in_progress') AND starts_at <= NOW()",
                    )
                    .fetch_all(&state.pool)
                    .await;

                    if let Ok(ids) = active_ids {
                        for id in ids {
                            if let Ok(pairings) = state.tournament_service.advance_tournament(id).await {
                                tracing::info!(
                                    tournament_id = id,
                                    pairings = pairings.len(),
                                    "advanced tournament round"
                                );
                            }
                        }
                    }
                }
            }
        }
    })
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
