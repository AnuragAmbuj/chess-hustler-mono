use hustle_chess::GameService;
use hustle_domain::{Pool, TimeControl};
use hustle_identity::signup::{Consents, ProfileInput, SignupService, VerifiedIdentity};
use hustle_identity::TokenService;
use hustle_moderation::{CreateReportRequest, ModerationError, ModerationService};
use hustle_realtime::EventPublisher;
use std::sync::atomic::{AtomicU64, Ordering};

fn counter() -> u64 {
    static N: AtomicU64 = AtomicU64::new(10_000);
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
    format!("mod_user_{}", counter())
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

async fn make_user(pool: &sqlx::PgPool) -> (i64, String, String) {
    let token_service = TokenService::new(b"test_secret_key_32_bytes_long_ok!");
    let signup = SignupService::new(pool.clone(), token_service, 50_000);
    let p = phone();
    let h = handle();
    let outcome = signup
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

    let public_id: String = sqlx::query_scalar("SELECT public_id FROM users WHERE id = $1")
        .bind(outcome.user_id)
        .fetch_one(pool)
        .await
        .unwrap();

    (outcome.user_id, h, public_id)
}

#[tokio::test]
async fn test_chat_message_flow_and_profanity_masking() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let redis_url = std::env::var("CH_REDIS_URL")
        .unwrap_or_else(|_| "redis+unix:///tmp/opencode/redis.sock".into());
    let redis = redis::Client::open(redis_url.as_str()).unwrap();
    let publisher = EventPublisher::new(redis.clone());
    let mod_svc = ModerationService::new(pool.clone(), redis.clone(), publisher);
    let game_svc = GameService::new(pool.clone());

    let (u1_id, u1_handle, _u1_pid) = make_user(&pool).await;
    let (u2_id, _u2_handle, _u2_pid) = make_user(&pool).await;
    let (u3_id, _u3_handle, _u3_pid) = make_user(&pool).await;

    let tc = TimeControl {
        initial_ms: 180_000,
        increment_ms: 2_000,
    };
    let game = game_svc
        .create_game(u1_id, u2_id, Pool::Blitz, tc, true)
        .await
        .unwrap();

    // 1. Clean message from Player A
    let msg1 = mod_svc
        .send_chat(u1_id, &game.public_id, "Good luck and have fun!")
        .await
        .unwrap();
    assert_eq!(msg1.handle, u1_handle);
    assert_eq!(msg1.body, "Good luck and have fun!");
    assert!(!msg1.masked);

    // 2. Message from third-party (Player C) is rejected
    let err = mod_svc
        .send_chat(u3_id, &game.public_id, "Hey spectators!")
        .await
        .unwrap_err();
    assert!(matches!(err, ModerationError::NotInGame));

    // 3. Message too long (> 200 chars) is rejected
    let long_msg = "a".repeat(201);
    let err_long = mod_svc
        .send_chat(u2_id, &game.public_id, &long_msg)
        .await
        .unwrap_err();
    assert!(matches!(err_long, ModerationError::MessageTooLong));

    // 4. Message with profanity is masked
    let msg2 = mod_svc
        .send_chat(u2_id, &game.public_id, "you bastard play faster")
        .await
        .unwrap();
    assert!(msg2.masked);
    assert!(msg2.body.contains("you ******* play faster"));

    // 5. Chat history contains both messages
    let history = mod_svc
        .get_game_chat(u1_id, &game.public_id, 50)
        .await
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].body, "Good luck and have fun!");
    assert_eq!(history[1].body, "you ******* play faster");
}

#[tokio::test]
async fn test_rate_limiting_chat() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let redis_url = std::env::var("CH_REDIS_URL")
        .unwrap_or_else(|_| "redis+unix:///tmp/opencode/redis.sock".into());
    let redis = redis::Client::open(redis_url.as_str()).unwrap();
    let publisher = EventPublisher::new(redis.clone());
    let mod_svc = ModerationService::new(pool.clone(), redis.clone(), publisher);
    let game_svc = GameService::new(pool.clone());

    let (u1_id, _u1_h, _u1_pid) = make_user(&pool).await;
    let (u2_id, _u2_h, _u2_pid) = make_user(&pool).await;

    let mut conn = redis.get_multiplexed_async_connection().await.unwrap();
    let _: Result<(), _> = redis::cmd("DEL")
        .arg(format!("sanction:mute:{u1_id}"))
        .query_async(&mut conn)
        .await;

    let tc = TimeControl {
        initial_ms: 180_000,
        increment_ms: 2_000,
    };
    let game = game_svc
        .create_game(u1_id, u2_id, Pool::Blitz, tc, true)
        .await
        .unwrap();

    // 1st message succeeds
    let _ = mod_svc
        .send_chat(u1_id, &game.public_id, "Hi!")
        .await
        .unwrap();

    // Immediate 2nd message is rate limited
    let err = mod_svc
        .send_chat(u1_id, &game.public_id, "Hi again!")
        .await
        .unwrap_err();
    assert!(matches!(err, ModerationError::RateLimited(_)));
}

