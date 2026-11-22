//! L3 integration tests — Friends, Blocks & Direct Challenge REST endpoints (real PG & Redis).
//! Skips gracefully without CH_TEST_DATABASE_URL.

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
    format!("fr_user_{}", counter())
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
async fn friends_endpoints_require_auth() {
    let app = router(AppState::test_stub());

    let (status, _) = api_request(&app, "GET", "/v1/friends", None, None).await;
    assert_eq!(status, 401);

    let (status, _) = api_request(&app, "GET", "/v1/friends/requests", None, None).await;
    assert_eq!(status, 401);

    let (status, _) = api_request(
        &app,
        "POST",
        "/v1/friends/requests",
        None,
        Some(json!({ "handle": "anyone" })),
    )
    .await;
    assert_eq!(status, 401);

    let (status, _) = api_request(&app, "GET", "/v1/blocks", None, None).await;
    assert_eq!(status, 401);

    let (status, _) = api_request(
        &app,
        "POST",
        "/v1/challenge",
        None,
        Some(json!({ "handle": "anyone", "tc": { "initial_s": 300, "increment_s": 0 } })),
    )
    .await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn friends_requests_and_lifecycle_pg() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (token_a, handle_a, _user_a) = make_authed_user(&state).await;
    let (token_b, handle_b, _user_b) = make_authed_user(&state).await;

    // 1. Initially friends list is empty
    let (status, friends_a) = api_request(&app, "GET", "/v1/friends", Some(&token_a), None).await;
    assert_eq!(status, 200);
    assert_eq!(friends_a.as_array().unwrap().len(), 0);

    // 2. User A cannot friend themselves
    let (status, _) = api_request(
        &app,
        "POST",
        "/v1/friends/requests",
        Some(&token_a),
        Some(json!({ "handle": handle_a })),
    )
    .await;
    assert_eq!(status, 400);

    // 3. User A requests User B
    let (status, req_res) = api_request(
        &app,
        "POST",
        "/v1/friends/requests",
        Some(&token_a),
        Some(json!({ "handle": handle_b })),
    )
    .await;
    assert_eq!(status, 201, "res: {req_res}");
    let req_id = req_res["id"].as_i64().expect("request id");

    // 4. Duplicate request returns conflict
    let (status, _) = api_request(
        &app,
        "POST",
        "/v1/friends/requests",
        Some(&token_a),
        Some(json!({ "handle": handle_b })),
    )
    .await;
    assert_eq!(status, 409);

    // 5. User A sees 1 outgoing request, 0 incoming
    let (status, reqs_a) =
        api_request(&app, "GET", "/v1/friends/requests", Some(&token_a), None).await;
    assert_eq!(status, 200);
    assert_eq!(reqs_a["outgoing"].as_array().unwrap().len(), 1);
    assert_eq!(reqs_a["incoming"].as_array().unwrap().len(), 0);

    // 6. User B sees 1 incoming request from User A
    let (status, reqs_b) =
        api_request(&app, "GET", "/v1/friends/requests", Some(&token_b), None).await;
    assert_eq!(status, 200);
    assert_eq!(reqs_b["incoming"].as_array().unwrap().len(), 1);
    assert_eq!(reqs_b["incoming"][0]["handle"], handle_a);

    // 7. User B accepts request
    let (status, accept_res) = api_request(
        &app,
        "POST",
        &format!("/v1/friends/requests/{req_id}/accept"),
        Some(&token_b),
        None,
    )
    .await;
    assert_eq!(status, 200, "res: {accept_res}");
    assert_eq!(accept_res["status"], "accepted");

    // 8. Both User A and User B now have 1 friend
    let (status, friends_a) = api_request(&app, "GET", "/v1/friends", Some(&token_a), None).await;
    assert_eq!(status, 200);
    assert_eq!(friends_a.as_array().unwrap().len(), 1);
    assert_eq!(friends_a[0]["handle"], handle_b);
    let friendship_id = friends_a[0]["id"].as_i64().unwrap();

    let (status, friends_b) = api_request(&app, "GET", "/v1/friends", Some(&token_b), None).await;
    assert_eq!(status, 200);
    assert_eq!(friends_b.as_array().unwrap().len(), 1);
    assert_eq!(friends_b[0]["handle"], handle_a);

    // 9. Unfriend
    let (status, _) = api_request(
        &app,
        "DELETE",
        &format!("/v1/friends/{friendship_id}"),
        Some(&token_a),
        None,
    )
    .await;
    assert_eq!(status, 204);

    // 10. Friends list is empty again
    let (status, friends_a) = api_request(&app, "GET", "/v1/friends", Some(&token_a), None).await;
    assert_eq!(status, 200);
    assert_eq!(friends_a.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn friends_blocks_lifecycle_pg() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (token_a, _handle_a, _user_a) = make_authed_user(&state).await;
    let (_token_b, handle_b, _user_b) = make_authed_user(&state).await;

    // 1. Initial blocks empty
    let (status, blocks) = api_request(&app, "GET", "/v1/blocks", Some(&token_a), None).await;
    assert_eq!(status, 200);
    assert_eq!(blocks.as_array().unwrap().len(), 0);

    // 2. Block user B
    let (status, block_res) = api_request(
        &app,
        "POST",
        "/v1/blocks",
        Some(&token_a),
        Some(json!({ "handle": handle_b })),
    )
    .await;
    assert_eq!(status, 201, "res: {block_res}");
    assert_eq!(block_res["handle"], handle_b);

    // 3. Blocked list contains user B
    let (status, blocks) = api_request(&app, "GET", "/v1/blocks", Some(&token_a), None).await;
    assert_eq!(status, 200);
    assert_eq!(blocks.as_array().unwrap().len(), 1);
    assert_eq!(blocks[0]["handle"], handle_b);

    // 4. Unblock user B
    let (status, _) = api_request(
        &app,
        "DELETE",
        &format!("/v1/blocks/{handle_b}"),
        Some(&token_a),
        None,
    )
    .await;
    assert_eq!(status, 204);

    // 5. Blocked list empty
    let (status, blocks) = api_request(&app, "GET", "/v1/blocks", Some(&token_a), None).await;
    assert_eq!(status, 200);
    assert_eq!(blocks.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn friends_challenge_lifecycle_pg() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (token_a, handle_a, _user_a) = make_authed_user(&state).await;
    let (token_b, handle_b, _user_b) = make_authed_user(&state).await;

    // 1. User A sends challenge to User B
    let (status, challenge) = api_request(
        &app,
        "POST",
        "/v1/challenge",
        Some(&token_a),
        Some(json!({
            "handle": handle_b,
            "tc": {
                "initial_s": 300,
                "increment_s": 0
            },
            "rated": false
        })),
    )
    .await;
    assert_eq!(status, 201, "challenge: {challenge}");
    let ch_id = challenge["id"].as_str().unwrap().to_string();
    assert_eq!(challenge["challenger_handle"], handle_a);
    assert_eq!(challenge["target_handle"], handle_b);

    // 2. User B declines first challenge
    let (status, decline_res) = api_request(
        &app,
        "POST",
        &format!("/v1/challenge/{ch_id}/decline"),
        Some(&token_b),
        None,
    )
    .await;
    assert_eq!(status, 200, "decline: {decline_res}");
    assert_eq!(decline_res["status"], "declined");

    // 3. User A sends second challenge to User B
    let (status, challenge2) = api_request(
        &app,
        "POST",
        "/v1/challenge",
        Some(&token_a),
        Some(json!({
            "handle": handle_b,
            "tc": {
                "initial_s": 180,
                "increment_s": 2
            },
            "rated": true
        })),
    )
    .await;
    assert_eq!(status, 201, "challenge2: {challenge2}");
    let ch_id2 = challenge2["id"].as_str().unwrap().to_string();

    // 4. User B accepts challenge
    let (status, accept_res) = api_request(
        &app,
        "POST",
        &format!("/v1/challenge/{ch_id2}/accept"),
        Some(&token_b),
        None,
    )
    .await;
    assert_eq!(status, 200, "accept: {accept_res}");
    let game_id = accept_res["game_id"].as_str().expect("game_id returned");
    assert!(!game_id.is_empty());

    // 5. Verify the game exists in database via GET /v1/games/{game_id}
    let (status, game_snapshot) = api_request(
        &app,
        "GET",
        &format!("/v1/games/{game_id}"),
        Some(&token_a),
        None,
    )
    .await;
    assert_eq!(status, 200, "game snapshot: {game_snapshot}");
    assert_eq!(game_snapshot["id"], game_id);
    assert_eq!(game_snapshot["status"], "starting");
}
