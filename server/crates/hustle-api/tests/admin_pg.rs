//! L3 integration tests — Admin Console & Anomaly Review API endpoints (real PG).
//! Skips gracefully without CH_TEST_DATABASE_URL.

use http_body_util::BodyExt;
use hustle_api::{router, AppState};
use hustle_identity::signup::{Consents, ProfileInput, VerifiedIdentity};
use hustle_kernel::ServerConfig;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use tower::ServiceExt;

fn counter() -> u64 {
    static N: AtomicU64 = AtomicU64::new(70_000);
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
    format!("api_adm_{}", counter())
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

    // If role != "user", update user role and issue new access token with that role
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
async fn test_admin_endpoints_role_gating() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool);
    let app = router(state.clone());

    let (user_token, _, user_id) = make_user(&state, "user").await;

    // 1. Unauthenticated gets 401
    let (status, _) = api_request(&app, "GET", "/v1/admin/flags", None, None).await;
    assert_eq!(status, 401);

    // 2. Normal user gets 403 Forbidden
    let (status, err) = api_request(&app, "GET", "/v1/admin/flags", Some(&user_token), None).await;
    assert_eq!(status, 403);
    assert_eq!(err["error"]["code"], "auth.forbidden");

    let (status, _) = api_request(
        &app,
        "POST",
        &format!("/v1/admin/players/{user_id}/sanctions"),
        Some(&user_token),
        Some(json!({ "kind": "warning", "reason": "test" })),
    )
    .await;
    assert_eq!(status, 403);

    // Admin config gating
    let (status, _) = api_request(
        &app,
        "GET",
        "/v1/admin/config/economy.welcome_gold",
        None,
        None,
    )
    .await;
    assert_eq!(status, 401);

    let (status, err) = api_request(
        &app,
        "GET",
        "/v1/admin/config/economy.welcome_gold",
        Some(&user_token),
        None,
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(err["error"]["code"], "auth.forbidden");

    let (status, _) = api_request(
        &app,
        "PUT",
        "/v1/admin/config/economy.welcome_gold",
        Some(&user_token),
        Some(json!({ "value": 50000, "version": 1 })),
    )
    .await;
    assert_eq!(status, 403);
}

