//! L3 integration tests — Glicko-2 application against real Postgres.

use hustle_domain::Pool;
use hustle_ratings::{Outcome, PgRatingApplier, RatingApplier};
use sqlx;

#[tokio::test]
async fn white_win_moves_both_ratings_and_writes_history() {
    let Some(pg) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let (white, black) = hustle_testkit::make_two_users(&pg, "r").await;
    let game = hustle_testkit::make_game(&pg, white, black, "blitz").await;
    let applier = PgRatingApplier::new();

    let mut tx = pg.begin().await.unwrap();
    let applied = applier
        .apply_in_tx(&mut tx, game, white, black, Pool::Blitz, Outcome::WhiteWin)
        .await
        .unwrap()
        .expect("first application applies");
    tx.commit().await.unwrap();

    // Defaults are 1200/350; a win vs an equal provisional opponent lifts
    // white and drops black by the same magnitude.
    assert!(applied.white.after > applied.white.before, "white gains");
    assert!(applied.black.after < applied.black.before, "black loses");
    assert!((applied.white.delta + applied.black.delta).abs() < 1e-9);
    assert!(applied.white.rd_after < 350.0, "RD shrinks after playing");
    assert!(applied.black.rd_after < 350.0);

    // Persisted state (stored as f32 — compare with tolerance)
    let w = hustle_ratings::get(&pg, white, Pool::Blitz)
        .await
        .unwrap()
        .unwrap();
    assert!(
        (w.rating as f64 - applied.white.after).abs() < 0.5,
        "white persisted"
    );
    assert_eq!(w.games_count, 1);
    let b = hustle_ratings::get(&pg, black, Pool::Blitz)
        .await
        .unwrap()
        .unwrap();
    assert!(
        (b.rating as f64 - applied.black.after).abs() < 0.5,
        "black persisted"
    );

    // Two history rows (one per player)
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rating_history WHERE game_id = $1")
        .bind(game)
        .fetch_one(&pg)
        .await
        .unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn second_application_is_a_noop() {
    let Some(pg) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let (white, black) = hustle_testkit::make_two_users(&pg, "r").await;
    let game = hustle_testkit::make_game(&pg, white, black, "blitz").await;
    let applier = PgRatingApplier::new();

    for i in 0..2 {
        let mut tx = pg.begin().await.unwrap();
        let out = applier
            .apply_in_tx(&mut tx, game, white, black, Pool::Blitz, Outcome::Draw)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        if i == 0 {
            assert!(out.is_some(), "first apply applies");
        } else {
            assert!(out.is_none(), "replay is a no-op");
        }
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rating_history WHERE game_id = $1")
        .bind(game)
        .fetch_one(&pg)
        .await
        .unwrap();
    assert_eq!(count, 2, "exactly one row per player even after replay");
    let w = hustle_ratings::get(&pg, white, Pool::Blitz)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(w.games_count, 1, "games_count not double-bumped");
}

#[tokio::test]
async fn draw_between_equals_is_symmetric() {
    let Some(pg) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let (white, black) = hustle_testkit::make_two_users(&pg, "r").await;
    let game = hustle_testkit::make_game(&pg, white, black, "rapid").await;
    let applier = PgRatingApplier::new();

    let mut tx = pg.begin().await.unwrap();
    let applied = applier
        .apply_in_tx(&mut tx, game, white, black, Pool::Rapid, Outcome::Draw)
        .await
        .unwrap()
        .unwrap();
    tx.commit().await.unwrap();

    // Equal defaults: a draw must move both ratings (slightly up for both vs
    // equal provisional opponents is standard Glicko-2 behavior for wins-only
    // histories; deltas are equal magnitude, opposite signs).
    assert!(
        (applied.white.delta + applied.black.delta).abs() < 1e-9,
        "symmetric deltas"
    );
}

#[tokio::test]
async fn pools_are_independent() {
    let Some(pg) = hustle_testkit::pool().await else {
        eprintln!("skip: no CH_TEST_DATABASE_URL");
        return;
    };
    let (white, black) = hustle_testkit::make_two_users(&pg, "r").await;
    let game = hustle_testkit::make_game(&pg, white, black, "bullet").await;
    let applier = PgRatingApplier::new();

    let mut tx = pg.begin().await.unwrap();
    applier
        .apply_in_tx(&mut tx, game, white, black, Pool::Bullet, Outcome::WhiteWin)
        .await
        .unwrap()
        .unwrap();
    tx.commit().await.unwrap();

    let w_bullet = hustle_ratings::get(&pg, white, Pool::Bullet)
        .await
        .unwrap()
        .unwrap();
    let w_blitz = hustle_ratings::get(&pg, white, Pool::Blitz)
        .await
        .unwrap()
        .map_or(1200.0, |r| r.rating as f64);
    assert!(
        (w_bullet.rating as f64 - w_blitz).abs() > 1.0,
        "blitz untouched at default"
    );
}
