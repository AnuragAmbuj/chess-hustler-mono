//! Security & Authorization Matrix Audit (Threat Model §4, Testing Strategy §6)
//!
//! Verifies that all protected API routes strictly enforce authentication barriers (401),
//! reject tampered/invalid tokens, reject unauthorized operations, and return structured error codes.

use http_body_util::BodyExt;
use hustle_api::{router, AppState};
use tower::ServiceExt;

fn app() -> axum::Router {
    router(AppState::test_stub())
}

async fn request_status_and_body(
    method: axum::http::Method,
    uri: &str,
    token: Option<&str>,
    body_content: Option<&'static str>,
) -> (u16, String) {
    let mut builder = axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header("Content-Type", "application/json");

    if let Some(tok) = token {
        builder = builder.header("Authorization", format!("Bearer {tok}"));
    }

    let body = match body_content {
        Some(s) => axum::body::Body::from(s),
        None => axum::body::Body::empty(),
    };

    let res = app()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();

    let status = res.status().as_u16();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn unauthenticated_access_to_me_returns_401() {
    let (status, body) = request_status_and_body(axum::http::Method::GET, "/v1/me", None, None).await;
    assert_eq!(status, 401, "Expected 401 Unauthorized for /v1/me, got {status}");
    assert!(body.contains("auth."), "Body should contain auth error code: {body}");
}

#[tokio::test]
async fn unauthenticated_access_to_gold_returns_401() {
    let (status, body) = request_status_and_body(axum::http::Method::GET, "/v1/me/gold", None, None).await;
    assert_eq!(status, 401, "Expected 401 Unauthorized for /v1/me/gold, got {status}");
    assert!(body.contains("auth."), "Body should contain auth error code: {body}");
}

#[tokio::test]
async fn unauthenticated_access_to_daily_claim_returns_401() {
    let (status, body) = request_status_and_body(axum::http::Method::POST, "/v1/me/daily/claim", None, None).await;
    assert_eq!(status, 401, "Expected 401 for unauthenticated daily claim, got {status}");
    assert!(body.contains("auth."), "Body: {body}");
}

#[tokio::test]
async fn unauthenticated_access_to_matchmaking_returns_401() {
    let (status, body) = request_status_and_body(
        axum::http::Method::POST,
        "/v1/matchmaking/queue",
        None,
        Some(r#"{"initial_s":180,"increment_s":2,"rated":true}"#),
    )
    .await;
    assert_eq!(status, 401, "Expected 401 for queue join without token, got {status}");
    assert!(body.contains("auth."), "Body: {body}");
}

#[tokio::test]
async fn unauthenticated_access_to_admin_endpoints_returns_401() {
    let (status, body) = request_status_and_body(axum::http::Method::GET, "/v1/admin/reports", None, None).await;
    assert_eq!(status, 401, "Expected 401 for unauthenticated admin access, got {status}");
    assert!(body.contains("auth."), "Body: {body}");
}

#[tokio::test]
async fn malformed_bearer_token_returns_401() {
    let (status, body) = request_status_and_body(
        axum::http::Method::GET,
        "/v1/me",
        Some("garbage_not_a_jwt"),
        None,
    )
    .await;
    assert_eq!(status, 401, "Expected 401 for garbage token, got {status}");
    assert!(body.contains("auth.token_expired") || body.contains("auth."), "Body: {body}");
}
