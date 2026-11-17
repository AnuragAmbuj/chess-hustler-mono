//! Process configuration from environment variables (12-factor).
//!
//! Every value has a default that matches the seeded `app_config` rows so a
//! bare `cargo run` reproduces the documented economy (docs/04-economy/*).
//! Runtime *tunables* move to DB-backed `app_config` in Phase 1 — these env
//! values are bootstrap/static config only, never hot rules.

use std::env;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Deployment environment name: local | staging | prod.
    pub env: String,
    pub host: String,
    pub port: u16,
    pub version: &'static str,

    // --- Public economy defaults (mirrored by /v1/meta/config; enforced server-side later) ---
    pub welcome_gold: i64,
    pub ads_gold_per_ad: i64,
    pub ads_max_per_day: u32,
    pub ads_cooldown_s: u64,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        Self {
            env: env::var("CH_ENV").unwrap_or_else(|_| "local".to_string()),
            host: env::var("CH_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: num_env("CH_PORT", 8080),
            version: env!("CARGO_PKG_VERSION"),
            welcome_gold: num_env("CH_ECONOMY_WELCOME_GOLD", 50_000),
            ads_gold_per_ad: num_env("CH_ADS_GOLD_PER_AD", 2_000),
            ads_max_per_day: num_env("CH_ADS_MAX_PER_DAY", 5),
            ads_cooldown_s: num_env("CH_ADS_COOLDOWN_S", 60),
        }
    }
}

fn num_env<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
