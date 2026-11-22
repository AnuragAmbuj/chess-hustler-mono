use crate::error::FriendError;
use crate::models::{
    BlockedUserItem, FriendInfo, FriendPresence, FriendRequestItem, FriendRequestsResponse,
};
use hustle_realtime::EventPublisher;
use redis::AsyncCommands;
use serde_json::json;
use sqlx::{PgPool, Row};
use time::OffsetDateTime;

#[derive(Clone)]
pub struct FriendService {
    pool: PgPool,
    redis: redis::Client,
    publisher: EventPublisher,
}

impl FriendService {
    pub fn new(pool: PgPool, redis: redis::Client, publisher: EventPublisher) -> Self {
        Self {
            pool,
            redis,
            publisher,
        }
    }

    /// List all accepted friends for `user_id` with live presence from Redis.
    pub async fn get_friends(&self, user_id: i64) -> Result<Vec<FriendInfo>, FriendError> {
        let rows = sqlx::query(
            r#"
            SELECT
                f.id as friendship_id,
                f.responded_at,
                u.id as other_id,
                u.public_id,
                p.handle,
                p.display_name,
                p.avatar_cosmetic_id,
                COALESCE(r.rating, 1200)::int as rating
            FROM friendships f
            JOIN users u ON u.id = (CASE WHEN f.requester_id = $1 THEN f.addressee_id ELSE f.requester_id END)
            JOIN profiles p ON p.user_id = u.id
            LEFT JOIN ratings r ON r.user_id = u.id AND r.pool = 'blitz'
            WHERE (f.requester_id = $1 OR f.addressee_id = $1) AND f.status = 'accepted'
            ORDER BY p.handle ASC
            "#
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let mut friends = Vec::with_capacity(rows.len());
        let mut redis_conn = self.redis.get_multiplexed_async_connection().await?;

        for row in rows {
            let other_id: i64 = row.get("other_id");
            let is_online: bool = redis_conn
                .sismember("presence:online", other_id)
                .await
                .unwrap_or(false);

            let status_str: Option<String> = if is_online {
                redis_conn
                    .get(format!("presence:status:{other_id}"))
                    .await
                    .unwrap_or(None)
            } else {
                None
            };

            let status = status_str.unwrap_or_else(|| {
                if is_online {
                    "idle".to_string()
                } else {
                    "offline".to_string()
                }
            });

            friends.push(FriendInfo {
                id: row.get("friendship_id"),
                user_id: row.get("public_id"),
                handle: row.get("handle"),
                display_name: row.get("display_name"),
                avatar_cosmetic_id: row.get("avatar_cosmetic_id"),
                rating: row.get("rating"),
                presence: FriendPresence {
                    online: is_online,
                    status,
                },
                accepted_at: row.get("responded_at"),
            });
        }

        Ok(friends)
    }

    /// List pending incoming and outgoing friend requests.
    pub async fn get_requests(&self, user_id: i64) -> Result<FriendRequestsResponse, FriendError> {
        let incoming_rows = sqlx::query(
            r#"
            SELECT
                f.id,
                f.created_at,
                p.handle,
                p.display_name,
                p.avatar_cosmetic_id,
                COALESCE(r.rating, 1200)::int as rating
            FROM friendships f
            JOIN users u ON u.id = f.requester_id
            JOIN profiles p ON p.user_id = u.id
            LEFT JOIN ratings r ON r.user_id = u.id AND r.pool = 'blitz'
            WHERE f.addressee_id = $1 AND f.status = 'pending'
            ORDER BY f.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let outgoing_rows = sqlx::query(
            r#"
            SELECT
                f.id,
                f.created_at,
                p.handle,
                p.display_name,
                p.avatar_cosmetic_id,
                COALESCE(r.rating, 1200)::int as rating
            FROM friendships f
            JOIN users u ON u.id = f.addressee_id
            JOIN profiles p ON p.user_id = u.id
            LEFT JOIN ratings r ON r.user_id = u.id AND r.pool = 'blitz'
            WHERE f.requester_id = $1 AND f.status = 'pending'
            ORDER BY f.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(FriendRequestsResponse {
            incoming: incoming_rows
                .into_iter()
                .map(|r| FriendRequestItem {
                    id: r.get("id"),
                    handle: r.get("handle"),
                    display_name: r.get("display_name"),
                    rating: r.get("rating"),
                    avatar_cosmetic_id: r.get("avatar_cosmetic_id"),
                    created_at: r.get("created_at"),
                })
                .collect(),
            outgoing: outgoing_rows
                .into_iter()
                .map(|r| FriendRequestItem {
                    id: r.get("id"),
                    handle: r.get("handle"),
                    display_name: r.get("display_name"),
                    rating: r.get("rating"),
                    avatar_cosmetic_id: r.get("avatar_cosmetic_id"),
                    created_at: r.get("created_at"),
                })
                .collect(),
        })
    }

    /// Send a friend request by handle.
    pub async fn send_request(
        &self,
        requester_id: i64,
        target_handle: &str,
    ) -> Result<FriendRequestItem, FriendError> {
        let target = sqlx::query(
            r#"
            SELECT u.id, u.public_id, p.handle, p.display_name, p.avatar_cosmetic_id, COALESCE(r.rating, 1200)::int as rating
            FROM profiles p
            JOIN users u ON u.id = p.user_id
            LEFT JOIN ratings r ON r.user_id = u.id AND r.pool = 'blitz'
            WHERE lower(p.handle) = lower($1)
            "#
        )
        .bind(target_handle)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| FriendError::UserNotFound(target_handle.to_string()))?;

        let addressee_id: i64 = target.get("id");
        if requester_id == addressee_id {
            return Err(FriendError::SelfActionNotAllowed);
        }

        let target_public_id: String = target.get("public_id");
        let target_handle: String = target.get("handle");
        let target_display: String = target.get("display_name");
        let target_avatar: Option<String> = target.get("avatar_cosmetic_id");
        let target_rating: i32 = target.get("rating");

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
        .bind(requester_id)
        .bind(addressee_id)
        .fetch_one(&self.pool)
        .await?;

        if is_blocked {
            return Err(FriendError::BlockedByUser);
        }

        // Check existing friendship relation
        let existing = sqlx::query(
            r#"
            SELECT id, requester_id, addressee_id, status
            FROM friendships
            WHERE (requester_id = $1 AND addressee_id = $2)
               OR (requester_id = $2 AND addressee_id = $1)
            "#,
        )
        .bind(requester_id)
        .bind(addressee_id)
        .fetch_optional(&self.pool)
        .await?;

        let requester_meta = sqlx::query(
            r#"
            SELECT u.public_id, p.handle, p.display_name
            FROM users u
            JOIN profiles p ON p.user_id = u.id
            WHERE u.id = $1
            "#,
        )
        .bind(requester_id)
        .fetch_one(&self.pool)
        .await?;

        let requester_public_id: String = requester_meta.get("public_id");
        let requester_handle: String = requester_meta.get("handle");
        let requester_display: String = requester_meta.get("display_name");

        let request_item = if let Some(row) = existing {
            let row_id: i64 = row.get("id");
            let row_requester_id: i64 = row.get("requester_id");
            let row_status: String = row.get("status");

            match row_status.as_str() {
                "accepted" => return Err(FriendError::AlreadyFriends),
                "pending" => {
                    if row_requester_id == requester_id {
                        return Err(FriendError::RequestAlreadyPending);
                    } else {
                        // Incoming pending request from target: auto-accept!
                        sqlx::query(
                            r#"
                            UPDATE friendships
                            SET status = 'accepted', responded_at = now()
                            WHERE id = $1
                            "#,
                        )
                        .bind(row_id)
                        .execute(&self.pool)
                        .await?;

                        let _ = self
                            .publisher
                            .publish(
                                &format!("u:{requester_id}"),
                                "friend.accepted",
                                json!({
                                    "user": {
                                        "id": target_public_id,
                                        "handle": target_handle,
                                    }
                                }),
                            )
                            .await;

                        let _ = self
                            .publisher
                            .publish(
                                &format!("u:{addressee_id}"),
                                "friend.accepted",
                                json!({
                                    "user": {
                                        "id": requester_public_id,
                                        "handle": requester_handle,
                                    }
                                }),
                            )
                            .await;

                        FriendRequestItem {
                            id: row_id,
                            handle: target_handle,
                            display_name: target_display,
                            rating: target_rating,
                            avatar_cosmetic_id: target_avatar,
                            created_at: OffsetDateTime::now_utc(),
                        }
                    }
                }
                _ => {
                    // Re-activate previously removed/declined
                    let updated = sqlx::query(
                        r#"
                        UPDATE friendships
                        SET requester_id = $1, addressee_id = $2, status = 'pending',
                            created_at = now(), responded_at = NULL
                        WHERE id = $3
                        RETURNING id, created_at
                        "#,
                    )
                    .bind(requester_id)
                    .bind(addressee_id)
                    .bind(row_id)
                    .fetch_one(&self.pool)
                    .await?;

                    FriendRequestItem {
                        id: updated.get("id"),
                        handle: target_handle,
                        display_name: target_display,
                        rating: target_rating,
                        avatar_cosmetic_id: target_avatar,
                        created_at: updated.get("created_at"),
                    }
                }
            }
        } else {
            // Fresh request insert
            let inserted = sqlx::query(
                r#"
                INSERT INTO friendships (requester_id, addressee_id, status)
                VALUES ($1, $2, 'pending')
                RETURNING id, created_at
                "#,
            )
            .bind(requester_id)
            .bind(addressee_id)
            .fetch_one(&self.pool)
            .await?;

            FriendRequestItem {
                id: inserted.get("id"),
                handle: target_handle,
                display_name: target_display,
                rating: target_rating,
                avatar_cosmetic_id: target_avatar,
                created_at: inserted.get("created_at"),
            }
        };

        // Notify target via personal event stream
        let _ = self
            .publisher
            .publish(
                &format!("u:{addressee_id}"),
                "friend.request",
                json!({
                    "id": request_item.id,
                    "from": {
                        "id": requester_public_id,
                        "handle": requester_handle,
                        "display_name": requester_display,
                    }
                }),
            )
            .await;

        Ok(request_item)
    }

    /// Accept an incoming friend request by request ID.
    pub async fn accept_request(&self, user_id: i64, request_id: i64) -> Result<(), FriendError> {
        let row = sqlx::query(
            r#"
            SELECT f.id, f.requester_id, u.public_id as addressee_public_id, p.handle as addressee_handle
            FROM friendships f
            JOIN users u ON u.id = f.addressee_id
            JOIN profiles p ON p.user_id = u.id
            WHERE f.id = $1 AND f.addressee_id = $2 AND f.status = 'pending'
            "#
        )
        .bind(request_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(FriendError::RequestNotFound(request_id))?;

        let requester_id: i64 = row.get("requester_id");
        let addressee_public_id: String = row.get("addressee_public_id");
        let addressee_handle: String = row.get("addressee_handle");

        sqlx::query(
            r#"
            UPDATE friendships
            SET status = 'accepted', responded_at = now()
            WHERE id = $1
            "#,
        )
        .bind(request_id)
        .execute(&self.pool)
        .await?;

        // Publish friend.accepted event to requester's stream
        let _ = self
            .publisher
            .publish(
                &format!("u:{requester_id}"),
                "friend.accepted",
                json!({
                    "user": {
                        "id": addressee_public_id,
                        "handle": addressee_handle,
                    }
                }),
            )
            .await;

        Ok(())
    }

    /// Decline an incoming friend request by request ID.
    pub async fn decline_request(&self, user_id: i64, request_id: i64) -> Result<(), FriendError> {
        let res = sqlx::query(
            r#"
            UPDATE friendships
            SET status = 'declined', responded_at = now()
            WHERE id = $1 AND addressee_id = $2 AND status = 'pending'
            "#,
        )
        .bind(request_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        if res.rows_affected() == 0 {
            return Err(FriendError::RequestNotFound(request_id));
        }

        Ok(())
    }

    /// Remove an existing friend or cancel a friendship by ID.
    pub async fn remove_friend(&self, user_id: i64, friendship_id: i64) -> Result<(), FriendError> {
        let res = sqlx::query(
            r#"
            UPDATE friendships
            SET status = 'removed', responded_at = now()
            WHERE id = $1 AND (requester_id = $2 OR addressee_id = $2)
            "#,
        )
        .bind(friendship_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        if res.rows_affected() == 0 {
            return Err(FriendError::NotAuthorized);
        }

        Ok(())
    }

    /// List all blocked users for `user_id`.
    pub async fn get_blocks(&self, user_id: i64) -> Result<Vec<BlockedUserItem>, FriendError> {
        let rows = sqlx::query(
            r#"
            SELECT p.handle, p.display_name, b.created_at as blocked_at
            FROM blocks b
            JOIN users u ON u.id = b.blocked_id
            JOIN profiles p ON p.user_id = u.id
            WHERE b.blocker_id = $1
            ORDER BY b.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| BlockedUserItem {
                handle: r.get("handle"),
                display_name: r.get("display_name"),
                blocked_at: r.get("blocked_at"),
            })
            .collect())
    }

    /// Block a user by handle. Also terminates any friendship between them.
    pub async fn block_user(
        &self,
        blocker_id: i64,
        target_handle: &str,
    ) -> Result<BlockedUserItem, FriendError> {
        let target = sqlx::query(
            r#"
            SELECT u.id, p.handle, p.display_name
            FROM profiles p
            JOIN users u ON u.id = p.user_id
            WHERE lower(p.handle) = lower($1)
            "#,
        )
        .bind(target_handle)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| FriendError::UserNotFound(target_handle.to_string()))?;

        let blocked_id: i64 = target.get("id");
        if blocker_id == blocked_id {
            return Err(FriendError::SelfActionNotAllowed);
        }

        let target_handle: String = target.get("handle");
        let target_display: String = target.get("display_name");

        let inserted = sqlx::query(
            r#"
            INSERT INTO blocks (blocker_id, blocked_id)
            VALUES ($1, $2)
            ON CONFLICT (blocker_id, blocked_id) DO UPDATE SET created_at = now()
            RETURNING created_at as blocked_at
            "#,
        )
        .bind(blocker_id)
        .bind(blocked_id)
        .fetch_one(&self.pool)
        .await?;

        // Sever any friendship
        let _ = sqlx::query(
            r#"
            UPDATE friendships
            SET status = 'removed', responded_at = now()
            WHERE (requester_id = $1 AND addressee_id = $2)
               OR (requester_id = $2 AND addressee_id = $1)
            "#,
        )
        .bind(blocker_id)
        .bind(blocked_id)
        .execute(&self.pool)
        .await;

        Ok(BlockedUserItem {
            handle: target_handle,
            display_name: target_display,
            blocked_at: inserted.get("blocked_at"),
        })
    }

    /// Unblock a user by handle.
    pub async fn unblock_user(
        &self,
        blocker_id: i64,
        target_handle: &str,
    ) -> Result<(), FriendError> {
        let target = sqlx::query(
            r#"
            SELECT u.id
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

        sqlx::query(
            r#"
            DELETE FROM blocks
            WHERE blocker_id = $1 AND blocked_id = $2
            "#,
        )
        .bind(blocker_id)
        .bind(target_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
