//! L3 integration tests — Moderation, Grievance, and In-Game Chat endpoints (real PG & Redis).
//! Skips gracefully without CH_TEST_DATABASE_URL.

use http_body_util::BodyExt;
use hustle_api::{router, AppState};
use hustle_domain::{Pool, TimeControl};
use hustle_identity::signup::{Consents, ProfileInput, VerifiedIdentity};
use hustle_kernel::ServerConfig;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use tower::ServiceExt;

fn counter() -> u64 {
    static N: AtomicU64 = AtomicU64::new(50_000);
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
    format!("api_mod_{}", counter())
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
async fn moderation_endpoints_auth_check() {
    let app = router(AppState::test_stub());

    // Grievance is public (200 OK without auth)
    let (status, body) = api_request(&app, "GET", "/v1/grievance", None, None).await;
    assert_eq!(status, 200, "grievance info: {body}");
    assert_eq!(body["officer_name"], "Priya Sharma");

    // Reports require auth
    let (status, _) = api_request(
        &app,
        "POST",
        "/v1/reports",
        None,
        Some(json!({ "category": "abuse", "target_handle": "nobody" })),
    )
    .await;
    assert_eq!(status, 401);

    // Chat requires auth
    let (status, _) = api_request(&app, "GET", "/v1/games/some_game/chat", None, None).await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn moderation_and_chat_lifecycle_pg() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config.clone(), pool.clone());
    let app = router(state.clone());

    let (token_a, handle_a, user_a) = make_authed_user(&state).await;
    let (token_b, handle_b, user_b) = make_authed_user(&state).await;

    // Reset Redis report/chat rate limits for test user
    let redis_client = redis::Client::open(config.redis_url.as_str()).unwrap();
    if let Ok(mut c) = redis_client.get_multiplexed_async_connection().await {
        let _: Result<(), _> = redis::cmd("DEL")
            .arg(format!("report:rate:{user_a}"))
            .query_async(&mut c)
            .await;
    }

    // 1. Create a game between User A and User B
    let tc = TimeControl {
        initial_ms: 180_000,
        increment_ms: 2_000,
    };
    let game = state
        .game_service
        .create_game(user_a, user_b, Pool::Blitz, tc, true)
        .await
        .unwrap();

    // 2. User A sends chat message in game
    let (status, msg_res) = api_request(
        &app,
        "POST",
        &format!("/v1/games/{}/chat", game.public_id),
        Some(&token_a),
        Some(json!({ "text": "Good luck, have fun!" })),
    )
    .await;
    assert_eq!(status, 201, "res: {msg_res}");
    assert_eq!(msg_res["handle"], handle_a);
    assert_eq!(msg_res["body"], "Good luck, have fun!");
    assert_eq!(msg_res["masked"], false);

    // 3. User B retrieves chat history
    let (status, history) = api_request(
        &app,
        "GET",
        &format!("/v1/games/{}/chat", game.public_id),
        Some(&token_b),
        None,
    )
    .await;
    assert_eq!(status, 200, "history: {history}");
    assert_eq!(history.as_array().unwrap().len(), 1);
    assert_eq!(history[0]["body"], "Good luck, have fun!");

    // 4. User A submits report against User B
    let (status, rep_res) = api_request(
        &app,
        "POST",
        "/v1/reports",
        Some(&token_a),
        Some(json!({
            "category": "cheating",
            "target_handle": handle_b,
            "game_id": game.public_id,
            "note": "Suspected external engine usage."
        })),
    )
    .await;
    assert_eq!(status, 201, "report: {rep_res}");
    assert_eq!(rep_res["category"], "cheating");
    assert_eq!(rep_res["target_handle"], handle_b);
    assert_eq!(rep_res["status"], "open");
}
