//! L3 integration tests — Tournaments v1 API endpoints (real PG).
//! Skips gracefully without CH_TEST_DATABASE_URL.

use http_body_util::BodyExt;
use hustle_api::{router, AppState};
use hustle_identity::signup::{Consents, ProfileInput, VerifiedIdentity};
use hustle_kernel::ServerConfig;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use tower::ServiceExt;

fn counter() -> u64 {
    static N: AtomicU64 = AtomicU64::new(90_000);
    N.fetch_add(1, Ordering::Relaxed)
        + (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            % 10_000_000)
}

fn phone() -> String {
    format!("+9198{:010}", counter())
}

fn handle() -> String {
    format!("api_tn_{}", counter())
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

async fn make_user(state: &AppState, role: &str) -> (String, String, i64) {
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

    let token = if role != "user" {
        sqlx::query("UPDATE users SET role = $1 WHERE id = $2")
            .bind(role)
            .bind(outcome.user_id)
            .execute(&state.pool)
            .await
            .unwrap();

        state
            .tokens
            .issue_access(outcome.user_id, role, 1001)
            .unwrap()
    } else {
        outcome.access_token
    };

    (token, h, outcome.user_id)
}

async fn api_request(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<serde_json::Value>,
    extra_headers: Option<Vec<(&str, &str)>>,
) -> (u16, axum::http::HeaderMap, serde_json::Value) {
    let mut builder = axum::http::Request::builder().method(method).uri(uri);

    if let Some(t) = token {
        builder = builder.header(axum::http::header::AUTHORIZATION, format!("Bearer {t}"));
    }
    if body.is_some() {
        builder = builder.header(axum::http::header::CONTENT_TYPE, "application/json");
    }
    if let Some(headers) = extra_headers {
        for (k, v) in headers {
            builder = builder.header(k, v);
        }
    }

    let payload = match body {
        Some(b) => axum::body::Body::from(serde_json::to_vec(&b).unwrap()),
        None => axum::body::Body::empty(),
    };

    let req = builder.body(payload).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status().as_u16();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, headers, json)
}

#[tokio::test]
async fn tournaments_list_public_returns_200() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool);
    let app = router(state);

    let (status, _, body) = api_request(&app, "GET", "/v1/tournaments", None, None, None).await;
    assert_eq!(status, 200);
    assert!(body.is_array());
}

#[tokio::test]
async fn tournament_detail_not_found_is_404() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool);
    let app = router(state);

    let (status, _, body) = api_request(
        &app,
        "GET",
        "/v1/tournaments/tn_nonexistent_12345",
        None,
        None,
        None,
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "tn.not_found");
}

#[tokio::test]
async fn tournament_create_non_admin_is_403() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool);
    let app = router(state.clone());

    let (token, _, _) = make_user(&state, "user").await;

    let payload = json!({
        "name": "Unauthorized Open",
        "tc": { "initial_s": 180, "increment_s": 2 },
        "rounds": 5,
        "round_minutes": 15,
        "capacity": 32,
        "starts_at": "2026-12-01T10:00:00Z",
        "registration_closes_at": "2026-12-01T09:50:00Z"
    });

    let (status, _, _) = api_request(
        &app,
        "POST",
        "/v1/admin/tournaments",
        Some(&token),
        Some(payload),
        None,
    )
    .await;

    assert_eq!(status, 403);
}

#[tokio::test]
async fn tournament_create_and_join_lifecycle() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool);
    let app = router(state.clone());

    let (admin_token, _, _) = make_user(&state, "admin").await;
    let (player_token, _, _) = make_user(&state, "user").await;

    let payload = json!({
        "name": "Delhi Blitz Masters",
        "tc": { "initial_s": 180, "increment_s": 2 },
        "rounds": 5,
        "round_minutes": 15,
        "capacity": 16,
        "starts_at": "2026-12-01T12:00:00Z",
        "registration_closes_at": "2026-12-01T11:55:00Z",
        "prize_config": {
            "pool_gold": 1000
        }
    });

    // 1. Admin creates tournament
    let (status, _, body) = api_request(
        &app,
        "POST",
        "/v1/admin/tournaments",
        Some(&admin_token),
        Some(payload),
        None,
    )
    .await;

    assert_eq!(status, 201);
    let public_id = body["public_id"].as_str().unwrap().to_string();
    assert_eq!(body["name"], "Delhi Blitz Masters");
    assert_eq!(body["status"], "registration");

    // 2. Fetch tournament detail
    let (status, _, detail) = api_request(
        &app,
        "GET",
        &format!("/v1/tournaments/{public_id}"),
        Some(&player_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(detail["tournament"]["public_id"], public_id);
    assert_eq!(detail["entries_count"], 0);

    // 3. Player joins tournament
    let (status, _, entry) = api_request(
        &app,
        "POST",
        &format!("/v1/tournaments/{public_id}/join"),
        Some(&player_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(entry["withdrawn"], false);

    // 4. Detail shows entry count 1
    let (status, _, detail2) = api_request(
        &app,
        "GET",
        &format!("/v1/tournaments/{public_id}"),
        Some(&player_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(detail2["entries_count"], 1);

    // 5. Player withdraws from tournament
    let (status, _, _) = api_request(
        &app,
        "POST",
        &format!("/v1/tournaments/{public_id}/withdraw"),
        Some(&player_token),
        None,
        None,
    )
    .await;
    assert_eq!(status, 204);
}