#[tokio::test]
async fn test_admin_anomaly_flags_lifecycle() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (admin_token, _, _) = make_user(&state, "admin").await;
    let (_, _, target_user_id) = make_user(&state, "user").await;

    // Seed an anomaly flag directly in PG
    let flag_row = sqlx::query(
        r#"
        INSERT INTO anomaly_flags (kind, subject_user_id, score, evidence, status)
        VALUES ('engine_match', $1, 0.94, '{"evaluated_moves": 20, "agreement_pct": 95}', 'open')
        RETURNING id
        "#,
    )
    .bind(target_user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let flag_id: i64 = sqlx::Row::get(&flag_row, "id");

    // 1. Query GET /v1/admin/flags
    let (status, flags) = api_request(
        &app,
        "GET",
        "/v1/admin/flags?status=open",
        Some(&admin_token),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert!(flags.as_array().unwrap().iter().any(|f| f["id"] == flag_id));

    // 2. Resolve flag POST /v1/admin/flags/{id}/resolve
    let (status, res) = api_request(
        &app,
        "POST",
        &format!("/v1/admin/flags/{flag_id}/resolve"),
        Some(&admin_token),
        Some(json!({
            "resolution": "confirmed",
            "notes": "Verified engine moves via reviewer inspection"
        })),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(res["status"], "confirmed");
    assert!(res["reviewed_by"].as_str().unwrap().starts_with("admin:"));
}

#[tokio::test]
async fn test_admin_sanctions_player_inspection_reports_and_audit() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (admin_token, _, _) = make_user(&state, "admin").await;
    let (user_token, target_handle, target_user_id) = make_user(&state, "user").await;
    let (_, reporter_handle, _) = make_user(&state, "user").await;

    // 1. Apply sanction via POST /v1/admin/players/{id}/sanctions
    let (status, sanction) = api_request(
        &app,
        "POST",
        &format!("/v1/admin/players/{target_handle}/sanctions"),
        Some(&admin_token),
        Some(json!({
            "kind": "warning",
            "reason": "Repeated stalling violation",
            "expires_at": null,
            "evidence": { "stalled_plies": 4 }
        })),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(sanction["user_id"], target_user_id);
    assert_eq!(sanction["kind"], "warning");

    // 2. Inspect player via GET /v1/admin/players/{id}
    let (status, player_detail) = api_request(
        &app,
        "GET",
        &format!("/v1/admin/players/{target_user_id}"),
        Some(&admin_token),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(player_detail["handle"], target_handle);
    assert_eq!(
        player_detail["active_sanctions"].as_array().unwrap().len(),
        1
    );

    // 3. Submit a user report from user_token
    let (status, rep_res) = api_request(
        &app,
        "POST",
        "/v1/reports",
        Some(&user_token),
        Some(json!({
            "category": "abuse",
            "target_handle": reporter_handle,
            "note": "Abusive chat messages"
        })),
    )
    .await;
    assert_eq!(status, 201);
    let report_id = rep_res["id"].as_i64().unwrap();

    // 4. Admin lists reports via GET /v1/admin/reports
    let (status, reports) = api_request(
        &app,
        "GET",
        "/v1/admin/reports?status=open",
        Some(&admin_token),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert!(reports
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == report_id));

    // 5. Admin actions report via POST /v1/admin/reports/{id}/action
    let (status, actioned_rep) = api_request(
        &app,
        "POST",
        &format!("/v1/admin/reports/{report_id}/action"),
        Some(&admin_token),
        Some(json!({
            "action": "actioned",
            "resolution": { "decision": "warning_issued" }
        })),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(actioned_rep["status"], "actioned");

    // 6. Admin inspects audit log via GET /v1/admin/audit
    let (status, audit_logs) = api_request(
        &app,
        "GET",
        "/v1/admin/audit?limit=20",
        Some(&admin_token),
        None,
    )
    .await;
    assert_eq!(status, 200);
    let audit_arr = audit_logs.as_array().unwrap();
    assert!(
        audit_arr.iter().any(|a| a["action"] == "create_sanction"),
        "Expected create_sanction in audit log"
    );
    assert!(
        audit_arr.iter().any(|a| a["action"] == "action_report"),
        "Expected action_report in audit log"
    );
}

#[tokio::test]
async fn test_admin_dynamic_config_optimistic_locking() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (admin_token, _, _) = make_user(&state, "admin").await;

    // 1. Read seeded config knob
    let (status, item) = api_request(
        &app,
        "GET",
        "/v1/admin/config/economy.welcome_gold",
        Some(&admin_token),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(item["key"], "economy.welcome_gold");
    let initial_version = item["version"].as_i64().unwrap() as i32;

    // 2. Optimistic update with matching version
    let (status, updated) = api_request(
        &app,
        "PUT",
        "/v1/admin/config/economy.welcome_gold",
        Some(&admin_token),
        Some(json!({
            "value": 60000,
            "version": initial_version
        })),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(updated["version"], initial_version + 1);
    assert_eq!(updated["value"], 60000);
    assert!(updated["updated_by"]
        .as_str()
        .unwrap()
        .starts_with("admin:"));

    // 3. Update with stale version triggers 409 Conflict
    let (status, conflict_err) = api_request(
        &app,
        "PUT",
        "/v1/admin/config/economy.welcome_gold",
        Some(&admin_token),
        Some(json!({
            "value": 70000,
            "version": initial_version // Stale!
        })),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(conflict_err["error"]["code"], "admin.version_conflict");

    // 4. Update with current version succeeds
    let (status, updated2) = api_request(
        &app,
        "PUT",
        "/v1/admin/config/economy.welcome_gold",
        Some(&admin_token),
        Some(json!({
            "value": 75000,
            "version": initial_version + 1
        })),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(updated2["version"], initial_version + 2);
    assert_eq!(updated2["value"], 75000);

    // 5. Query non-existent key returns 404
    let (status, not_found) = api_request(
        &app,
        "GET",
        "/v1/admin/config/non_existent.knob",
        Some(&admin_token),
        None,
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(not_found["error"]["code"], "admin.config_not_found");

    // 6. Create new key with version 0
    let (status, created) = api_request(
        &app,
        "PUT",
        "/v1/admin/config/feature.ai_hints",
        Some(&admin_token),
        Some(json!({
            "value": { "enabled": true, "max_per_game": 3 },
            "version": 0
        })),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(created["key"], "feature.ai_hints");
    assert_eq!(created["version"], 1);

    // 7. Verify audit log recorded update_config and create_config
    let (status, audit) = api_request(
        &app,
        "GET",
        "/v1/admin/audit?limit=20",
        Some(&admin_token),
        None,
    )
    .await;
    assert_eq!(status, 200);
    let audit_arr = audit.as_array().unwrap();
    assert!(
        audit_arr
            .iter()
            .any(|a| a["action"] == "update_config" && a["target_id"] == "economy.welcome_gold"),
        "Expected update_config in audit log"
    );
    assert!(
        audit_arr
            .iter()
            .any(|a| a["action"] == "create_config" && a["target_id"] == "feature.ai_hints"),
        "Expected create_config in audit log"
    );
}
