use hustle_identity::signup::{Consents, ProfileInput, SignupService, VerifiedIdentity};
use hustle_identity::TokenService;
use hustle_rewards::{GameFinishedEvent, RewardError, RewardService};
use std::sync::atomic::{AtomicU64, Ordering};
use time::OffsetDateTime;

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
    format!("rew_user_{}", counter())
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

async fn make_user(pool: &sqlx::PgPool, welcome_gold: i64) -> i64 {
    let token_service = TokenService::new(b"test_secret_key_32_bytes_long_ok!");
    let signup = SignupService::new(pool.clone(), token_service, welcome_gold);
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

    outcome.user_id
}

#[tokio::test]
async fn test_missions_lifecycle_and_claim() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let svc = RewardService::new(pool.clone());
    let u1 = make_user(&pool, 1000).await;
    let u2 = make_user(&pool, 1000).await;
    let now = OffsetDateTime::now_utc();

    // 1. Initial missions seeding
    let missions = svc.ensure_user_missions(u1, now).await.unwrap();
    assert!(!missions.is_empty());
    assert!(missions.iter().any(|m| m.id == "daily_games_1"));

    // 2. Incomplete mission cannot be claimed
    let err = svc
        .claim_mission(u1, "daily_games_1", now)
        .await
        .unwrap_err();
    match err {
        RewardError::NotCompleted { current, goal } => {
            assert_eq!(current, 0);
            assert_eq!(goal, 1);
        }
        other => panic!("expected NotCompleted, got: {:?}", other),
    }

    // 3. Record game finish: u1 beats u2 by checkmate
    let event = GameFinishedEvent {
        white_user_id: u1,
        black_user_id: u2,
        winner_user_id: Some(u1),
        checkmate: true,
        draw: false,
    };
    svc.record_game_finished(&event, now).await.unwrap();

    // 4. Verify progress updated
    let updated = svc.ensure_user_missions(u1, now).await.unwrap();
    let games_mission = updated.iter().find(|m| m.id == "daily_games_1").unwrap();
    assert_eq!(games_mission.progress, 1);
    assert!(!games_mission.claimed);

    let win_mission = updated.iter().find(|m| m.id == "daily_win_1").unwrap();
    assert_eq!(win_mission.progress, 1);

    let mate_mission = updated
        .iter()
        .find(|m| m.id == "daily_checkmate_1")
        .unwrap();
    assert_eq!(mate_mission.progress, 1);

    // 5. Claim daily_games_1 (reward: 250 gold)
    let claim_res = svc.claim_mission(u1, "daily_games_1", now).await.unwrap();
    assert!(claim_res.claimed);
    assert_eq!(claim_res.amount, 250);
    // User started with 1000 + 250 = 1250 (+ optional first_win achievement if triggered)
    assert!(claim_res.new_balance >= 1250);

    // 6. Claiming again fails with AlreadyClaimed
    let replay_err = svc
        .claim_mission(u1, "daily_games_1", now)
        .await
        .unwrap_err();
    match replay_err {
        RewardError::AlreadyClaimed(id) => assert_eq!(id, "daily_games_1"),
        other => panic!("expected AlreadyClaimed, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_achievements_progression() {
    let Some(pool) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let svc = RewardService::new(pool.clone());
    let u1 = make_user(&pool, 1000).await;

    // 1. Initial achievements list
    let achs = svc.get_user_achievements(u1).await.unwrap();
    assert!(!achs.is_empty());
    let first_win = achs.iter().find(|a| a.id == "achieve_first_win").unwrap();
    assert!(!first_win.unlocked);
    assert_eq!(first_win.progress, 0);

    // 2. Update profile with 1 win to simulate game completion
    sqlx::query(
        "UPDATE profiles SET wins = wins + 1, games_count = games_count + 1 WHERE user_id = $1",
    )
    .bind(u1)
    .execute(&pool)
    .await
    .unwrap();

    // 3. Evaluate achievements
    let unlocked = svc.evaluate_achievements(u1).await.unwrap();
    assert!(unlocked.contains(&"achieve_first_win".to_string()));

    // 4. Achievement is now marked unlocked
    let achs_after = svc.get_user_achievements(u1).await.unwrap();
    let first_win_after = achs_after
        .iter()
        .find(|a| a.id == "achieve_first_win")
        .unwrap();
    assert!(first_win_after.unlocked);
    assert!(first_win_after.granted_at.is_some());
    assert_eq!(first_win_after.progress, 1);

    // 5. Balance has the 500 gold reward credited (1000 + 500 = 1500)
    let bal: i64 = sqlx::query_scalar("SELECT balance FROM gold_balances WHERE user_id = $1")
        .bind(u1)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(bal, 1500);
}
