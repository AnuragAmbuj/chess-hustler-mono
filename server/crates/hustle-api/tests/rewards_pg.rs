//! L3 integration tests — Rewards & Daily Streak endpoints (real PG).
//! Skips gracefully without CH_TEST_DATABASE_URL.

use http_body_util::BodyExt;
use hustle_api::{router, AppState};
use hustle_gold::{
    daily_streak::{claim_daily_streak, ist_offset},
    GoldLedger, GoldReason, LedgerOp, PgGoldLedger,
};
use hustle_identity::signup::{Consents, ProfileInput, VerifiedIdentity};
use hustle_kernel::ServerConfig;
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
    format!("reward_user_{}", counter())
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
async fn daily_endpoints_require_auth() {
    let app = router(AppState::test_stub());

    let (status, body) = api_request(&app, "GET", "/v1/me/daily", None, None).await;
    assert_eq!(status, 401);
    assert_eq!(body["error"]["code"], "auth.token_expired");

    let (status, body) = api_request(&app, "POST", "/v1/me/daily/claim", None, None).await;
    assert_eq!(status, 401);
    assert_eq!(body["error"]["code"], "auth.token_expired");

    let (status, body) = api_request(&app, "GET", "/v1/me/gold/summary", None, None).await;
    assert_eq!(status, 401);
    assert_eq!(body["error"]["code"], "auth.token_expired");
}

#[tokio::test]
async fn daily_claim_and_summary_lifecycle_pg() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    let (token, _handle, user_id) = make_authed_user(&state).await;

    // 1. Initial daily status before any claims
    let (status, body) = api_request(&app, "GET", "/v1/me/daily", Some(&token), None).await;
    assert_eq!(status, 200, "body: {body}");
    assert_eq!(body["available"], true);
    assert_eq!(body["streak"], 0);
    assert_eq!(body["next_reward"], 500);
    assert!(body["next_reset_ist"].as_str().unwrap().contains("+05:30"));

    // 2. Initial gold summary reflects welcome grant (50,000)
    let (status, body) = api_request(&app, "GET", "/v1/me/gold/summary", Some(&token), None).await;
    assert_eq!(status, 200, "body: {body}");
    assert_eq!(body["balance"], 50_000);
    assert_eq!(body["total_earned"], 50_000);
    assert_eq!(body["total_spent"], 0);
    let breakdown = body["recent_30d_breakdown"].as_array().unwrap();
    assert_eq!(breakdown.len(), 1);
    assert_eq!(breakdown[0]["reason"], "welcome");
    assert_eq!(breakdown[0]["amount"], 50_000);

    // 3. First claim on Day 1
    let (status, body) = api_request(&app, "POST", "/v1/me/daily/claim", Some(&token), None).await;
    assert_eq!(status, 200, "body: {body}");
    assert_eq!(body["claimed"], true);
    assert_eq!(body["amount"], 500);
    assert_eq!(body["streak"], 1);
    assert_eq!(body["new_balance"], 50_500);

    // Verify row in daily_claims table has unique constraint on (user_id, claim_date_ist)
    let claim_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM daily_claims WHERE user_id = $1 AND claim_date_ist IS NOT NULL",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(claim_count, 1);

    // 4. Daily status immediately after claim: available is now false, streak is 1, next reward is 750
    let (status, body) = api_request(&app, "GET", "/v1/me/daily", Some(&token), None).await;
    assert_eq!(status, 200, "body: {body}");
    assert_eq!(body["available"], false);
    assert_eq!(body["streak"], 1);
    assert_eq!(body["next_reward"], 750);

    // 5. Repeat claim on same day is idempotent: returns claimed=false, amount=0, new_balance unchanged
    let (status, body) = api_request(&app, "POST", "/v1/me/daily/claim", Some(&token), None).await;
    assert_eq!(status, 200, "body: {body}");
    assert_eq!(body["claimed"], false);
    assert_eq!(body["amount"], 0);
    assert_eq!(body["streak"], 1);
    assert_eq!(body["new_balance"], 50_500);

    // 6. Gold summary reflects the newly credited daily streak
    let (status, body) = api_request(&app, "GET", "/v1/me/gold/summary", Some(&token), None).await;
    assert_eq!(status, 200, "body: {body}");
    assert_eq!(body["balance"], 50_500);
    assert_eq!(body["total_earned"], 50_500);
    assert_eq!(body["total_spent"], 0);
    let breakdown = body["recent_30d_breakdown"].as_array().unwrap();
    assert_eq!(breakdown.len(), 2);
    let daily_entry = breakdown
        .iter()
        .find(|b| b["reason"] == "daily_streak")
        .expect("daily_streak in breakdown");
    assert_eq!(daily_entry["amount"], 500);
    assert_eq!(daily_entry["count"], 1);
}

