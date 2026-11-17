//! L3 integration tests — T1 signup atomicity + refresh rotation (real PG).
//! Skips gracefully without CH_TEST_DATABASE_URL.

use hustle_identity::sessions::SessionStore;
use hustle_identity::signup::{
    Consents, ProfileInput, SignupError, SignupService, VerifiedIdentity,
};
use hustle_identity::tokens::TokenService;
use hustle_identity::{SessionError, SsoUser};
use std::sync::atomic::{AtomicU64, Ordering};

fn counter() -> u64 {
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

fn phone() -> String {
    format!("+9198{:010}", counter())
}

fn handle() -> String {
    format!("hustler_{}", counter())
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

fn svc(pool: sqlx::PgPool) -> SignupService {
    SignupService::new(pool, TokenService::new(b"test-secret-16bytes!!"), 50_000)
}

#[tokio::test]
async fn t1_signup_grants_welcome_atomically() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let (p, h) = (phone(), handle());
    let outcome = svc(pool.clone())
        .signup(
            VerifiedIdentity::Phone(p.clone()),
            consents(),
            ProfileInput {
                handle: h.clone(),
                display_name: "Test Player".into(),
            },
            Some("dev-web"),
            Some("aabbcc"),
        )
        .await
        .unwrap();

    assert!(outcome.is_new_user);
    assert_eq!(outcome.gold_balance, 50_000);
    assert_eq!(outcome.handle, h);

    // Access token validates and binds user + session.
    let claims = TokenService::new(b"test-secret-16bytes!!")
        .validate_access(&outcome.access_token)
        .unwrap();
    assert_eq!(claims.sub, outcome.user_id.to_string());
    assert_eq!(claims.sid, outcome.session_id.to_string());

    // T1 contents, one transaction's worth:
    // ledger row + balance
    let (ledger_rows, balance): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM gold_ledger WHERE user_id = $1),
                (SELECT balance FROM gold_balances WHERE user_id = $1)",
    )
    .bind(outcome.user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ledger_rows, 1);
    assert_eq!(balance, 50_000);
    let reason: String = sqlx::query_scalar("SELECT reason FROM gold_ledger WHERE user_id = $1")
        .bind(outcome.user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(reason, "welcome");

    // consents + settings + identity + active session
    let (consent_rows, settings, identities, active_sessions): (i64, i64, i64, i64) =
        sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM consent_records WHERE user_id = $1),
                (SELECT COUNT(*) FROM user_settings WHERE user_id = $1),
                (SELECT COUNT(*) FROM auth_identities WHERE user_id = $1 AND kind = 'phone'),
                (SELECT COUNT(*) FROM user_sessions WHERE user_id = $1 AND revoked_at IS NULL)",
        )
        .bind(outcome.user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(consent_rows, 5);
    assert_eq!(settings, 1);
    assert_eq!(identities, 1);
    assert_eq!(active_sessions, 1);
}

#[tokio::test]
async fn re_signup_dedupes_welcome_and_keeps_profile() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let p = phone();
    let h1 = handle();
    let svc = svc(pool.clone());
    let first = svc
        .signup(
            VerifiedIdentity::Phone(p.clone()),
            consents(),
            ProfileInput {
                handle: h1.clone(),
                display_name: "First Name".into(),
            },
            None,
            None,
        )
        .await
        .unwrap();

    // Second verification of the same identity: user reused, welcome idem-deduped.
    let h2 = handle();
    let second = svc
        .signup(
            VerifiedIdentity::Phone(p.clone()),
            consents(),
            ProfileInput {
                handle: h2.clone(),
                display_name: "Second Name".into(),
            },
            None,
            None,
        )
        .await
        .unwrap();

    assert!(!second.is_new_user);
    assert_eq!(second.gold_balance, 50_000, "no double welcome");

    // Exactly one user, one ledger row, profile keeps the FIRST handle.
    let (users, ledger_rows): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM users WHERE phone_e164 = $1),
                (SELECT COUNT(*) FROM gold_ledger WHERE user_id = $2)",
    )
    .bind(&p)
    .bind(first.user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(users, 1);
    assert_eq!(ledger_rows, 1);
    let stored: String = sqlx::query_scalar("SELECT handle FROM profiles WHERE user_id = $1")
        .bind(first.user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, h1);
}

#[tokio::test]
async fn handle_conflict_rejects_and_rolls_back() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let h = handle();
    let svc = svc(pool.clone());
    let winner = svc
        .signup(
            VerifiedIdentity::Phone(phone()),
            consents(),
            ProfileInput {
                handle: h.clone(),
                display_name: "Winner".into(),
            },
            None,
            None,
        )
        .await
        .unwrap();

    // A different phone trying the same handle → HandleTaken; its whole T1
    // (user row included) must roll back.
    let loser_phone = phone();
    let err = svc
        .signup(
            VerifiedIdentity::Phone(loser_phone.clone()),
            consents(),
            ProfileInput {
                handle: h.clone(),
                display_name: "Loser".into(),
            },
            None,
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, SignupError::HandleTaken));

    let loser_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE phone_e164 = $1")
        .bind(&loser_phone)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(loser_rows, 0, "failed signup must roll back entirely");
    let still_one: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM profiles WHERE lower(handle) = lower($1)")
            .bind(&h)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(still_one, 1);
    assert_eq!(winner.handle, h);
}

