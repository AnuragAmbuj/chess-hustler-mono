//! L3 integration tests — REST auth endpoints through Axum router (real PG).
//! Skips gracefully without CH_TEST_DATABASE_URL.

use async_trait::async_trait;
use http_body_util::BodyExt;
use hustle_api::{router, AppState};
use hustle_identity::signup::Consents;
use hustle_identity::{OtpError, OtpSender, OtpService};
use hustle_kernel::ServerConfig;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

struct CapturingOtpSender(Arc<Mutex<Option<String>>>);

#[async_trait]
impl OtpSender for CapturingOtpSender {
    async fn send(&self, _phone: &str, code: &str) -> Result<(), OtpError> {
        *self.0.lock().unwrap() = Some(code.to_string());
        Ok(())
    }
}

fn counter() -> u64 {
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

fn phone() -> String {
    format!("+9198{:010}", counter())
}

fn handle() -> String {
    format!("auth_user_{}", counter())
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

async fn json_post(
    app: &axum::Router,
    uri: &str,
    auth_header: Option<&str>,
    body: serde_json::Value,
) -> (u16, serde_json::Value) {
    let mut builder = axum::http::Request::builder()
        .method("POST")
        .uri(uri)
        .header(axum::http::header::CONTENT_TYPE, "application/json");

    if let Some(token) = auth_header {
        builder = builder.header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"));
    }

    let req = builder
        .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status().as_u16();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn auth_validation_errors() {
    let app = router(AppState::test_stub());

    // 1. otp_verify without age confirmation -> 403 auth.age_gate
    let (status, body) = json_post(
        &app,
        "/v1/auth/otp/verify",
        None,
        json!({
            "phone": "+919876543210",
            "code": "123456",
            "age_confirmed": false,
            "consents": consents(),
            "profile": { "handle": "test_user" }
        }),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "auth.age_gate");

    // 2. otp_verify missing consents -> 400 validation.failed
    let (status, body) = json_post(
        &app,
        "/v1/auth/otp/verify",
        None,
        json!({
            "phone": "+919876543210",
            "code": "123456",
            "age_confirmed": true,
            "profile": { "handle": "test_user" }
        }),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "validation.failed");

    // 3. invalid code -> 401 auth.otp_invalid
    let (status, body) = json_post(
        &app,
        "/v1/auth/otp/verify",
        None,
        json!({
            "phone": "+919876543210",
            "code": "000000",
            "age_confirmed": true,
            "consents": consents(),
            "profile": { "handle": "test_user" }
        }),
    )
    .await;
    assert_eq!(status, 401);
    assert_eq!(body["error"]["code"], "auth.otp_invalid");
}

#[tokio::test]
async fn auth_flow_end_to_end_pg() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };

    let config = ServerConfig::from_env();
    let mut state = AppState::build(config, pool);
    let captured_code = Arc::new(Mutex::new(None));
    state.otp = Arc::new(OtpService::new(Box::new(CapturingOtpSender(
        captured_code.clone(),
    ))));
    let app = router(state);

    let user_phone = phone();
    let user_handle = handle();

    // 1. Send OTP
    let (status, _) = json_post(
        &app,
        "/v1/auth/otp/send",
        None,
        json!({ "phone": user_phone }),
    )
    .await;
    assert_eq!(status, 204);

    let code = captured_code
        .lock()
        .unwrap()
        .clone()
        .expect("code was captured");

    // 2. Verify OTP -> issue tokens + user payload
    let (status, body) = json_post(
        &app,
        "/v1/auth/otp/verify",
        None,
        json!({
            "phone": user_phone,
            "code": code,
            "age_confirmed": true,
            "consents": consents(),
            "profile": {
                "handle": user_handle,
                "display_name": "Test Player"
            }
        }),
    )
    .await;
    assert_eq!(status, 200, "verify body: {body:?}");

    let access_token = body["access_token"].as_str().unwrap().to_string();
    let refresh_token = body["refresh_token"].as_str().unwrap().to_string();
    assert_eq!(body["expires_in"], 900);
    assert_eq!(body["user"]["handle"], user_handle);
    assert_eq!(body["user"]["display_name"], "Test Player");
    assert_eq!(body["user"]["gold"], 50000);
    assert_eq!(body["user"]["ratings"]["blitz"], 1200);
    assert!(body["user"]["id"].as_str().unwrap().starts_with('0'));

    // 3. Refresh rotation
    let (status, refresh_body) = json_post(
        &app,
        "/v1/auth/refresh",
        None,
        json!({ "refresh_token": refresh_token }),
    )
    .await;
    assert_eq!(status, 200);
    let new_access = refresh_body["access_token"].as_str().unwrap().to_string();
    assert_ne!(new_access, access_token);

    // 4. Logout current session
    let (status, _) = json_post(&app, "/v1/auth/logout", Some(&new_access), json!({})).await;
    assert_eq!(status, 204);

    // 5. Logout all sessions
    let (status, _) = json_post(&app, "/v1/auth/logout-all", Some(&new_access), json!({})).await;
    assert_eq!(status, 204);
}