#[tokio::test]
async fn test_submit_report_and_rate_limit() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let redis_url = std::env::var("CH_REDIS_URL")
        .unwrap_or_else(|_| "redis+unix:///tmp/opencode/redis.sock".into());
    let redis = redis::Client::open(redis_url.as_str()).unwrap();
    let publisher = EventPublisher::new(redis.clone());
    let mod_svc = ModerationService::new(pool.clone(), redis.clone(), publisher);

    let (u1_id, _u1_h, _u1_pid) = make_user(&pool).await;
    let (_u2_id, u2_handle, _u2_pid) = make_user(&pool).await;

    let mut conn = redis.get_multiplexed_async_connection().await.unwrap();
    let _: Result<(), _> = redis::cmd("DEL")
        .arg(format!("report:rate:{u1_id}"))
        .query_async(&mut conn)
        .await;

    // 1. Submit valid report
    let rep = mod_svc
        .submit_report(
            u1_id,
            CreateReportRequest {
                category: "abuse".into(),
                target_handle: u2_handle.clone(),
                game_id: None,
                note: Some("offensive conduct".into()),
            },
        )
        .await
        .unwrap();

    assert_eq!(rep.category, "abuse");
    assert_eq!(rep.target_handle, u2_handle);
    assert_eq!(rep.status, "open");

    // 2. Submit reports up to rate limit (5 per 24 hours)
    for _ in 0..4 {
        mod_svc
            .submit_report(
                u1_id,
                CreateReportRequest {
                    category: "other".into(),
                    target_handle: u2_handle.clone(),
                    game_id: None,
                    note: None,
                },
            )
            .await
            .unwrap();
    }

    // 6th report exceeds daily rate limit
    let err = mod_svc
        .submit_report(
            u1_id,
            CreateReportRequest {
                category: "other".into(),
                target_handle: u2_handle.clone(),
                game_id: None,
                note: None,
            },
        )
        .await
        .unwrap_err();

    assert!(matches!(err, ModerationError::RateLimited(_)));
}

#[tokio::test]
async fn test_chat_mute_sanction() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let redis_url = std::env::var("CH_REDIS_URL")
        .unwrap_or_else(|_| "redis+unix:///tmp/opencode/redis.sock".into());
    let redis = redis::Client::open(redis_url.as_str()).unwrap();
    let publisher = EventPublisher::new(redis.clone());
    let mod_svc = ModerationService::new(pool.clone(), redis.clone(), publisher);
    let game_svc = GameService::new(pool.clone());

    let (u1_id, _u1_h, _u1_pid) = make_user(&pool).await;
    let (u2_id, _u2_h, _u2_pid) = make_user(&pool).await;

    let mut conn = redis.get_multiplexed_async_connection().await.unwrap();
    let _: Result<(), _> = redis::cmd("DEL")
        .arg(format!("sanction:mute:{u1_id}"))
        .query_async(&mut conn)
        .await;

    let tc = TimeControl {
        initial_ms: 180_000,
        increment_ms: 2_000,
    };
    let game = game_svc
        .create_game(u1_id, u2_id, Pool::Blitz, tc, true)
        .await
        .unwrap();

    // Insert active mute sanction
    sqlx::query(
        r#"
        INSERT INTO sanctions (user_id, kind, reason, issued_by)
        VALUES ($1, 'mute', 'chat abuse violation', 'system_moderator')
        "#,
    )
    .bind(u1_id)
    .execute(&pool)
    .await
    .unwrap();

    // Verify user is muted
    assert!(mod_svc.is_muted(u1_id).await.unwrap());

    // Muted user cannot send chat
    let err = mod_svc
        .send_chat(u1_id, &game.public_id, "Hello!")
        .await
        .unwrap_err();
    assert!(matches!(err, ModerationError::Muted));
}

#[tokio::test]
async fn test_grievance_info() {
    let pool = sqlx::PgPool::connect_lazy("postgres://127.0.0.1:54329/hustle_stub").unwrap();
    let redis = redis::Client::open("redis+unix:///tmp/opencode/redis.sock").unwrap();
    let publisher = EventPublisher::new(redis.clone());
    let mod_svc = ModerationService::new(pool, redis, publisher);

    let info = mod_svc.get_grievance_info();
    assert_eq!(info.officer_name, "Priya Sharma");
    assert_eq!(info.acknowledgment_sla_hours, 24);
    assert_eq!(info.disposition_sla_days, 15);
    assert!(!info.policies.is_empty());
}