#[tokio::test]
async fn daily_streak_progression_and_wrapping_pg() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());

    let (_token, _handle, user_id) = make_authed_user(&state).await;

    // Simulate 8 consecutive days of claims
    // Day 1: 500
    // Day 2: 750
    // Day 3: 1000
    // Day 4: 1500
    // Day 5: 2000
    // Day 6: 3000
    // Day 7: 5000
    // Day 8: wraps to Day 1 (500)
    let expected_rewards = [500, 750, 1000, 1500, 2000, 3000, 5000, 500];
    let expected_streaks = [1, 2, 3, 4, 5, 6, 7, 1];

    let base_date = time::Date::from_calendar_date(2026, time::Month::January, 1).unwrap();
    let ist = ist_offset();

    for day_idx in 0..8 {
        let date = base_date
            .checked_add(time::Duration::days(day_idx as i64))
            .unwrap();
        let simulated_now = date.with_hms(12, 0, 0).unwrap().assume_offset(ist);

        let res = claim_daily_streak(&pool, user_id, simulated_now)
            .await
            .expect("claim succeeds");

        assert!(res.claimed, "day {day_idx} must claim successfully");
        assert_eq!(
            res.streak, expected_streaks[day_idx],
            "day {day_idx} streak mismatch"
        );
        assert_eq!(
            res.amount, expected_rewards[day_idx],
            "day {day_idx} reward mismatch"
        );
    }

    // Verify all 8 days were recorded in daily_claims with claim_date_ist populated
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM daily_claims WHERE user_id = $1 AND claim_date_ist IS NOT NULL",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 8);
}

#[tokio::test]
async fn daily_streak_resets_on_missed_day_pg() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());

    let (_token, _handle, user_id) = make_authed_user(&state).await;

    let base_date = time::Date::from_calendar_date(2026, time::Month::February, 1).unwrap();
    let ist = ist_offset();

    // Day 1: claim
    let now1 = base_date.with_hms(10, 0, 0).unwrap().assume_offset(ist);
    let res1 = claim_daily_streak(&pool, user_id, now1)
        .await
        .expect("day 1 claim");
    assert!(res1.claimed);
    assert_eq!(res1.streak, 1);
    assert_eq!(res1.amount, 500);

    // Day 2: claim
    let now2 = base_date
        .next_day()
        .unwrap()
        .with_hms(10, 0, 0)
        .unwrap()
        .assume_offset(ist);
    let res2 = claim_daily_streak(&pool, user_id, now2)
        .await
        .expect("day 2 claim");
    assert!(res2.claimed);
    assert_eq!(res2.streak, 2);
    assert_eq!(res2.amount, 750);

    // Skip Day 3, jump to Day 4 (missed day resets streak!)
    let day4 = base_date.checked_add(time::Duration::days(3)).unwrap();
    let now4 = day4.with_hms(10, 0, 0).unwrap().assume_offset(ist);

    let status = hustle_gold::daily_streak::get_daily_status(&pool, user_id, now4)
        .await
        .expect("status check");
    assert!(status.available);
    assert_eq!(status.streak, 0, "streak must reset to 0 after missed day");
    assert_eq!(status.next_reward, 500, "next reward resets to day 1");

    let res4 = claim_daily_streak(&pool, user_id, now4)
        .await
        .expect("day 4 claim");
    assert!(res4.claimed);
    assert_eq!(res4.streak, 1, "streak restarts at 1");
    assert_eq!(res4.amount, 500);
}

