//! hustle-api — HTTP surface (axum). Phase 0: health/ready/meta skeleton with
//! the contract-drift gate. Phase 1 expands per docs/03-api/rest.md; every new
//! route MUST be added to `ROUTES` AND `contracts/openapi.yaml` — the
//! `contract_drift` test fails CI otherwise (ADR-009, ADR-012).

use axum::{extract::State, routing::get, Json, Router};
use hustle_domain::{pool_for_time_control, TimeControl};
use hustle_kernel::ServerConfig;
use serde_json::json;

#[derive(Clone)]
pub struct AppState {
    pub config: ServerConfig,
}

/// Single source of truth for served routes (method, path).
/// Consumed by `router()` and asserted against the contract by the drift test.
pub const ROUTES: &[(http::Method, &str)] = &[
    (http::Method::GET, "/"),
    (http::Method::GET, "/healthz"),
    (http::Method::GET, "/readyz"),
    (http::Method::GET, "/v1/meta/config"),
];

/// Time-control presets served to clients (pool games; BYO-TC is friends-only
/// in later phases). Values mirror the PRD MM-1 defaults.
pub const TC_PRESETS: &[(u32, u32)] = &[
    (60, 0),
    (60, 2),
    (180, 0),
    (180, 2),
    (300, 0),
    (600, 0),
    (600, 5),
    (900, 10),
    (1800, 20),
];

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/v1/meta/config", get(meta_config))
        .with_state(state)
}

async fn root() -> Json<serde_json::Value> {
    Json(json!({
        "service": "chess-hustle",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Liveness — process is up. Never checks dependencies.
async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

/// Readiness — Phase 0 always ready; Phase 1 adds DB/Redis round-trips.
async fn readyz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ready" }))
}

/// Public client boot config. Values shown to users MUST come from here or
/// `app_config` — never hardcoded in clients (mandate: config, not code).
async fn meta_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    let c = &state.config;
    let time_controls: Vec<serde_json::Value> = TC_PRESETS
        .iter()
        .map(|(initial_s, increment_s)| {
            let tc = TimeControl {
                initial_ms: initial_s * 1000,
                increment_ms: increment_s * 1000,
            };
            json!({
                "initial_s": initial_s,
                "increment_s": increment_s,
                "pool": pool_for_time_control(&tc).as_str(),
            })
        })
        .collect();

    Json(json!({
        "server_time_unix_ms": server_time_unix_ms(),
        "economy": {
            "welcome_gold": c.welcome_gold,
        },
        "ads": {
            "gold_per_ad": c.ads_gold_per_ad,
            "max_per_day": c.ads_max_per_day,
            "cooldown_s": c.ads_cooldown_s,
        },
        "time_controls": time_controls,
    }))
}

fn server_time_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hustle_domain::Pool;

    #[test]
    fn presets_span_all_four_pools() {
        let pools: Vec<Pool> = TC_PRESETS
            .iter()
            .map(|(s, i)| {
                pool_for_time_control(&TimeControl {
                    initial_ms: s * 1000,
                    increment_ms: i * 1000,
                })
            })
            .collect();
        for p in Pool::ALL {
            assert!(pools.contains(&p), "no preset maps to pool {p}");
        }
    }
}
