//! L3 integration tests — Cosmetics & Loadout endpoints (real PG).
//! Skips gracefully without CH_TEST_DATABASE_URL.

use http_body_util::BodyExt;
use hustle_api::{router, AppState};
use hustle_identity::signup::{Consents, ProfileInput, VerifiedIdentity};
use hustle_kernel::ServerConfig;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use tower::ServiceExt;

fn counter() -> u64 {
    static N: AtomicU64 = AtomicU64::new(60_000);
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
    format!("api_cosm_{}", counter())
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
async fn cosmetics_catalog_and_detail_pg() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool);
    let app = router(state.clone());

    // 1. Catalog is accessible without auth (public)
    let (status, _, catalog) = api_request(&app, "GET", "/v1/cosmetics", None, None, None).await;
    assert_eq!(status, 200);
    assert!(catalog.is_array());
    let items = catalog.as_array().unwrap();
    assert!(!items.is_empty());

    // 2. Filter by category
    let (status, _, boards) = api_request(
        &app,
        "GET",
        "/v1/cosmetics?category=board",
        None,
        None,
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert!(boards
        .as_array()
        .unwrap()
        .iter()
        .all(|i| i["category"] == "board"));

    // 3. Detail view
    let (status, _, item) = api_request(
        &app,
        "GET",
        "/v1/cosmetics/board_chaturanga",
        None,
        None,
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(item["id"], "board_chaturanga");
    assert_eq!(item["price_gold"], 30000);
}

#[tokio::test]
async fn cosmetics_purchase_and_loadout_lifecycle_pg() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (token, _handle, _user_id) = make_authed_user(&state).await;

    // 1. Unauthenticated loadout requires auth
    let (status, _, _) = api_request(&app, "GET", "/v1/me/loadout", None, None, None).await;
    assert_eq!(status, 401);

    // 2. Initial loadout is empty
    let (status, _, loadout) =
        api_request(&app, "GET", "/v1/me/loadout", Some(&token), None, None).await;
    assert_eq!(status, 200);
    assert!(loadout["board_id"].is_null());

    // 3. Equipping unowned cosmetic fails (403 Forbidden)
    let (status, _, err) = api_request(
        &app,
        "PUT",
        "/v1/me/loadout",
        Some(&token),
        Some(json!({ "board_id": "board_chaturanga" })),
        None,
    )
    .await;
    assert_eq!(status, 403, "expected 403, got: {err}");

    // 4. Purchase cosmetic with Idempotency-Key
    let idem_key = format!("test_idem_{}", counter());
    let (status, headers, purchase_res) = api_request(
        &app,
        "POST",
        "/v1/cosmetics/board_chaturanga/purchase",
        Some(&token),
        None,
        Some(vec![("Idempotency-Key", &idem_key)]),
    )
    .await;
    assert_eq!(status, 200, "purchase response: {purchase_res}");
    assert_eq!(purchase_res["owned"], true);
    assert_eq!(purchase_res["ledger"]["reason"], "cosmetic_purchase");
    assert_eq!(purchase_res["ledger"]["delta"], -30000);
    assert!(headers.get("Idempotent-Replay").is_none());

    // 5. Replaying purchase with same Idempotency-Key returns Idempotent-Replay header
    let (status, replay_headers, replay_res) = api_request(
        &app,
        "POST",
        "/v1/cosmetics/board_chaturanga/purchase",
        Some(&token),
        None,
        Some(vec![("Idempotency-Key", &idem_key)]),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(replay_res["owned"], true);
    assert_eq!(
        replay_headers
            .get("Idempotent-Replay")
            .and_then(|v| v.to_str().ok()),
        Some("true")
    );

    // 6. Successfully equip owned cosmetic
    let (status, _, updated_loadout) = api_request(
        &app,
        "PUT",
        "/v1/me/loadout",
        Some(&token),
        Some(json!({ "board_id": "board_chaturanga" })),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(updated_loadout["board_id"], "board_chaturanga");

    // 7. GET /v1/me/loadout returns equipped item
    let (status, _, loadout) =
        api_request(&app, "GET", "/v1/me/loadout", Some(&token), None, None).await;
    assert_eq!(status, 200);
    assert_eq!(loadout["board_id"], "board_chaturanga");
}
