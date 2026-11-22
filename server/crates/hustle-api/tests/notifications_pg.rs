//! L3 integration tests — Notifications subsystem endpoints (real PG).
//! Skips gracefully without CH_TEST_DATABASE_URL.

use http_body_util::BodyExt;
use hustle_api::{router, AppState};
use hustle_identity::signup::{Consents, ProfileInput, VerifiedIdentity};
use hustle_kernel::ServerConfig;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use tower::ServiceExt;

fn counter() -> u64 {
    static N: AtomicU64 = AtomicU64::new(80_000);
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
    format!("api_notif_{}", counter())
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

async fn make_user(state: &AppState) -> (String, String, i64) {
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
async fn test_notifications_auth_gating() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool);
    let app = router(state);

    // 1. GET /v1/notifications requires auth
    let (status, _) = api_request(&app, "GET", "/v1/notifications", None, None).await;
    assert_eq!(status, 401);

    // 2. POST /v1/notifications/read requires auth
    let (status, _) = api_request(
        &app,
        "POST",
        "/v1/notifications/read",
        None,
        Some(json!({ "all": true })),
    )
    .await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn test_notifications_feed_pagination_and_unread_filter() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (user_token, _, user_id) = make_user(&state).await;

    // Seed 5 notifications directly in PG
    let kinds = [
        "friend_request",
        "game_invite",
        "system",
        "rating_update",
        "achievement",
    ];
    let mut ids = Vec::new();
    for (i, kind) in kinds.iter().enumerate() {
        let read_clause = if i == 2 { "now()" } else { "NULL" };
        let query = format!(
            r#"
            INSERT INTO notifications (user_id, kind, payload, read_at)
            VALUES ($1, $2, $3, {read_clause})
            RETURNING id
            "#
        );
        let row = sqlx::query(&query)
            .bind(user_id)
            .bind(kind)
            .bind(json!({ "index": i, "details": format!("Payload for {kind}") }))
            .fetch_one(&pool)
            .await
            .unwrap();
        let nid: i64 = sqlx::Row::get(&row, "id");
        ids.push(nid);
    }

    // 1. Fetch first page with limit=2
    let (status, page1) = api_request(
        &app,
        "GET",
        "/v1/notifications?limit=2",
        Some(&user_token),
        None,
    )
    .await;
    assert_eq!(status, 200);
    let items1 = page1["items"].as_array().unwrap();
    assert_eq!(items1.len(), 2);
    assert_eq!(items1[0]["id"], ids[4]); // Newest first
    assert_eq!(items1[1]["id"], ids[3]);
    let next_cursor1 = page1["next_cursor"].as_i64();
    assert_eq!(next_cursor1, Some(ids[3]));

    // 2. Fetch second page with cursor
    let (status, page2) = api_request(
        &app,
        "GET",
        &format!("/v1/notifications?limit=2&cursor={}", next_cursor1.unwrap()),
        Some(&user_token),
        None,
    )
    .await;
    assert_eq!(status, 200);
    let items2 = page2["items"].as_array().unwrap();
    assert_eq!(items2.len(), 2);
    assert_eq!(items2[0]["id"], ids[2]);
    assert_eq!(items2[1]["id"], ids[1]);
    let next_cursor2 = page2["next_cursor"].as_i64();
    assert_eq!(next_cursor2, Some(ids[1]));

    // 3. Fetch third page
    let (status, page3) = api_request(
        &app,
        "GET",
        &format!("/v1/notifications?limit=2&cursor={}", next_cursor2.unwrap()),
        Some(&user_token),
        None,
    )
    .await;
    assert_eq!(status, 200);
    let items3 = page3["items"].as_array().unwrap();
    assert_eq!(items3.len(), 1);
    assert_eq!(items3[0]["id"], ids[0]);
    assert!(page3["next_cursor"].is_null());

    // 4. Test unread_only filter: should exclude notification #2 (index 2)
    let (status, unread_page) = api_request(
        &app,
        "GET",
        "/v1/notifications?unread_only=true",
        Some(&user_token),
        None,
    )
    .await;
    assert_eq!(status, 200);
    let unread_items = unread_page["items"].as_array().unwrap();
    assert_eq!(unread_items.len(), 4);
    assert!(!unread_items.iter().any(|item| item["id"] == ids[2]));
    for item in unread_items {
        assert!(item["read_at"].is_null());
    }
}

