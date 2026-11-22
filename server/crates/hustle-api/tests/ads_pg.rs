//! L3 integration tests — Rewarded Ads endpoints (real PG).
//! Skips gracefully without CH_TEST_DATABASE_URL.

use http_body_util::BodyExt;
use hustle_api::{router, AppState};
use hustle_identity::signup::{Consents, ProfileInput, VerifiedIdentity};
use hustle_kernel::ServerConfig;
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
    format!("api_ads_{}", counter())
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
async fn ads_status_unauthenticated_is_401() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool);
    let app = router(state);

    let (status, _, _) = api_request(&app, "GET", "/v1/ads/status", None, None, None).await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn ads_status_authenticated_returns_eligibility() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool);
    let app = router(state.clone());

    let (token, _, _) = make_user(&state).await;

    let (status, _, body) = api_request(
        &app,
        "GET",
        "/v1/ads/status",
        Some(&token),
        None,
        None,
    )
    .await;

    assert_eq!(status, 200);
    assert_eq!(body["enabled"], true);
    assert_eq!(body["max_per_day"], 5);
    assert_eq!(body["gold_per_ad"], 100);
    assert!(body["eligibility"].is_object());
}

#[tokio::test]
async fn ads_ssv_callback_unsupported_provider_is_400() {
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
        "/v1/ads/ssv/unsupported_network/callback?foo=bar",
        None,
        None,
        None,
    )
    .await;

    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "ads.unsupported_provider");
}

#[tokio::test]
async fn ads_ssv_callback_missing_or_invalid_signature_is_400() {
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
        "/v1/ads/ssv/admob/callback?ad_unit=123&custom_data=as_01HX&timestamp=1700000000000&key_id=1234&signature=invalidsig",
        None,
        None,
        None,
    )
    .await;

    assert_eq!(status, 400);
    // Either unknown key id or invalid signature
    assert!(
        body["error"]["code"] == "ads.unknown_key_id"
            || body["error"]["code"] == "ads.invalid_signature"
            || body["error"]["code"] == "ads.expired_timestamp"
    );
}
