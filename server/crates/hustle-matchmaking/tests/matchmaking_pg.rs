//! Integration tests for matchmaking queue and pairing sweep (PG + Redis).

use hustle_domain::{Pool, TimeControl};
use hustle_matchmaking::{MatchmakingService, QueueMode, QueueStatus};
use hustle_testkit::{make_two_users, make_user, pool, redis};

fn blitz_tc() -> TimeControl {
    TimeControl {
        initial_ms: 180_000,
        increment_ms: 2_000,
    }
}

fn bullet_tc() -> TimeControl {
    TimeControl {
        initial_ms: 60_000,
        increment_ms: 0,
    }
}

fn rapid_tc() -> TimeControl {
    TimeControl {
        initial_ms: 600_000,
        increment_ms: 0,
    }
}

#[tokio::test]
async fn queue_join_and_leave_lifecycle() {
    let Some(pool) = pool().await else { return };
    let Some(redis) = redis().await else { return };
    let mm = MatchmakingService::new(pool.clone(), redis.clone());

    let user_id = make_user(&pool, "mm1").await;

    // Initially not queued
    let status = mm.get_status(user_id).await.expect("status check");
    assert!(matches!(status, QueueStatus::NotQueued));

    // Join Blitz queue
    let receipt = mm
        .join_queue(user_id, blitz_tc(), true)
        .await
        .expect("join queue");
    assert_eq!(receipt.pool, "blitz");
    assert_eq!(receipt.ratings.rating, 1200);

    // Now queued
    let status = mm.get_status(user_id).await.expect("status check");
    match status {
        QueueStatus::Queued { pool, .. } => assert_eq!(pool, "blitz"),
        QueueStatus::NotQueued => panic!("expected queued"),
    }

    // Leave queue
    mm.leave_queue(user_id).await.expect("leave queue");

    // Status returns NotQueued
    let status = mm.get_status(user_id).await.expect("status check");
    assert!(matches!(status, QueueStatus::NotQueued));
}

#[tokio::test]
async fn queue_rejoin_replaces_previous_queue() {
    let Some(pool) = pool().await else { return };
    let Some(redis) = redis().await else { return };
    let mm = MatchmakingService::new(pool.clone(), redis.clone());

    let user_id = make_user(&pool, "mm2").await;

    // Join Blitz
    mm.join_queue(user_id, blitz_tc(), true)
        .await
        .expect("join blitz");

    // Rejoin Bullet
    mm.join_queue(user_id, bullet_tc(), true)
        .await
        .expect("join bullet");

    // Status is Bullet
    let status = mm.get_status(user_id).await.expect("status");
    match status {
        QueueStatus::Queued { pool, .. } => assert_eq!(pool, "bullet"),
        QueueStatus::NotQueued => panic!("expected queued in bullet"),
    }
}

#[tokio::test]
async fn sweep_pairs_compatible_players_and_creates_game() {
    let Some(pool) = pool().await else { return };
    let Some(redis) = redis().await else { return };
    let mm = MatchmakingService::new(pool.clone(), redis.clone());

    let (u1, u2) = make_two_users(&pool, "pair").await;

    // Both players join Blitz rated
    mm.join_queue(u1, blitz_tc(), true).await.expect("join u1");
    mm.join_queue(u2, blitz_tc(), true).await.expect("join u2");

    // Run sweep
    let matches = mm
        .sweep_pool(Pool::Blitz, QueueMode::Rated)
        .await
        .expect("sweep");
    assert_eq!(matches.len(), 1, "expected 1 match created");

    let m = &matches[0];
    assert_eq!(m.pool, Pool::Blitz);
    assert!(m.rated);
    assert!(
        (m.white_user_id == u1 && m.black_user_id == u2)
            || (m.white_user_id == u2 && m.black_user_id == u1)
    );

    // Verify game exists in Postgres in 'starting' status
    let game = mm
        .game_service()
        .get_game_by_public_id(&m.game_id)
        .await
        .expect("get game")
        .expect("game exists");
    assert_eq!(game.status, "starting");
    assert_eq!(game.pool, "blitz");
    assert_eq!(game.w_remaining_ms, Some(180_000));
    assert_eq!(game.b_remaining_ms, Some(180_000));

    // Both players no longer queued
    assert!(matches!(
        mm.get_status(u1).await.unwrap(),
        QueueStatus::NotQueued
    ));
    assert!(matches!(
        mm.get_status(u2).await.unwrap(),
        QueueStatus::NotQueued
    ));
}

#[tokio::test]
async fn sweep_blocks_prevent_pairing() {
    let Some(pool) = pool().await else { return };
    let Some(redis) = redis().await else { return };
    let mm = MatchmakingService::new(pool.clone(), redis.clone());

    let (u1, u2) = make_two_users(&pool, "block").await;

    // u1 blocks u2
    sqlx::query("INSERT INTO blocks (blocker_id, blocked_id) VALUES ($1, $2)")
        .bind(u1)
        .bind(u2)
        .execute(&pool)
        .await
        .expect("insert block");

    // Both join Rapid rated
    mm.join_queue(u1, rapid_tc(), true).await.expect("join u1");
    mm.join_queue(u2, rapid_tc(), true).await.expect("join u2");

    // Sweep should produce 0 matches
    let matches = mm
        .sweep_pool(Pool::Rapid, QueueMode::Rated)
        .await
        .expect("sweep");
    assert_eq!(matches.len(), 0, "blocked players must not match");

    // Both remain queued
    assert!(matches!(
        mm.get_status(u1).await.unwrap(),
        QueueStatus::Queued { .. }
    ));
    assert!(matches!(
        mm.get_status(u2).await.unwrap(),
        QueueStatus::Queued { .. }
    ));
}
