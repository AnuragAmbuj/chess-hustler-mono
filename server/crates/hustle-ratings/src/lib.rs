//! hustle-ratings — per-pool Glicko-2 application (ADR-005).
//!
//! Owns `ratings` + `rating_history` EXCLUSIVELY. The applier runs INSIDE the
//! caller's transaction (T3 finalize composes games + ratings + gold atomically,
//! schema §14). Both players' rows are locked `FOR UPDATE` so concurrent
//! finalizations serialize per player. Idempotency: unique `(user_id, game_id)`
//! on `rating_history` (migration 0002) + `ON CONFLICT DO NOTHING`.

use async_trait::async_trait;
use hustle_domain::ratings::{self, GlickoRating, Opponent};
use hustle_domain::Pool;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RatingsError {
    #[error("database error")]
    Db(#[from] sqlx::Error),
}

/// Outcome change from one player's perspective (ADR-005 uses 1/0.5/0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    WhiteWin,
    BlackWin,
    Draw,
}

impl Outcome {
    fn score_for(self, side: Side) -> f64 {
        match (self, side) {
            (Outcome::WhiteWin, Side::White) | (Outcome::BlackWin, Side::Black) => 1.0,
            (Outcome::WhiteWin, Side::Black) | (Outcome::BlackWin, Side::White) => 0.0,
            (Outcome::Draw, _) => 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Side {
    White,
    Black,
}

/// One player's applied rating change (REST/WS shape: before/after/delta).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RatingDelta {
    pub before: f64,
    pub after: f64,
    pub rd_after: f64,
    pub delta: f64,
}

pub struct AppliedRatings {
    pub white: RatingDelta,
    pub black: RatingDelta,
}

#[async_trait]
pub trait RatingApplier: Send + Sync {
    /// Applies one game's ratings inside the caller's transaction. Returns
    /// `None` when this game already applied its ratings (idempotent replay).
    async fn apply_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: i64,
        white_id: i64,
        black_id: i64,
        pool: Pool,
        outcome: Outcome,
    ) -> Result<Option<AppliedRatings>, RatingsError>;
}

#[derive(Clone, Default)]
pub struct PgRatingApplier;

impl PgRatingApplier {
    pub fn new() -> Self {
        Self
    }

    async fn fetch_for_update(
        conn: &mut PgConnection,
        user_id: i64,
        pool: Pool,
    ) -> Result<GlickoRating, sqlx::Error> {
        // Ensure the row exists (defaults per schema §6), then lock.
        sqlx::query(
            "INSERT INTO ratings (user_id, pool) VALUES ($1, $2)
             ON CONFLICT (user_id, pool) DO NOTHING",
        )
        .bind(user_id)
        .bind(pool.as_str())
        .execute(&mut *conn)
        .await?;
        let (r, rd, vol): (f32, f32, f32) = sqlx::query_as(
            "SELECT rating, rd, volatility FROM ratings WHERE user_id = $1 AND pool = $2 FOR UPDATE",
        )
        .bind(user_id)
        .bind(pool.as_str())
        .fetch_one(&mut *conn)
        .await?;
        Ok(GlickoRating {
            rating: r as f64,
            deviation: rd as f64,
            volatility: vol as f64,
        })
    }

    async fn write_player(
        conn: &mut PgConnection,
        game_id: i64,
        user_id: i64,
        pool: Pool,
        before: GlickoRating,
        after: GlickoRating,
    ) -> Result<Option<RatingDelta>, sqlx::Error> {
        let delta = after.rating - before.rating;

        // Idempotency wall: one history row per (user, game).
        let inserted: Option<i64> = sqlx::query_scalar(
            "INSERT INTO rating_history (user_id, pool, game_id, rating_before, rating_after, rd_after, delta)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (user_id, game_id) DO NOTHING
             RETURNING id",
        )
        .bind(user_id)
        .bind(pool.as_str())
        .bind(game_id)
        .bind(before.rating as f32)
        .bind(after.rating as f32)
        .bind(after.deviation as f32)
        .bind(delta as f32)
        .fetch_optional(&mut *conn)
        .await?;

        if inserted.is_none() {
            return Ok(None); // already applied
        }

        sqlx::query(
            "UPDATE ratings
             SET rating = $3, rd = $4, volatility = $5, games_count = games_count + 1,
                 peak = GREATEST(peak, $3), last_game_at = now()
             WHERE user_id = $1 AND pool = $2",
        )
        .bind(user_id)
        .bind(pool.as_str())
        .bind(after.rating as f32)
        .bind(after.deviation as f32)
        .bind(after.volatility as f32)
        .execute(&mut *conn)
        .await?;

        Ok(Some(RatingDelta {
            before: before.rating,
            after: after.rating,
            rd_after: after.deviation,
            delta,
        }))
    }
}

#[async_trait]
impl RatingApplier for PgRatingApplier {
    async fn apply_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: i64,
        white_id: i64,
        black_id: i64,
        pool: Pool,
        outcome: Outcome,
    ) -> Result<Option<AppliedRatings>, RatingsError> {
        // Lock both rows in a deterministic order (lower id first) to avoid
        // deadlocks between concurrent finalizations sharing a player.
        let (a_id, b_id) = if white_id < black_id {
            (white_id, black_id)
        } else {
            (black_id, white_id)
        };
        let a = Self::fetch_for_update(tx, a_id, pool).await?;
        let b = Self::fetch_for_update(tx, b_id, pool).await?;
        let (white, black) = if a_id == white_id { (a, b) } else { (b, a) };

        let white_after = ratings::update(
            white,
            &[Opponent {
                rating: black,
                score: outcome.score_for(Side::White),
            }],
        );
        let black_after = ratings::update(
            black,
            &[Opponent {
                rating: white,
                score: outcome.score_for(Side::Black),
            }],
        );

        let white_delta =
            Self::write_player(tx, game_id, white_id, pool, white, white_after).await?;
        let black_delta =
            Self::write_player(tx, game_id, black_id, pool, black, black_after).await?;

        Ok(match (white_delta, black_delta) {
            (Some(w), Some(b)) => Some(AppliedRatings { white: w, black: b }),
            // Neither-or-one means a prior finalize already applied.
            _ => None,
        })
    }
}

