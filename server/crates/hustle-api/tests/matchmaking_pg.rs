//! L3 integration tests — Matchmaking routes (real PG + Redis).
//! Skips gracefully without CH_TEST_DATABASE_URL or CH_TEST_REDIS_URL.

use http_body_util::BodyExt;
use hustle_api::{router, AppState};
use hustle_identity::signup::{Consents, ProfileInput, VerifiedIdentity};
use hustle_kernel::ServerConfig;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use tower::ServiceExt;

fn counter() -> u64 {
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

fn phone() -> String {
    format!("+9198{:010}", counter())
}

fn handle() -> String {
    format!("mm_user_{}", counter())
}

fn consents() -> Consents {
    Consents {
        essential: true,
        functional: true,
        marketing: false,
        personalised_ads: false,
        chat: true,
    }
}

async fn make_authed_user(state: &AppState) -> (String, String, i64) {
    let p = phone();
    let h = handle();
    let outcome = state
        .signup
        .signup(
            VerifiedIdentity::Phone(p),
            consents(),
            ProfileInput {
                handle: h.clone(),
                display_name: format!("Player {h}"),
            },
            None,
            None,
        )
        .await
        .expect("signup failed");

    (outcome.access_token, h, outcome.user_id)
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
async fn matchmaking_requires_auth() {
    let app = router(AppState::test_stub());

    let (status, _) = api_request(&app, "GET", "/v1/matchmaking/status", None, None).await;
    assert_eq!(status, 401);

    let (status, _) = api_request(
        &app,
        "POST",
        "/v1/matchmaking/queue",
        None,
        Some(json!({
            "tc": { "initial_s": 180, "increment_s": 2 },
            "rated": true
        })),
    )
    .await;
    assert_eq!(status, 401);

    let (status, _) = api_request(&app, "DELETE", "/v1/matchmaking/queue", None, None).await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn matchmaking_flow_pg() {
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
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (token, _handle, _user_id) = make_authed_user(&state).await;

    // 1. Initial status: not queued
    let (status, body) =
        api_request(&app, "GET", "/v1/matchmaking/status", Some(&token), None).await;
    assert_eq!(status, 200);
    assert_eq!(body["queued"], false);

    // 2. Validation rejection on invalid initial_s
    let (status, body) = api_request(
        &app,
        "POST",
        "/v1/matchmaking/queue",
        Some(&token),
        Some(json!({
            "tc": { "initial_s": 2, "increment_s": 0 },
            "rated": true
        })),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "validation.failed");

    // 3. Enqueue into blitz
    let (status, body) = api_request(
        &app,
        "POST",
        "/v1/matchmaking/queue",
        Some(&token),
        Some(json!({
            "tc": { "initial_s": 180, "increment_s": 2 },
            "rated": true
        })),
    )
    .await;
    assert_eq!(status, 202);
    assert_eq!(body["pool"], "blitz");
    assert!(body["est_wait_s"].is_number());
    assert_eq!(body["ratings"]["blitz"], 1200);

    // 4. Status is now queued in blitz
    let (status, body) =
        api_request(&app, "GET", "/v1/matchmaking/status", Some(&token), None).await;
    assert_eq!(status, 200);
    assert_eq!(body["queued"], true);
    assert_eq!(body["pool"], "blitz");

    // 5. Leave queue
    let (status, _) =
        api_request(&app, "DELETE", "/v1/matchmaking/queue", Some(&token), None).await;
    assert_eq!(status, 204);

    // 6. Status is back to not queued
    let (status, body) =
        api_request(&app, "GET", "/v1/matchmaking/status", Some(&token), None).await;
    assert_eq!(status, 200);
    assert_eq!(body["queued"], false);
}