#[tokio::test]
async fn test_notifications_mark_read_ids_and_all() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (user_token, _, user_id) = make_user(&state).await;

    // Seed 3 unread notifications
    let mut ids = Vec::new();
    for i in 1..=3 {
        let row = sqlx::query(
            "INSERT INTO notifications (user_id, kind, payload) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(user_id)
        .bind(format!("event_{i}"))
        .bind(json!({ "step": i }))
        .fetch_one(&pool)
        .await
        .unwrap();
        let nid: i64 = sqlx::Row::get(&row, "id");
        ids.push(nid);
    }

    // 1. Mark first two as read by ID
    let (status, res1) = api_request(
        &app,
        "POST",
        "/v1/notifications/read",
        Some(&user_token),
        Some(json!({
            "ids": [ids[0], ids[1]]
        })),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(res1["status"], "ok");
    assert_eq!(res1["updated"], 2);

    // 2. Marking them again updates 0 rows
    let (status, res2) = api_request(
        &app,
        "POST",
        "/v1/notifications/read",
        Some(&user_token),
        Some(json!({
            "ids": [ids[0], ids[1]]
        })),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(res2["updated"], 0);

    // 3. One unread remains
    let (status, unread_feed) = api_request(
        &app,
        "GET",
        "/v1/notifications?unread_only=true",
        Some(&user_token),
        None,
    )
    .await;
    assert_eq!(status, 200);
    let items = unread_feed["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], ids[2]);

    // 4. Mark all as read
    let (status, res3) = api_request(
        &app,
        "POST",
        "/v1/notifications/read",
        Some(&user_token),
        Some(json!({
            "all": true
        })),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(res3["status"], "ok");
    assert_eq!(res3["updated"], 1);

    // 5. Unread feed is now empty
    let (status, empty_unread) = api_request(
        &app,
        "GET",
        "/v1/notifications?unread_only=true",
        Some(&user_token),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(empty_unread["items"].as_array().unwrap().len(), 0);

    // 6. Validation error when neither `ids` nor `all: true` is supplied
    let (status, val_err) = api_request(
        &app,
        "POST",
        "/v1/notifications/read",
        Some(&user_token),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(val_err["error"]["code"], "validation.failed");
}

#[tokio::test]
async fn test_notifications_user_isolation() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (token_a, _, user_a_id) = make_user(&state).await;
    let (token_b, _, _user_b_id) = make_user(&state).await;

    // Seed notification for User A
    let row = sqlx::query(
        "INSERT INTO notifications (user_id, kind, payload) VALUES ($1, 'secret', '{\"msg\": \"for A only\"}') RETURNING id"
    )
    .bind(user_a_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let notif_a_id: i64 = sqlx::Row::get(&row, "id");

    // User B reads their feed — User A's notification must NOT be visible
    let (status, feed_b) =
        api_request(&app, "GET", "/v1/notifications", Some(&token_b), None).await;
    assert_eq!(status, 200);
    assert_eq!(feed_b["items"].as_array().unwrap().len(), 0);

    // User B tries to mark User A's notification as read — must affect 0 rows
    let (status, res) = api_request(
        &app,
        "POST",
        "/v1/notifications/read",
        Some(&token_b),
        Some(json!({ "ids": [notif_a_id] })),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(res["updated"], 0);

    // User A checks feed — notification is still unread
    let (status, feed_a) = api_request(
        &app,
        "GET",
        "/v1/notifications?unread_only=true",
        Some(&token_a),
        None,
    )
    .await;
    assert_eq!(status, 200);
    let items_a = feed_a["items"].as_array().unwrap();
    assert_eq!(items_a.len(), 1);
    assert_eq!(items_a[0]["id"], notif_a_id);
    assert!(items_a[0]["read_at"].is_null());
}