/// Read-side (own connection): a player's current rating facts in a pool.
pub struct RatingRow {
    pub rating: f32,
    pub rd: f32,
    pub volatility: f32,
    pub games_count: i32,
    pub peak: f32,
}

/// One pool's rating as served in profile/self payloads.
#[derive(Debug, Clone, Copy)]
pub struct PoolRating {
    pub pool: Pool,
    pub rating: f32,
    pub rd: f32,
    pub games_count: i32,
}

/// All rating rows a user has (missing pools are simply absent — callers
/// present the 1200/350 defaults for them).
pub async fn get_all(pool: &PgPool, user_id: i64) -> Result<Vec<PoolRating>, sqlx::Error> {
    let rows: Vec<(String, f32, f32, i32)> = sqlx::query_as(
        "SELECT pool, rating, rd, games_count FROM ratings WHERE user_id = $1 ORDER BY pool",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(p, rating, rd, games_count)| {
            let pool = Pool::parse(&p)?;
            Some(PoolRating {
                pool,
                rating,
                rd,
                games_count,
            })
        })
        .collect())
}

pub async fn get(
    pool: &PgPool,
    user_id: i64,
    pool_name: Pool,
) -> Result<Option<RatingRow>, sqlx::Error> {
    let row: Option<(f32, f32, f32, i32, f32)> = sqlx::query_as(
        "SELECT rating, rd, volatility, games_count, peak FROM ratings WHERE user_id = $1 AND pool = $2",
    )
    .bind(user_id)
    .bind(pool_name.as_str())
    .fetch_optional(pool)
    .await?;
    Ok(
        row.map(|(rating, rd, volatility, games_count, peak)| RatingRow {
            rating,
            rd,
            volatility,
            games_count,
            peak,
        }),
    )
}

pub async fn ensure_default(pool: &PgPool, user_id: i64) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    for p in Pool::ALL {
        sqlx::query(
            "INSERT INTO ratings (user_id, pool) VALUES ($1, $2) ON CONFLICT (user_id, pool) DO NOTHING",
        )
        .bind(user_id)
        .bind(p.as_str())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// A single entry in the leaderboard snapshot.
#[derive(Debug, Clone)]
pub struct LeaderboardRow {
    pub user_id: i64,
    pub public_id: sqlx::types::Uuid,
    pub handle: String,
    pub display_name: Option<String>,
    pub rating: f32,
    pub rd: f32,
    pub peak: f32,
    pub games_count: i32,
    pub wins: i32,
    pub losses: i32,
    pub draws: i32,
}

type RawLeaderboardRow = (
    i64,
    sqlx::types::Uuid,
    String,
    Option<String>,
    f32,
    f32,
    f32,
    i32,
    i32,
    i32,
    i32,
);

/// Fetch top-ranked active players in a pool, ordered by rating descending.
pub async fn get_leaderboard(
    pool: &PgPool,
    pool_name: Pool,
    limit: i64,
) -> Result<Vec<LeaderboardRow>, sqlx::Error> {
    let rows: Vec<RawLeaderboardRow> = sqlx::query_as(
        "SELECT r.user_id, u.public_id, p.handle, p.display_name, r.rating, r.rd, r.peak, r.games_count, p.wins, p.losses, p.draws
         FROM ratings r
         JOIN users u ON u.id = r.user_id
         JOIN profiles p ON p.user_id = r.user_id
         WHERE r.pool = $1 AND u.status = 'active'
         ORDER BY r.rating DESC
         LIMIT $2",
    )
    .bind(pool_name.as_str())
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(user_id, public_id, handle, display_name, rating, rd, peak, games_count, wins, losses, draws)| {
            LeaderboardRow {
                user_id,
                public_id,
                handle,
                display_name,
                rating,
                rd,
                peak,
                games_count,
                wins,
                losses,
                draws,
            }
        })
        .collect())
}
