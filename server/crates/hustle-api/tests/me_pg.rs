//! L3 integration tests — Me & Profile routes (real PG).
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
    format!("me_user_{}", counter())
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
async fn me_requires_auth() {
    let app = router(AppState::test_stub());

    let (status, body) = api_request(&app, "GET", "/v1/me", None, None).await;
    assert_eq!(status, 401);
    assert_eq!(body["error"]["code"], "auth.token_expired");
}

#[tokio::test]
async fn me_lifecycle_pg() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };

    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (token, user_handle, _user_id) = make_authed_user(&state).await;

    // 1. GET /v1/me
    let (status, me) = api_request(&app, "GET", "/v1/me", Some(&token), None).await;
    assert_eq!(status, 200, "get_me response: {me:?}");
    assert_eq!(me["handle"], user_handle);
    assert_eq!(me["gold"], 50000);
    assert_eq!(me["ratings"]["blitz"], 1200);
    assert_eq!(me["settings"]["chat_enabled"], true);
    assert_eq!(me["settings"]["marketing_opt_in"], false);
    assert!(me["deletion_requested_at"].is_null());
    let public_id = me["id"].as_str().unwrap().to_string();

    // 2. PATCH /v1/me (update display_name, bio, settings)
    let (status, patched) = api_request(
        &app,
        "PATCH",
        "/v1/me",
        Some(&token),
        Some(json!({
            "display_name": "Grandmaster Test",
            "bio": "Hustling 24/7",
            "settings": {
                "chat_enabled": false,
                "marketing_opt_in": true,
                "quiet_hours_start": 23,
                "quiet_hours_end": 6
            }
        })),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(patched["display_name"], "Grandmaster Test");
    assert_eq!(patched["bio"], "Hustling 24/7");
    assert_eq!(patched["settings"]["chat_enabled"], false);
    assert_eq!(patched["settings"]["marketing_opt_in"], true);
    assert_eq!(patched["settings"]["quiet_hours_start"], 23);
    assert_eq!(patched["settings"]["quiet_hours_end"], 6);

    // 3. PATCH /v1/me validation errors
    let (status, err) = api_request(
        &app,
        "PATCH",
        "/v1/me",
        Some(&token),
        Some(json!({
            "display_name": "x" // too short (< 2)
        })),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(err["error"]["code"], "validation.failed");

    // 4. GET /v1/me/gold (ledger page)
    let (status, gold_page) = api_request(&app, "GET", "/v1/me/gold", Some(&token), None).await;
    assert_eq!(status, 200);
    assert_eq!(gold_page["balance"], 50000);
    let items = gold_page["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["reason"], "welcome");
    assert_eq!(items[0]["delta"], 50000);

    // 5. GET /v1/players/{handle} (public view)
    let (status, player) = api_request(
        &app,
        "GET",
        &format!("/v1/players/{user_handle}"),
        None,
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(player["id"], public_id);
    assert_eq!(player["handle"], user_handle);
    assert_eq!(player["display_name"], "Grandmaster Test");
    assert_eq!(player["bio"], "Hustling 24/7");
    assert_eq!(player["ratings"]["rapid"], 1200);
    assert!(player.get("phone").is_none());
    assert!(player.get("settings").is_none());

    // 6. Unknown player -> 404
    let (status, err404) = api_request(
        &app,
        "GET",
        "/v1/players/nonexistent_player_xyz",
        None,
        None,
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(err404["error"]["code"], "player.not_found");

    // 7. DELETE /v1/me (DPDP erasure request)
    let (status, del) = api_request(&app, "DELETE", "/v1/me", Some(&token), None).await;
    assert_eq!(status, 200);
    assert_eq!(del["status"], "deletion_scheduled");
    assert_eq!(del["grace_period_days"], 30);

    let (_, me_del) = api_request(&app, "GET", "/v1/me", Some(&token), None).await;
    assert!(me_del["deletion_requested_at"].is_string());

    // 8. POST /v1/me/deletion-cancel
    let (status, _) = api_request(&app, "POST", "/v1/me/deletion-cancel", Some(&token), None).await;
    assert_eq!(status, 204);

    let (_, me_restored) = api_request(&app, "GET", "/v1/me", Some(&token), None).await;
    assert!(me_restored["deletion_requested_at"].is_null());
}

#[tokio::test]
async fn test_player_games_feed_pagination() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skipping test_player_games_feed_pagination: no DB");
        return;
    };

    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (_token_a, handle_a, user_a) = make_authed_user(&state).await;
    let (_token_b, handle_b, user_b) = make_authed_user(&state).await;

    // Create 3 completed games
    for _ in 0..3 {
        let g = state
            .game_service
            .create_game(
                user_a,
                user_b,
                hustle_domain::Pool::Blitz,
                hustle_domain::TimeControl {
                    initial_ms: 180_000,
                    increment_ms: 2_000,
                },
                true,
            )
            .await
            .unwrap();

        state.game_service.start_game(&g.public_id).await.unwrap();
        state
            .game_service
            .resign_game(&g.public_id, user_b)
            .await
            .unwrap();
    }

    // 1. Fetch first page with limit=2
    let (status, page1) = api_request(
        &app,
        "GET",
        &format!("/v1/players/{}/games?limit=2", handle_a),
        None,
        None,
    )
    .await;
    assert_eq!(status, 200);
    let items = page1["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["white"]["handle"], handle_a);
    assert_eq!(items[0]["black"]["handle"], handle_b);
    assert_eq!(items[0]["result"], "white");
    assert_eq!(items[0]["termination"], "resignation");
    assert!(page1["next_cursor"].is_number());
    let next_cursor = page1["next_cursor"].as_i64().unwrap();

    // 2. Fetch second page with cursor
    let (status, page2) = api_request(
        &app,
        "GET",
        &format!(
            "/v1/players/{}/games?limit=2&cursor={}",
            handle_a, next_cursor
        ),
        None,
        None,
    )
    .await;
    assert_eq!(status, 200);
    let items2 = page2["items"].as_array().unwrap();
    assert_eq!(items2.len(), 1);
    assert!(page2["next_cursor"].is_null());

    // 3. Unknown player -> 404
    let (status, err404) = api_request(
        &app,
        "GET",
        "/v1/players/unknown_player_nonexistent_xyz/games",
        None,
        None,
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(err404["error"]["code"], "player.not_found");
}

#[tokio::test]
async fn test_dpdp_export_me_bundle() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skipping test_dpdp_export_me_bundle: no DB");
        return;
    };

    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (token, handle, _user_id) = make_authed_user(&state).await;

    // 1. Unauthenticated request -> 401
    let (status, _) = api_request(&app, "GET", "/v1/me/export", None, None).await;
    assert_eq!(status, 401);

    // 2. Authenticated request -> 200 with full DPDP bundle
    let (status, body) = api_request(&app, "GET", "/v1/me/export", Some(&token), None).await;
    assert_eq!(status, 200);
    assert_eq!(body["status"], "ready");
    assert!(body["job_id"].as_str().unwrap().starts_with("export_"));
    assert!(body["exported_at"].is_string());

    let bundle = &body["bundle"];
    assert_eq!(bundle["profile"]["handle"], handle);
    assert_eq!(bundle["gold"]["balance"], 50000); // welcome bonus

    let consents = bundle["consents"].as_array().unwrap();
    assert!(!consents.is_empty());
    assert!(consents
        .iter()
        .any(|c| c["type"] == "essential" && c["granted"] == true));

    let ratings = &bundle["ratings"];
    assert_eq!(ratings["blitz"], 1200);
}
