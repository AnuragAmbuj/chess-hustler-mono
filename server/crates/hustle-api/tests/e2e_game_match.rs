//! End-to-End Scenario Test: Identity -> Gold -> Profile -> Matchmaking -> Game Pairing -> DPDP.
//!
//! Simulates two players ("ananya" and "vikram") through the complete Phase 1 flow
//! against live PostgreSQL 16 and Redis 6.

use async_trait::async_trait;
use http_body_util::BodyExt;
use hustle_api::{router, AppState};
use hustle_domain::Pool;
use hustle_identity::{OtpError, OtpSender, OtpService};
use hustle_kernel::ServerConfig;
use hustle_matchmaking::QueueMode;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

struct CapturingOtpSender(Arc<Mutex<Option<String>>>);

#[async_trait]
impl OtpSender for CapturingOtpSender {
    async fn send(&self, _phone: &str, code: &str) -> Result<(), OtpError> {
        *self.0.lock().unwrap() = Some(code.to_string());
        Ok(())
    }
}

fn counter() -> u64 {
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

fn next_phone() -> String {
    format!("+9198765{:05}", counter())
}

async fn api_request(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<serde_json::Value>,
) -> (u16, serde_json::Value) {
    let mut builder = axum::http::Request::builder().method(method).uri(uri);

    if let Some(t) = token {
        builder = builder.header(axum::http::header::AUTHORIZATION, format!("Bearer {t}"));
    }
    if body.is_some() {
        builder = builder.header(axum::http::header::CONTENT_TYPE, "application/json");
    }

    let payload = match body {
        Some(b) => axum::body::Body::from(serde_json::to_vec(&b).unwrap()),
        None => axum::body::Body::empty(),
    };

    let req = builder.body(payload).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status().as_u16();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn full_player_lifecycle_and_matchmaking_e2e() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let Some(_redis) = hustle_testkit::redis().await else {
        eprintln!("skip: no CH_TEST_REDIS_URL");
        return;
    };

    let mut config = ServerConfig::from_env();
    if let Ok(redis_url) = std::env::var("CH_TEST_REDIS_URL") {
        config.redis_url = redis_url;
    }
    let mut state = AppState::build(config, pool.clone());
    let captured_otp = Arc::new(Mutex::new(None));
    state.otp = Arc::new(OtpService::new(Box::new(CapturingOtpSender(
        captured_otp.clone(),
    ))));
    let app = router(state.clone());

    println!("--- 1. Check Boot Endpoints ---");
    let (status, body) = api_request(&app, "GET", "/healthz", None, None).await;
    assert_eq!(status, 200);
    assert_eq!(body["status"], "ok");

    let (status, body) = api_request(&app, "GET", "/readyz", None, None).await;
    assert_eq!(status, 200);
    assert_eq!(body["status"], "ready");

    let (status, body) = api_request(&app, "GET", "/v1/meta/config", None, None).await;
    assert_eq!(status, 200);
    assert_eq!(body["economy"]["welcome_gold"], 50000);
    assert!(body["time_controls"].as_array().unwrap().len() >= 4);

    println!("--- 2. Signup Player 1 (Ananya) ---");
    let phone1 = next_phone();
    let handle1 = format!("ananya_{}", counter());

    // Send OTP
    let (status, _) = api_request(
        &app,
        "POST",
        "/v1/auth/otp/send",
        None,
        Some(json!({ "phone": phone1 })),
    )
    .await;
    assert_eq!(status, 204);

    let code1 = captured_otp
        .lock()
        .unwrap()
        .take()
        .expect("code was captured");

    let (status, body1) = api_request(
        &app,
        "POST",
        "/v1/auth/otp/verify",
        None,
        Some(json!({
            "phone": phone1,
            "code": code1,
            "age_confirmed": true,
            "consents": {
                "essential": true,
                "functional": true,
                "marketing": false,
                "personalised_ads": false,
                "chat": true
            },
            "profile": {
                "handle": handle1,
                "display_name": "Ananya Plays"
            }
        })),
    )
    .await;
    assert_eq!(status, 200, "Ananya verify failed: {body1:?}");
    let token1 = body1["access_token"].as_str().unwrap().to_string();
    assert_eq!(body1["user"]["gold"], 50000);
    assert_eq!(body1["user"]["ratings"]["blitz"], 1200);

    println!("--- 3. Ananya Checks Profile & Gold Ledger ---");
    let (status, me1) = api_request(&app, "GET", "/v1/me", Some(&token1), None).await;
    assert_eq!(status, 200);
    assert_eq!(me1["handle"], handle1);
    assert_eq!(me1["gold"], 50000);
    assert_eq!(me1["ratings"]["blitz"], 1200);

    let (status, gold1) = api_request(&app, "GET", "/v1/me/gold", Some(&token1), None).await;
    assert_eq!(status, 200);
    assert_eq!(gold1["balance"], 50000);
    assert_eq!(gold1["items"].as_array().unwrap().len(), 1);
    assert_eq!(gold1["items"][0]["reason"], "welcome");

    println!("--- 4. Signup Player 2 (Vikram) ---");
    let phone2 = next_phone();
    let handle2 = format!("vikram_{}", counter());

    api_request(
        &app,
        "POST",
        "/v1/auth/otp/send",
        None,
        Some(json!({ "phone": phone2 })),
    )
    .await;

    let code2 = captured_otp
        .lock()
        .unwrap()
        .take()
        .expect("code was captured");

    let (status, body2) = api_request(
        &app,
        "POST",
        "/v1/auth/otp/verify",
        None,
        Some(json!({
            "phone": phone2,
            "code": code2,
            "age_confirmed": true,
            "consents": {
                "essential": true,
                "functional": true,
                "marketing": false,
                "personalised_ads": false,
                "chat": true
            },
            "profile": {
                "handle": handle2,
                "display_name": "Vikram64"
            }
        })),
    )
    .await;
    assert_eq!(status, 200, "Vikram verify failed: {body2:?}");
    let token2 = body2["access_token"].as_str().unwrap().to_string();

    println!("--- 5. Matchmaking Queue Join ---");
    // Ananya checks status: initially not queued
    let (status, status1) =
        api_request(&app, "GET", "/v1/matchmaking/status", Some(&token1), None).await;
    assert_eq!(status, 200);
    assert_eq!(status1["queued"], false);

    // Ananya joins Blitz queue
    let (status, receipt1) = api_request(
        &app,
        "POST",
        "/v1/matchmaking/queue",
        Some(&token1),
        Some(json!({
            "tc": { "initial_s": 180, "increment_s": 2 },
            "rated": true
        })),
    )
    .await;
    assert_eq!(status, 202);
    assert_eq!(receipt1["pool"], "blitz");
    assert_eq!(receipt1["ratings"]["blitz"], 1200);

    // Ananya status is now queued
    let (status, status1) =
        api_request(&app, "GET", "/v1/matchmaking/status", Some(&token1), None).await;
    assert_eq!(status, 200);
    assert_eq!(status1["queued"], true);
    assert_eq!(status1["pool"], "blitz");

    // Vikram joins Blitz queue
    let (status, receipt2) = api_request(
        &app,
        "POST",
        "/v1/matchmaking/queue",
        Some(&token2),
        Some(json!({
            "tc": { "initial_s": 180, "increment_s": 2 },
            "rated": true
        })),
    )
    .await;
    assert_eq!(status, 202);
    assert_eq!(receipt2["pool"], "blitz");

    println!("--- 6. Pairing Sweep ---");
    // Trigger pairing sweep
    let matches = state
        .matchmaking
        .sweep_pool(Pool::Blitz, QueueMode::Rated)
        .await
        .expect("sweep");
    assert_eq!(matches.len(), 1, "expected 1 match to be formed");

    let game_id = &matches[0].game_id;
    println!("Match created! Game ID: {}", game_id);

    // Verify both players popped from queue
    let (_, s1_after) =
        api_request(&app, "GET", "/v1/matchmaking/status", Some(&token1), None).await;
    assert_eq!(s1_after["queued"], false);

    let (_, s2_after) =
        api_request(&app, "GET", "/v1/matchmaking/status", Some(&token2), None).await;
    assert_eq!(s2_after["queued"], false);

    // Verify game in database
    let game_row = state
        .matchmaking
        .game_service()
        .get_game_by_public_id(game_id)
        .await
        .expect("query game")
        .expect("game exists");
    assert_eq!(game_row.status, "starting");
    assert_eq!(game_row.pool, "blitz");
    assert_eq!(game_row.w_remaining_ms, Some(180_000));
    assert_eq!(game_row.b_remaining_ms, Some(180_000));
    assert_eq!(game_row.version, 0);

    println!("--- 7. Public Profile Lookup ---");
    // Vikram checks Ananya's public profile
    let (status, pub_profile) = api_request(
        &app,
        "GET",
        &format!("/v1/players/{handle1}"),
        Some(&token2),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(pub_profile["handle"], handle1);
    assert_eq!(pub_profile["display_name"], "Ananya Plays");
    assert_eq!(pub_profile["ratings"]["blitz"], 1200);
    assert!(pub_profile["phone"].is_null()); // PII protected

    println!("--- 8. DPDP Consent & Deletion Lifecycle ---");
    // Ananya updates settings to opt into marketing -> auditable consent record created
    let (status, patched) = api_request(
        &app,
        "PATCH",
        "/v1/me",
        Some(&token1),
        Some(json!({
            "settings": {
                "marketing_opt_in": true
            }
        })),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(patched["settings"]["marketing_opt_in"], true);

    // Verify consent record was recorded
    let consent_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM consent_records WHERE purpose = 'marketing' AND granted = true",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(consent_count >= 1);

    // Ananya requests deletion (30-day grace period)
    let (status, del_resp) = api_request(&app, "DELETE", "/v1/me", Some(&token1), None).await;
    assert_eq!(status, 200);
    assert_eq!(del_resp["status"], "deletion_scheduled");
    assert_eq!(del_resp["grace_period_days"], 30);

    // Verify profile shows deletion pending
    let (status, me_del) = api_request(&app, "GET", "/v1/me", Some(&token1), None).await;
    assert_eq!(status, 200);
    assert!(me_del["deletion_requested_at"].is_string());

    // Ananya cancels deletion
    let (status, _) =
        api_request(&app, "POST", "/v1/me/deletion-cancel", Some(&token1), None).await;
    assert_eq!(status, 204);

    // Profile no longer shows deletion pending
    let (status, me_restored) = api_request(&app, "GET", "/v1/me", Some(&token1), None).await;
    assert_eq!(status, 200);
    assert!(me_restored["deletion_requested_at"].is_null());

    println!("E2E test completed successfully!");
}
