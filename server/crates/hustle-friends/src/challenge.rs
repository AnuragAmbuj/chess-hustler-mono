use crate::error::FriendError;
use crate::models::DirectChallenge;
use hustle_chess::GameService;
use hustle_domain::{pool_for_time_control, TimeControl};
use hustle_realtime::EventPublisher;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeRecord {
    pub id: String,
    pub challenger_id: i64,
    pub challenger_public_id: String,
    pub challenger_handle: String,
    pub challenger_rating: i32,
    pub target_id: i64,
    pub target_public_id: String,
    pub target_handle: String,
    pub initial_s: u32,
    pub increment_s: u32,
    pub rated: bool,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Clone)]
pub struct ChallengeService {
    pool: PgPool,
    redis: redis::Client,
    publisher: EventPublisher,
    game_service: GameService,
}

impl ChallengeService {
    pub fn new(
        pool: PgPool,
        redis: redis::Client,
        publisher: EventPublisher,
        game_service: GameService,
    ) -> Self {
        Self {
            pool,
            redis,
            publisher,
            game_service,
        }
    }

    /// Create an ephemeral direct challenge against `target_handle` valid for 120s.
    pub async fn create_challenge(
        &self,
        challenger_id: i64,
        target_handle: &str,
        initial_s: u32,
        increment_s: u32,
        rated: bool,
    ) -> Result<DirectChallenge, FriendError> {
        let target = sqlx::query(
            r#"
            SELECT u.id, u.public_id, p.handle
            FROM profiles p
            JOIN users u ON u.id = p.user_id
            WHERE lower(p.handle) = lower($1)
            "#,
        )
        .bind(target_handle)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| FriendError::UserNotFound(target_handle.to_string()))?;

        let target_id: i64 = target.get("id");
        if challenger_id == target_id {
            return Err(FriendError::SelfActionNotAllowed);
        }

        let target_public_id: String = target.get("public_id");
        let target_handle: String = target.get("handle");

        // Check if either user has blocked the other
        let is_blocked: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM blocks
                WHERE (blocker_id = $1 AND blocked_id = $2)
                   OR (blocker_id = $2 AND blocked_id = $1)
            )
            "#,
        )
        .bind(challenger_id)
        .bind(target_id)
        .fetch_one(&self.pool)
        .await?;

        if is_blocked {
            return Err(FriendError::BlockedByUser);
        }

        let challenger = sqlx::query(
            r#"
            SELECT u.public_id, p.handle, COALESCE(r.rating, 1200)::int as rating
            FROM users u
            JOIN profiles p ON p.user_id = u.id
            LEFT JOIN ratings r ON r.user_id = u.id AND r.pool = 'blitz'
            WHERE u.id = $1
            "#,
        )
        .bind(challenger_id)
        .fetch_one(&self.pool)
        .await?;

        let challenger_public_id: String = challenger.get("public_id");
        let challenger_handle: String = challenger.get("handle");
        let challenger_rating: i32 = challenger.get("rating");

        let challenge_id = format!("chal_{}", Ulid::new().to_string());
        let now = time::OffsetDateTime::now_utc();
        let expires = now + time::Duration::seconds(120);

        let now_str = now
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
        let expires_str = expires
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

        let record = ChallengeRecord {
            id: challenge_id.clone(),
            challenger_id,
            challenger_public_id: challenger_public_id.clone(),
            challenger_handle: challenger_handle.clone(),
            challenger_rating,
            target_id,
            target_public_id: target_public_id.clone(),
            target_handle: target_handle.clone(),
            initial_s,
            increment_s,
            rated,
            created_at: now_str.clone(),
            expires_at: expires_str.clone(),
        };

        let json_str = serde_json::to_string(&record)?;
        let mut conn = self.redis.get_multiplexed_async_connection().await?;
        let redis_key = format!("challenge:{challenge_id}");

        let _: () = conn.set_ex(&redis_key, &json_str, 120).await?;

        // Publish game.invite event to target's personal stream u:{target_id}
        let _ = self
            .publisher
            .publish(
                &format!("u:{target_id}"),
                "game.invite",
                json!({
                    "invite_id": challenge_id,
                    "from": {
                        "id": challenger_public_id,
                        "handle": challenger_handle,
                        "rating": challenger_rating,
                    },
                    "tc": {
                        "initial_s": initial_s,
                        "increment_s": increment_s,
                    },
                    "rated": rated,
                    "expires_at": expires_str,
                }),
            )
            .await;

        Ok(DirectChallenge {
            id: challenge_id,
            challenger_handle,
            challenger_rating,
            target_handle,
            initial_s,
            increment_s,
            rated,
            created_at: now_str,
            expires_at: expires_str,
        })
    }

    /// Accept a direct challenge, creating an active game.
    pub async fn accept_challenge(
        &self,
        target_id: i64,
        challenge_id: &str,
    ) -> Result<String, FriendError> {
        let redis_key = format!("challenge:{challenge_id}");
        let mut conn = self.redis.get_multiplexed_async_connection().await?;
        let json_str: Option<String> = conn.get(&redis_key).await?;

        let record: ChallengeRecord = match json_str {
            Some(s) => serde_json::from_str(&s)?,
            None => return Err(FriendError::ChallengeNotFound),
        };

        if record.target_id != target_id {
            return Err(FriendError::NotAuthorized);
        }

        // Delete the consumed challenge
        let _: () = conn.del(&redis_key).await.unwrap_or(());

        // Randomize white/black piece assignment
        let seed = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
        let (white_id, black_id) = if seed % 2 == 0 {
            (record.challenger_id, record.target_id)
        } else {
            (record.target_id, record.challenger_id)
        };

        let tc = TimeControl {
            initial_ms: record.initial_s * 1000,
            increment_ms: record.increment_s * 1000,
        };
        let pool = pool_for_time_control(&tc);

        let game_row = self
            .game_service
            .create_game(white_id, black_id, pool, tc, record.rated)
            .await
            .map_err(|e| FriendError::Database(sqlx::Error::Protocol(e.to_string())))?;

        let game_id = game_row.public_id;

        // Publish game.invite_result to challenger's personal stream
        let _ = self
            .publisher
            .publish(
                &format!("u:{}", record.challenger_id),
                "game.invite_result",
                json!({
                    "invite_id": challenge_id,
                    "accepted": true,
                    "game_id": game_id,
                }),
            )
            .await;

        Ok(game_id)
    }

    /// Decline a direct challenge.
    pub async fn decline_challenge(
        &self,
        target_id: i64,
        challenge_id: &str,
    ) -> Result<(), FriendError> {
        let redis_key = format!("challenge:{challenge_id}");
        let mut conn = self.redis.get_multiplexed_async_connection().await?;
        let json_str: Option<String> = conn.get(&redis_key).await?;

        let record: ChallengeRecord = match json_str {
            Some(s) => serde_json::from_str(&s)?,
            None => return Err(FriendError::ChallengeNotFound),
        };

        if record.target_id != target_id {
            return Err(FriendError::NotAuthorized);
        }

        let _: () = conn.del(&redis_key).await.unwrap_or(());

        // Publish decline to challenger
        let _ = self
            .publisher
            .publish(
                &format!("u:{}", record.challenger_id),
                "game.invite_result",
                json!({
                    "invite_id": challenge_id,
                    "accepted": false,
                }),
            )
            .await;

        Ok(())
    }
}