#[tokio::test]
async fn consent_gate_blocks_without_essential() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let p = phone();
    let err = svc(pool.clone())
        .signup(
            VerifiedIdentity::Phone(p.clone()),
            Consents {
                essential: false,
                ..consents()
            },
            ProfileInput {
                handle: handle(),
                display_name: "No Consent".into(),
            },
            None,
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, SignupError::ConsentMissing));
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE phone_e164 = $1")
        .bind(&p)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn google_signup_creates_email_user() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let sso = SsoUser {
        sub: format!("google-sub-{}", counter()),
        email: format!("gg{}@example.com", counter()),
    };
    let svc = svc(pool.clone());
    let first = svc
        .signup(
            VerifiedIdentity::Google(sso.clone()),
            consents(),
            ProfileInput {
                handle: handle(),
                display_name: "Google User".into(),
            },
            None,
            None,
        )
        .await
        .unwrap();
    assert!(first.is_new_user);
    assert_eq!(first.gold_balance, 50_000);

    // Same Google sub again → same user, deduped welcome.
    let again = svc
        .signup(
            VerifiedIdentity::Google(sso),
            consents(),
            ProfileInput {
                handle: handle(),
                display_name: "Google User".into(),
            },
            None,
            None,
        )
        .await
        .unwrap();
    assert!(!again.is_new_user);
    assert_eq!(again.user_id, first.user_id);
    let ledger_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM gold_ledger WHERE user_id = $1")
            .bind(first.user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(ledger_rows, 1);
}

#[tokio::test]
async fn refresh_rotation_reuse_kills_family() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let outcome = svc(pool.clone())
        .signup(
            VerifiedIdentity::Phone(phone()),
            consents(),
            ProfileInput {
                handle: handle(),
                display_name: "Rotator".into(),
            },
            None,
            None,
        )
        .await
        .unwrap();
    let store = SessionStore::new(pool.clone());

    // Normal rotation: new refresh token, old marked 'rotated'.
    let refresh1 = outcome.refresh_token.clone();
    let (issued2, old_row) = store.rotate(&refresh1).await.unwrap();
    assert_ne!(issued2.refresh.raw, refresh1);
    assert_eq!(old_row.user_id, outcome.user_id);
    assert_eq!(old_row.session_id, outcome.session_id);

    // Fresh access token binds the NEW session.
    let tokens = TokenService::new(b"test-secret-16bytes!!");
    let claims = tokens
        .validate_access(
            &tokens
                .issue_access(outcome.user_id, "user", issued2.session_id)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(claims.sid, issued2.session_id.to_string());

    // REUSE of the rotated token → family revoked.
    let err = store.rotate(&refresh1).await.unwrap_err();
    assert!(matches!(err, SessionError::Reused));

    // Even the previously-valid sibling is now dead.
    let err = store.rotate(&issued2.refresh.raw).await.unwrap_err();
    assert!(matches!(err, SessionError::Reused));

    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_sessions
         WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(outcome.user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active, 0, "reuse must revoke the entire family");

    let reasons: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT revoked_reason FROM user_sessions
         WHERE user_id = $1 AND revoked_at IS NOT NULL",
    )
    .bind(outcome.user_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(reasons.contains(&"rotation_reuse".to_string()));
}

#[tokio::test]
async fn logout_all_revokes_every_session() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let svc = svc(pool.clone());
    let cons = consents();
    let prof = || ProfileInput {
        handle: handle(),
        display_name: "All Out".into(),
    };
    // Two families: two signups of the same identity → two active sessions.
    let first = svc
        .signup(
            VerifiedIdentity::Phone(phone()),
            cons.clone(),
            prof(),
            None,
            None,
        )
        .await
        .unwrap();
    let second = svc
        .signup(
            VerifiedIdentity::Phone(
                sqlx::query_scalar::<_, String>("SELECT phone_e164 FROM users WHERE id = $1")
                    .bind(first.user_id)
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
            ),
            cons,
            prof(),
            None,
            None,
        )
        .await
        .unwrap();

    let store = SessionStore::new(pool.clone());
    let revoked = store.revoke_all(first.user_id, "logout_all").await.unwrap();
    assert_eq!(revoked, 2, "both families must go");
    assert_eq!(second.user_id, first.user_id);
    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_sessions WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(first.user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active, 0);
}
