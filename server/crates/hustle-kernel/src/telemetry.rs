//! Structured logging bootstrap (tracing). OTel exporter wiring lands in
//! Phase 1; the `tracing` facade is already the app-wide instrumentation API.

use tracing_subscriber::EnvFilter;

pub fn init(env_name: &str) {
    let default_level = if env_name == "local" { "debug" } else { "info" };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
