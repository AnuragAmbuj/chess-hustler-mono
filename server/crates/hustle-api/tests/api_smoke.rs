//! HTTP smoke tests for the Phase 0 surface (real HTTP through the router).

use http_body_util::BodyExt;
use hustle_api::{router, AppState};
use hustle_kernel::ServerConfig;
use tower::ServiceExt;

fn app() -> axum::Router {
    router(AppState {
        config: ServerConfig::from_env(),
    })
}

async fn get_body(uri: &str) -> (u16, String) {
    let res = app()
        .oneshot(
            axum::http::Request::builder()
                .uri(uri)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status().as_u16();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn healthz_is_ok() {
    let (status, body) = get_body("/healthz").await;
    assert_eq!(status, 200);
    assert_eq!(body, r#"{"status":"ok"}"#);
}

#[tokio::test]
async fn readyz_is_ok() {
    let (status, body) = get_body("/readyz").await;
    assert_eq!(status, 200);
    assert_eq!(body, r#"{"status":"ready"}"#);
}

#[tokio::test]
async fn meta_config_reflects_economy_defaults() {
    let (status, body) = get_body("/v1/meta/config").await;
    assert_eq!(status, 200);
    assert!(body.contains(r#""welcome_gold":50000"#), "body: {body}");
    assert!(body.contains(r#""gold_per_ad":2000"#), "body: {body}");
    assert!(body.contains(r#""max_per_day":5"#), "body: {body}");
    // 180+0 preset must map to the blitz pool (lifecycle §3).
    assert!(body.contains(r#""initial_s":180"#), "body: {body}");
    assert!(body.contains(r#""pool":"blitz""#), "body: {body}");
}

#[tokio::test]
async fn unknown_path_is_404() {
    let (status, _) = get_body("/v1/nope").await;
    assert_eq!(status, 404);
}
