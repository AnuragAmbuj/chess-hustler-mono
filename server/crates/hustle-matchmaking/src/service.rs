//! Matchmaking service orchestrator (ADR-004, overview.md §4.2).

use crate::queue::{
    pool_queue_key, user_queue_key, QueueEntry, QueueMode, ATOMIC_CLAIM_LUA, QUEUE_TTL_SECONDS,
};
use crate::sweeper::can_pair;
use hustle_chess::{GameRow, GameService, GameServiceError};
use hustle_domain::{pool_for_time_control, Pool, TimeControl};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MatchmakingError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("game service error: {0}")]
    Game(#[from] GameServiceError),
    #[error("json serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("user not found: {0}")]
    UserNotFound(i64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueReceipt {
    pub pool: String,
    pub est_wait_s: u64,
    pub ratings: QueueRatings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueRatings {
    pub rating: i32,
    pub rd: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "queued", rename_all = "lowercase")]
pub enum QueueStatus {
    #[serde(rename = "true")]
    Queued { pool: String, waited_s: u64 },
    #[serde(rename = "false")]
    NotQueued,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResult {
    pub game_id: String,
    pub white_user_id: i64,
    pub black_user_id: i64,
    pub pool: Pool,
    pub rated: bool,
}

#[derive(Clone)]
pub struct MatchmakingService {
    pool: PgPool,
    redis: redis::Client,
    game_service: GameService,
}

impl MatchmakingService {
    pub fn new(pool: PgPool, redis: redis::Client) -> Self {
        let game_service = GameService::new(pool.clone());
        Self {
            pool,
            redis,
            game_service,
        }
    }

    pub fn game_service(&self) -> &GameService {
        &self.game_service
    }

    pub fn redis_client(&self) -> &redis::Client {
        &self.redis
    }

    /// Enqueue a player into matchmaking (ADR-004: one queue per user across pools).
    pub async fn join_queue(
        &self,
        user_id: i64,
        tc: TimeControl,
        rated: bool,
    ) -> Result<QueueReceipt, MatchmakingError> {
        let pool = pool_for_time_control(&tc);
        let mode = QueueMode::from_rated(rated);

        // Fetch user public_id and handle
        let user_info: Option<(String, String)> = sqlx::query_as(
            "SELECT u.public_id, p.handle
             FROM users u
             JOIN profiles p ON p.user_id = u.id
             WHERE u.id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        let (public_id, handle) = user_info.ok_or(MatchmakingError::UserNotFound(user_id))?;

        // Fetch current rating for pool or default to provisional 1200 / 350
        let rating_info: Option<(f32, f32, i32)> = sqlx::query_as(
            "SELECT rating, rd, games_count
             FROM ratings
             WHERE user_id = $1 AND pool = $2",
        )
        .bind(user_id)
        .bind(pool.as_str())
        .fetch_optional(&self.pool)
        .await?;

        let (rating, rd, provisional) = match rating_info {
            Some((r, rd, games)) => (r as f64, rd as f64, games < 8),
            None => (1200.0, 350.0, true),
        };
        let now_s = now_unix_s();

        let entry = QueueEntry {
            user_id,
            public_id,
            handle,
            pool,
            mode,
            tc,
            rating,
            rd,
            provisional,
            enqueued_at_unix_s: now_s,
        };

        let mut conn = self.redis.get_multiplexed_async_connection().await?;

        // Implicit leave: remove from old queue if already queued
        let u_key = user_queue_key(user_id);
        let old_entry_json: Option<String> = conn.get(&u_key).await?;
        if let Some(old_json) = old_entry_json {
            if let Ok(old_entry) = serde_json::from_str::<QueueEntry>(&old_json) {
                let old_q_key = pool_queue_key(old_entry.pool, old_entry.mode);
                let _: Result<i64, _> = conn.zrem(&old_q_key, user_id).await;
            }
        }

        // Store new entry and add to queue ZSET
        let entry_json = serde_json::to_string(&entry)?;
        let q_key = pool_queue_key(pool, mode);

        let _: () = conn.set_ex(&u_key, &entry_json, QUEUE_TTL_SECONDS).await?;
        let _: i64 = conn.zadd(&q_key, user_id, now_s as f64).await?;

        // Estimate wait time from current queue size
        let card: u64 = conn.zcard(&q_key).await.unwrap_or(1);
        let est_wait_s = match card {
            0 | 1 => 12,
            2..=5 => 6,
            _ => 3,
        };

        Ok(QueueReceipt {
            pool: pool.as_str().to_string(),
            est_wait_s,
            ratings: QueueRatings {
                rating: rating.round() as i32,
                rd: rd.round() as i32,
            },
        })
    }

    /// Leave matchmaking queue (idempotent; safe if not queued).
    pub async fn leave_queue(&self, user_id: i64) -> Result<(), MatchmakingError> {
        let mut conn = self.redis.get_multiplexed_async_connection().await?;
        let u_key = user_queue_key(user_id);
        let entry_json: Option<String> = conn.get(&u_key).await?;

        if let Some(json_str) = entry_json {
            if let Ok(entry) = serde_json::from_str::<QueueEntry>(&json_str) {
                let q_key = pool_queue_key(entry.pool, entry.mode);
                let _: Result<i64, _> = conn.zrem(&q_key, user_id).await;
            }
        }

        let _: () = conn.del(&u_key).await?;

        // Cleanup across all queues for safety
        for p in Pool::ALL {
            for m in [QueueMode::Rated, QueueMode::Casual] {
                let q_key = pool_queue_key(p, m);
                let _: Result<i64, _> = conn.zrem(&q_key, user_id).await;
            }
        }

        Ok(())
    }

    /// Query current queue status for a player.
    pub async fn get_status(&self, user_id: i64) -> Result<QueueStatus, MatchmakingError> {
        let mut conn = self.redis.get_multiplexed_async_connection().await?;
        let u_key = user_queue_key(user_id);
        let entry_json: Option<String> = conn.get(&u_key).await?;

        match entry_json {
            Some(json_str) => match serde_json::from_str::<QueueEntry>(&json_str) {
                Ok(entry) => {
                    let waited_s = (now_unix_s() - entry.enqueued_at_unix_s).max(0) as u64;
                    Ok(QueueStatus::Queued {
                        pool: entry.pool.as_str().to_string(),
                        waited_s,
                    })
                }
                Err(_) => Ok(QueueStatus::NotQueued),
            },
            None => Ok(QueueStatus::NotQueued),
        }
    }

    /// Sweeper pass for a single pool and mode.
    pub async fn sweep_pool(
        &self,
        pool: Pool,
        mode: QueueMode,
    ) -> Result<Vec<MatchResult>, MatchmakingError> {
        let mut conn = self.redis.get_multiplexed_async_connection().await?;
        let q_key = pool_queue_key(pool, mode);

        // Fetch user_ids ordered by enqueue timestamp ASC
        let user_ids: Vec<i64> = conn.zrange(&q_key, 0, -1).await?;
        if user_ids.len() < 2 {
            return Ok(vec![]);
        }

        // Fetch QueueEntry for all members
        let mut entries = Vec::new();
        for uid in user_ids {
            let u_key = user_queue_key(uid);
            let json_opt: Option<String> = conn.get(&u_key).await?;
            if let Some(json_str) = json_opt {
                if let Ok(entry) = serde_json::from_str::<QueueEntry>(&json_str) {
                    entries.push(entry);
                    continue;
                }
            }
            // Stale member in queue, clean up
            let _: Result<i64, _> = conn.zrem(&q_key, uid).await;
        }

        let now_s = now_unix_s();
        let mut paired = std::collections::HashSet::new();
        let mut matches = Vec::new();

        let n = entries.len();
        for i in 0..n {
            let p = &entries[i];
            if paired.contains(&p.user_id) {
                continue;
            }

            for q in entries.iter().skip(i + 1) {
                if paired.contains(&q.user_id) {
                    continue;
                }

                if !can_pair(p, q, now_s) {
                    continue;
                }

                // Check block list between p and q
                let is_blocked: bool = sqlx::query_scalar(
                    "SELECT EXISTS(
                        SELECT 1 FROM blocks
                        WHERE (blocker_id = $1 AND blocked_id = $2)
                           OR (blocker_id = $2 AND blocked_id = $1)
                     )",
                )
                .bind(p.user_id)
                .bind(q.user_id)
                .fetch_one(&self.pool)
                .await?;

                if is_blocked {
                    continue;
                }

                // Attempt atomic claim via Lua script
                let u_p_key = user_queue_key(p.user_id);
                let u_q_key = user_queue_key(q.user_id);

                let script = redis::Script::new(ATOMIC_CLAIM_LUA);
                let claim_ok: i32 = script
                    .key(&q_key)
                    .key(&u_p_key)
                    .key(&u_q_key)
                    .arg(p.user_id)
                    .arg(q.user_id)
                    .invoke_async(&mut conn)
                    .await?;

                if claim_ok == 1 {
                    paired.insert(p.user_id);
                    paired.insert(q.user_id);

                    // Color assignment: alternate based on timestamp
                    let p_is_white = (now_s % 2) == 0;
                    let (white, black) = if p_is_white { (p, q) } else { (q, p) };

                    // Create game in PostgreSQL
                    let game = self
                        .game_service
                        .create_game(white.user_id, black.user_id, pool, p.tc, mode.is_rated())
                        .await?;

                    // Publish matchmaking.matched events
                    self.publish_match_events(&mut conn, &game, white, black, mode.is_rated())
                        .await?;

                    matches.push(MatchResult {
                        game_id: game.public_id,
                        white_user_id: white.user_id,
                        black_user_id: black.user_id,
                        pool,
                        rated: mode.is_rated(),
                    });

                    break;
                }
            }
        }

        Ok(matches)
    }

    /// Sweep all 4 pools across rated and casual queues.
    pub async fn sweep_all(&self) -> Result<Vec<MatchResult>, MatchmakingError> {
        let mut all_matches = Vec::new();
        for pool in Pool::ALL {
            for mode in [QueueMode::Rated, QueueMode::Casual] {
                let matches = self.sweep_pool(pool, mode).await?;
                all_matches.extend(matches);
            }
        }
        Ok(all_matches)
    }

    /// Fan out matchmaking.matched events to both users via Redis Streams and PubSub.
    async fn publish_match_events(
        &self,
        conn: &mut redis::aio::MultiplexedConnection,
        game: &GameRow,
        white: &QueueEntry,
        black: &QueueEntry,
        rated: bool,
    ) -> Result<(), MatchmakingError> {
        let white_payload = serde_json::json!({
            "game_id": game.public_id,
            "color": "white",
            "rated": rated,
            "tc": {
                "initial_s": white.tc.initial_ms / 1000,
                "increment_s": white.tc.increment_ms / 1000,
            },
            "opponent": {
                "handle": black.handle,
                "rating": black.rating.round() as i32,
                "provisional": black.provisional,
            }
        });

        let black_payload = serde_json::json!({
            "game_id": game.public_id,
            "color": "black",
            "rated": rated,
            "tc": {
                "initial_s": black.tc.initial_ms / 1000,
                "increment_s": black.tc.increment_ms / 1000,
            },
            "opponent": {
                "handle": white.handle,
                "rating": white.rating.round() as i32,
                "provisional": white.provisional,
            }
        });

        let white_stream = format!("u:{}", white.user_id);
        let black_stream = format!("u:{}", black.user_id);

        let white_json = serde_json::to_string(&white_payload)?;
        let black_json = serde_json::to_string(&black_payload)?;

        // Append to personal Redis Streams
        let _: Result<String, _> = redis::cmd("XADD")
            .arg(&white_stream)
            .arg("*")
            .arg("type")
            .arg("matchmaking.matched")
            .arg("data")
            .arg(&white_json)
            .query_async(conn)
            .await;

        let _: Result<String, _> = redis::cmd("XADD")
            .arg(&black_stream)
            .arg("*")
            .arg("type")
            .arg("matchmaking.matched")
            .arg("data")
            .arg(&black_json)
            .query_async(conn)
            .await;

        // Publish to PubSub for live WebSocket subscribers
        let _: Result<i64, _> = conn
            .publish(format!("ws:{white_stream}"), &white_json)
            .await;
        let _: Result<i64, _> = conn
            .publish(format!("ws:{black_stream}"), &black_json)
            .await;

        Ok(())
    }
}

fn now_unix_s() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