#[tokio::test]
async fn gold_summary_breakdown_tracks_debits_and_credits_pg() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let ledger = PgGoldLedger::new(pool.clone());

    let (_token, _handle, user_id) = make_authed_user(&state).await;

    // Initially 50,000 from welcome
    // Spend 10,000 on cosmetic purchase
    ledger
        .grant(LedgerOp {
            user_id,
            delta: -10_000,
            reason: GoldReason::CosmeticPurchase,
            ref_type: Some("cosmetic".to_string()),
            ref_id: Some("board_ipl_night".to_string()),
            idem_key: Some(format!("purchase:{user_id}:1")),
            meta: serde_json::json!({}),
        })
        .await
        .expect("spend succeeds");

    let summary = ledger.summary(user_id).await.expect("summary succeeds");
    assert_eq!(summary.balance, 40_000);
    assert_eq!(summary.total_earned, 50_000);
    assert_eq!(summary.total_spent, 10_000);

    let cosmetic_item = summary
        .recent_30d_breakdown
        .iter()
        .find(|i| i.reason == "cosmetic_purchase")
        .expect("cosmetic in breakdown");
    assert_eq!(cosmetic_item.amount, -10_000);
    assert_eq!(cosmetic_item.count, 1);
}

#[tokio::test]
async fn missions_and_achievements_api_endpoints_pg() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let config = ServerConfig::from_env();
    let state = AppState::build(config, pool.clone());
    let app = router(state.clone());

    // 1. Unauthenticated endpoints return 401
    let (status, _) = api_request(&app, "GET", "/v1/me/missions", None, None).await;
    assert_eq!(status, 401);

    let (status, _) = api_request(&app, "GET", "/v1/me/achievements", None, None).await;
    assert_eq!(status, 401);

    let (status, _) = api_request(
        &app,
        "POST",
        "/v1/me/missions/daily_games_1/claim",
        None,
        None,
    )
    .await;
    assert_eq!(status, 401);

    // 2. Authenticated user retrieves missions
    let (token, _handle, user_id) = make_authed_user(&state).await;

    let (status, missions_body) =
        api_request(&app, "GET", "/v1/me/missions", Some(&token), None).await;
    assert_eq!(status, 200);
    assert!(missions_body.is_array());
    let list = missions_body.as_array().unwrap();
    assert!(!list.is_empty());
    assert!(list.iter().any(|m| m["id"] == "daily_games_1"));

    // 3. Authenticated user retrieves achievements
    let (status, achs_body) =
        api_request(&app, "GET", "/v1/me/achievements", Some(&token), None).await;
    assert_eq!(status, 200);
    assert!(achs_body.is_array());
    let ach_list = achs_body.as_array().unwrap();
    assert!(!ach_list.is_empty());
    assert!(ach_list.iter().any(|a| a["id"] == "achieve_first_win"));

    // 4. Incomplete mission claim fails (400)
    let (status, _) = api_request(
        &app,
        "POST",
        "/v1/me/missions/daily_games_1/claim",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, 400);

    // 5. Simulate progress completion on daily_games_1
    sqlx::query("UPDATE user_missions SET progress = goal WHERE user_id = $1 AND mission_id = 'daily_games_1'")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    // 6. Claim succeeds (200)
    let (status, claim_body) = api_request(
        &app,
        "POST",
        "/v1/me/missions/daily_games_1/claim",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(claim_body["claimed"], true);
    assert_eq!(claim_body["amount"], 250);
    assert_eq!(claim_body["new_balance"], 50_250);

    // 7. Replaying claim returns 409 Conflict
    let (status, _) = api_request(
        &app,
        "POST",
        "/v1/me/missions/daily_games_1/claim",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, 409);
}
