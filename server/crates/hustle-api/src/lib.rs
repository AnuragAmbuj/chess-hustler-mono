//! hustle-api — HTTP surface (axum). Phase 0: health/ready/meta skeleton with
//! the contract-drift gate. Phase 1 expands per docs/03-api/rest.md; every new
//! route MUST be added to `ROUTES` AND `contracts/openapi.yaml` — the
//! `contract_drift` test fails CI otherwise (ADR-009, ADR-012).

pub mod admin;
pub mod ads;
pub mod auth;
pub mod cosmetics;
pub mod error;
pub mod friends;
pub mod games;
pub mod leaderboards;
pub mod matchmaking;
pub mod me;
pub mod moderation;
pub mod notifications;
pub mod realtime;
pub mod rewards;
pub mod tournaments;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use hustle_domain::{pool_for_time_control, TimeControl};
use hustle_gold::PgGoldLedger;
use hustle_identity::{OtpService, SessionStore, SignupService, TokenService};
use hustle_kernel::ServerConfig;
use hustle_matchmaking::MatchmakingService;
use serde_json::json;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: ServerConfig,
    pub pool: sqlx::PgPool,
    pub redis: redis::Client,
    pub otp: Arc<OtpService>,
    pub signup: Arc<SignupService>,
    pub sessions: Arc<SessionStore>,
    pub tokens: Arc<TokenService>,
    pub ledger: Arc<PgGoldLedger>,
    pub matchmaking: Arc<MatchmakingService>,
    pub ticket_store: Arc<hustle_realtime::TicketStore>,
    pub publisher: Arc<hustle_realtime::EventPublisher>,
    pub game_service: Arc<hustle_chess::GameService>,
    pub friend_service: Arc<hustle_friends::FriendService>,
    pub challenge_service: Arc<hustle_friends::ChallengeService>,
    pub moderation_service: Arc<hustle_moderation::ModerationService>,
    pub cosmetics_service: Arc<hustle_cosmetics::CosmeticsService>,
    pub reward_service: Arc<hustle_rewards::RewardService>,
    pub ad_verifier: Arc<hustle_ads::AdMobVerifier>,
    pub ads_service: Arc<hustle_ads::AdsService>,
    pub tournament_service: Arc<hustle_tournaments::TournamentService>,
}

impl AppState {
    /// Standard wiring: services over PostgreSQL and Redis.
    pub fn build(config: ServerConfig, pool: sqlx::PgPool) -> Self {
        let tokens = Arc::new(TokenService::new(config.jwt_secret.as_bytes()));
        let signup = Arc::new(SignupService::new(
            pool.clone(),
            TokenService::new(config.jwt_secret.as_bytes()),
            config.welcome_gold,
        ));
        let redis_client = redis::Client::open(config.redis_url.as_str()).expect("valid redis url");
        let otp_sender: Box<dyn hustle_identity::OtpSender> = match (
            config.otp_provider.as_str(),
            config.whatsapp_webhook_url.as_ref(),
        ) {
            ("whatsapp", Some(url)) => {
                tracing::info!(endpoint = %url, "using WhatsApp webhook OTP sender");
                Box::new(hustle_identity::WhatsAppWebhookSender::new(
                    url.clone(),
                    config.whatsapp_webhook_token.clone(),
                ))
            }
            _ => Box::new(hustle_identity::LogOtpSender),
        };
        let otp = Arc::new(OtpService::with_redis(otp_sender, redis_client.clone()));
        let sessions = Arc::new(SessionStore::new(pool.clone()));
        let ledger = Arc::new(PgGoldLedger::new(pool.clone()));
        let matchmaking = Arc::new(MatchmakingService::new(pool.clone(), redis_client.clone()));
        let ticket_store = Arc::new(hustle_realtime::TicketStore::new(redis_client.clone()));
        let publisher = Arc::new(hustle_realtime::EventPublisher::new(redis_client.clone()));
        let game_service = Arc::new(hustle_chess::GameService::new(pool.clone()));
        let friend_service = Arc::new(hustle_friends::FriendService::new(
            pool.clone(),
            redis_client.clone(),
            (*publisher).clone(),
        ));
        let challenge_service = Arc::new(hustle_friends::ChallengeService::new(
            pool.clone(),
            redis_client.clone(),
            (*publisher).clone(),
            (*game_service).clone(),
        ));
        let moderation_service = Arc::new(hustle_moderation::ModerationService::new(
            pool.clone(),
            redis_client.clone(),
            (*publisher).clone(),
        ));
        let cosmetics_service = Arc::new(hustle_cosmetics::CosmeticsService::new(pool.clone()));
        let reward_service = Arc::new(hustle_rewards::RewardService::new(pool.clone()));
        let ad_verifier = Arc::new(hustle_ads::AdMobVerifier::new());
        if let Some(ref json) = config.admob_keys_json {
            match ad_verifier.load_from_json(json) {
                Ok(n) => tracing::info!(count = n, "loaded AdMob SSV keys from CH_ADMOB_KEYS_JSON"),
                Err(e) => tracing::error!(error = %e, "failed to parse CH_ADMOB_KEYS_JSON"),
            }
        }
        if let Some(ref path) = config.admob_keys_file {
            match std::fs::read_to_string(path) {
                Ok(contents) => match ad_verifier.load_from_json(&contents) {
                    Ok(n) => tracing::info!(count = n, path = %path, "loaded AdMob SSV keys from file"),
                    Err(e) => tracing::error!(error = %e, path = %path, "failed to parse AdMob keys file"),
                },
                Err(e) => tracing::error!(error = %e, path = %path, "failed to read AdMob keys file"),
            }
        }
        let ads_service = Arc::new(hustle_ads::AdsService::new(pool.clone(), ad_verifier.clone()));
        let tournament_service = Arc::new(hustle_tournaments::TournamentService::new(pool.clone()));
        Self {
            config,
            pool,
            redis: redis_client,
            otp,
            signup,
            sessions,
            tokens,
            ledger,
            matchmaking,
            ticket_store,
            publisher,
            game_service,
            friend_service,
            challenge_service,
            moderation_service,
            cosmetics_service,
            reward_service,
            ad_verifier,
            ads_service,
            tournament_service,
        }
    }

    /// Construct a test stub with a lazy, unconnected pool (for route/smoke tests that don't hit PG).
    pub fn test_stub() -> Self {
        let config = ServerConfig::from_env();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://127.0.0.1:54329/hustle_stub")
            .expect("lazy stub pool");
        let mut state = Self::build(config, pool);
        state.otp = Arc::new(OtpService::new(Box::new(hustle_identity::LogOtpSender)));
        state
    }
}

/// Single source of truth for served routes (method, path).
/// Consumed by `router()` and asserted against the contract by the drift test.
pub const ROUTES: &[(http::Method, &str)] = &[
    (http::Method::GET, "/"),
    (http::Method::GET, "/healthz"),
    (http::Method::GET, "/readyz"),
    (http::Method::GET, "/v1/meta/config"),
    (http::Method::POST, "/v1/auth/otp/send"),
    (http::Method::POST, "/v1/auth/otp/verify"),
    (http::Method::POST, "/v1/auth/refresh"),
    (http::Method::POST, "/v1/auth/logout"),
    (http::Method::POST, "/v1/auth/logout-all"),
    (http::Method::GET, "/v1/me"),
    (http::Method::PATCH, "/v1/me"),
    (http::Method::DELETE, "/v1/me"),
    (http::Method::POST, "/v1/me/deletion-cancel"),
    (http::Method::GET, "/v1/me/export"),
    (http::Method::GET, "/v1/me/gold"),
    (http::Method::GET, "/v1/me/daily"),
    (http::Method::POST, "/v1/me/daily/claim"),
    (http::Method::GET, "/v1/me/gold/summary"),
    (http::Method::GET, "/v1/me/missions"),
    (http::Method::POST, "/v1/me/missions/{id}/claim"),
    (http::Method::GET, "/v1/me/achievements"),
    (http::Method::GET, "/v1/players/{handle}"),
    (http::Method::GET, "/v1/players/{handle}/games"),
    (http::Method::GET, "/v1/leaderboards"),
    (http::Method::POST, "/v1/matchmaking/queue"),
    (http::Method::DELETE, "/v1/matchmaking/queue"),
    (http::Method::GET, "/v1/matchmaking/status"),
    (http::Method::GET, "/v1/realtime/ticket"),
    (http::Method::GET, "/v1/games/{id}"),
    (http::Method::GET, "/v1/games/{id}/moves"),
    (http::Method::GET, "/v1/games/{id}/pgn"),
    (http::Method::POST, "/v1/games/{id}/resign"),
    (http::Method::POST, "/v1/games/{id}/draw-offer"),
    (http::Method::POST, "/v1/games/{id}/draw-accept"),
    (http::Method::POST, "/v1/games/{id}/draw-decline"),
    (http::Method::POST, "/v1/games/{id}/abort"),
    (http::Method::POST, "/v1/games/{id}/claim-timeout"),
    (http::Method::GET, "/v1/friends"),
    (http::Method::GET, "/v1/friends/requests"),
    (http::Method::POST, "/v1/friends/requests"),
    (http::Method::POST, "/v1/friends/requests/{id}/accept"),
    (http::Method::POST, "/v1/friends/requests/{id}/decline"),
    (http::Method::DELETE, "/v1/friends/{id}"),
    (http::Method::GET, "/v1/blocks"),
    (http::Method::POST, "/v1/blocks"),
    (http::Method::DELETE, "/v1/blocks/{handle}"),
    (http::Method::POST, "/v1/challenge"),
    (http::Method::POST, "/v1/challenge/{id}/accept"),
    (http::Method::POST, "/v1/challenge/{id}/decline"),
    (http::Method::POST, "/v1/reports"),
    (http::Method::GET, "/v1/grievance"),
    (http::Method::GET, "/v1/games/{id}/chat"),
    (http::Method::POST, "/v1/games/{id}/chat"),
    (http::Method::GET, "/v1/cosmetics"),
    (http::Method::GET, "/v1/cosmetics/{id}"),
    (http::Method::POST, "/v1/cosmetics/{id}/purchase"),
    (http::Method::GET, "/v1/me/loadout"),
    (http::Method::PUT, "/v1/me/loadout"),
    (http::Method::GET, "/v1/admin/flags"),
    (http::Method::POST, "/v1/admin/flags/{id}/resolve"),
    (http::Method::GET, "/v1/admin/players/{id}"),
    (http::Method::POST, "/v1/admin/players/{id}/sanctions"),
    (http::Method::GET, "/v1/admin/reports"),
    (http::Method::POST, "/v1/admin/reports/{id}/action"),
    (http::Method::GET, "/v1/admin/audit"),
    (http::Method::GET, "/v1/notifications"),
    (http::Method::POST, "/v1/notifications/read"),
    (http::Method::GET, "/v1/admin/config/{key}"),
    (http::Method::PUT, "/v1/admin/config/{key}"),
    (http::Method::POST, "/v1/ads/session"),
    (http::Method::GET, "/v1/ads/status"),
    (http::Method::GET, "/v1/ads/ssv/{provider}/callback"),
    (http::Method::GET, "/v1/tournaments"),
    (http::Method::GET, "/v1/tournaments/{id}"),
    (http::Method::POST, "/v1/tournaments/{id}/join"),
    (http::Method::POST, "/v1/tournaments/{id}/withdraw"),
    (http::Method::POST, "/v1/admin/tournaments"),
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
        .route("/v1/auth/otp/send", post(auth::otp_send))
        .route("/v1/auth/otp/verify", post(auth::otp_verify))
        .route("/v1/auth/refresh", post(auth::refresh))
        .route("/v1/auth/logout", post(auth::logout))
        .route("/v1/auth/logout-all", post(auth::logout_all))
        .route(
            "/v1/me",
            get(me::get_me).patch(me::patch_me).delete(me::delete_me),
        )
        .route("/v1/me/deletion-cancel", post(me::cancel_deletion))
        .route("/v1/me/export", get(me::export_me))
        .route("/v1/me/gold", get(me::get_me_gold))
        .route("/v1/me/daily", get(rewards::get_daily))
        .route("/v1/me/daily/claim", post(rewards::claim_daily))
        .route("/v1/me/gold/summary", get(rewards::get_gold_summary))
        .route("/v1/me/missions", get(rewards::get_my_missions))
        .route(
            "/v1/me/missions/{id}/claim",
            post(rewards::claim_my_mission),
        )
        .route("/v1/me/achievements", get(rewards::get_my_achievements))
        .route("/v1/players/{handle}", get(me::get_player_profile))
        .route("/v1/players/{handle}/games", get(me::get_player_games))
        .route("/v1/leaderboards", get(leaderboards::get_leaderboard))
        .route(
            "/v1/matchmaking/queue",
            post(matchmaking::queue_join).delete(matchmaking::queue_leave),
        )
        .route("/v1/matchmaking/status", get(matchmaking::queue_status))
        .route("/v1/realtime/ticket", get(realtime::get_ticket))
        .route("/v1/games/{id}", get(games::get_game))
        .route("/v1/games/{id}/moves", get(games::get_game_moves))
        .route("/v1/games/{id}/pgn", get(games::get_game_pgn))
        .route("/v1/games/{id}/resign", post(games::post_game_resign))
        .route("/v1/games/{id}/draw-offer", post(games::post_draw_offer))
        .route("/v1/games/{id}/draw-accept", post(games::post_draw_accept))
        .route(
            "/v1/games/{id}/draw-decline",
            post(games::post_draw_decline),
        )
        .route("/v1/games/{id}/abort", post(games::post_game_abort))
        .route(
            "/v1/games/{id}/claim-timeout",
            post(games::post_claim_timeout),
        )
        .route("/v1/friends", get(friends::get_friends))
        .route(
            "/v1/friends/requests",
            get(friends::get_requests).post(friends::post_request),
        )
        .route(
            "/v1/friends/requests/{id}/accept",
            post(friends::post_accept_request),
        )
        .route(
            "/v1/friends/requests/{id}/decline",
            post(friends::post_decline_request),
        )
        .route("/v1/friends/{id}", delete(friends::delete_friend))
        .route(
            "/v1/blocks",
            get(friends::get_blocks).post(friends::post_block),
        )
        .route("/v1/blocks/{handle}", delete(friends::delete_block))
        .route("/v1/challenge", post(friends::post_challenge))
        .route(
            "/v1/challenge/{id}/accept",
            post(friends::post_accept_challenge),
        )
        .route(
            "/v1/challenge/{id}/decline",
            post(friends::post_decline_challenge),
        )
        .route("/v1/reports", post(moderation::post_report))
        .route("/v1/grievance", get(moderation::get_grievance))
        .route(
            "/v1/games/{id}/chat",
            get(moderation::get_game_chat).post(moderation::post_game_chat),
        )
        .route("/v1/cosmetics", get(cosmetics::get_cosmetics))
        .route("/v1/cosmetics/{id}", get(cosmetics::get_cosmetic_item))
        .route(
            "/v1/cosmetics/{id}/purchase",
            post(cosmetics::post_purchase_cosmetic),
        )
        .route(
            "/v1/me/loadout",
            get(cosmetics::get_my_loadout).put(cosmetics::put_my_loadout),
        )
        .route("/v1/admin/flags", get(admin::get_admin_flags))
        .route(
            "/v1/admin/flags/{id}/resolve",
            post(admin::post_admin_resolve_flag),
        )
        .route("/v1/admin/players/{id}", get(admin::get_admin_player))
        .route(
            "/v1/admin/players/{id}/sanctions",
            post(admin::post_admin_player_sanction),
        )
        .route("/v1/admin/reports", get(admin::get_admin_reports))
        .route(
            "/v1/admin/reports/{id}/action",
            post(admin::post_admin_action_report),
        )
        .route("/v1/admin/audit", get(admin::get_admin_audit))
        .route("/v1/notifications", get(notifications::get_notifications))
        .route(
            "/v1/notifications/read",
            post(notifications::post_notifications_read),
        )
        .route(
            "/v1/admin/config/{key}",
            get(admin::get_admin_config).put(admin::put_admin_config),
        )
        .route("/v1/ads/session", post(ads::create_ad_session))
        .route("/v1/ads/status", get(ads::get_ad_status))
        .route(
            "/v1/ads/ssv/{provider}/callback",
            get(ads::handle_ad_ssv_callback),
        )
        .route("/v1/tournaments", get(tournaments::get_tournaments))
        .route(
            "/v1/tournaments/{id}",
            get(tournaments::get_tournament_detail),
        )
        .route(
            "/v1/tournaments/{id}/join",
            post(tournaments::post_join_tournament),
        )
        .route(
            "/v1/tournaments/{id}/withdraw",
            post(tournaments::post_withdraw_tournament),
        )
        .route(
            "/v1/admin/tournaments",
            post(tournaments::post_create_tournament),
        )
        .route("/ws", get(realtime::ws_upgrade_handler))
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

/// Readiness — verifies live PostgreSQL and Redis connectivity with a short timeout.
async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let timeout = std::time::Duration::from_secs(2);
    let pg_ok = tokio::time::timeout(
        timeout,
        sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.pool),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false);

    let redis_ok = tokio::time::timeout(timeout, async {
        match state.redis.get_multiplexed_async_connection().await {
            Ok(mut conn) => redis::cmd("PING")
                .query_async::<String>(&mut conn)
                .await
                .is_ok(),
            Err(_) => false,
        }
    })
    .await
    .unwrap_or(false);

    if pg_ok && redis_ok {
        (
            StatusCode::OK,
            Json(json!({ "status": "ready", "postgres": true, "redis": true })),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "not_ready", "postgres": pg_ok, "redis": redis_ok })),
        )
    }
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
