//! Session store: PG `user_sessions` with refresh rotation + family reuse
//! detection (ADR-008). One row per issued refresh token; rotated tokens keep
//! their hash so reusing a superseded token revokes the whole family.

use sqlx::{PgPool, Postgres, Transaction};
use thiserror::Error;
use time::OffsetDateTime;

use crate::tokens::{new_refresh, refresh_hash, RefreshPair};

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("refresh token unknown")]
    Unknown,
    #[error("refresh token revoked (family invalidated on reuse)")]
    Reused,
    #[error("session revoked: {0}")]
    Revoked(String),
    #[error("database error")]
    Db(#[from] sqlx::Error),
}

#[derive(Debug)]
pub struct IssuedSession {
    pub session_id: i64,
    pub user_id: i64,
    pub refresh: RefreshPair,
}

/// Verified single-sign-on facts (Google id_token's `sub` + verified email).
#[derive(Debug, Clone)]
pub struct SsoUser {
    pub sub: String,
    pub email: String,
}

/// A validated refresh token's row.
#[derive(Debug)]
pub struct RefreshRow {
    pub session_id: i64,
    pub user_id: i64,
    pub family_id: uuid_like::FamilyId,
}

/// uuid alias to keep the type vocabulary local.
pub mod uuid_like {
    pub type FamilyId = uuid::Uuid;
}

/// `user_sessions` row as needed for rotation: (id, user_id, family, revoked_at, revoked_reason).
type RotationRow = (
    i64,
    i64,
    uuid_like::FamilyId,
    Option<OffsetDateTime>,
    Option<String>,
);

pub struct SessionStore {
    pool: PgPool,
}

impl SessionStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Issue a fresh session (new family). Runs inside the caller's txn so
    /// signup composes it atomically.
    pub async fn create(
        tx: &mut Transaction<'_, Postgres>,
        user_id: i64,
        device_label: Option<&str>,
        ip_hash: Option<&str>,
    ) -> Result<IssuedSession, SessionError> {
        let refresh = new_refresh();
        let family = uuid::Uuid::now_v7();
        let session_id: i64 = sqlx::query_scalar(
            "INSERT INTO user_sessions (user_id, family_id, refresh_hash, device_label, ip_hash)
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(user_id)
        .bind(family)
        .bind(&refresh.hash)
        .bind(device_label)
        .bind(ip_hash)
        .fetch_one(&mut **tx)
        .await?;
        Ok(IssuedSession {
            session_id,
            user_id,
            refresh,
        })
    }

    /// Rotate: validates the presented refresh token, revokes it ('rotated')
    /// and issues a sibling token in the same family. Reuse of a REVOKED token
    /// invalidates the entire family (T9-adjacent; ADR-008).
    pub async fn rotate(
        &self,
        presented_raw: &str,
    ) -> Result<(IssuedSession, RefreshRow), SessionError> {
        let hash = refresh_hash(presented_raw);
        let mut tx = self.pool.begin().await?;

        let row: Option<RotationRow> = sqlx::query_as(
            "SELECT id, user_id, family_id, revoked_at, revoked_reason
                 FROM user_sessions WHERE refresh_hash = $1 FOR UPDATE",
        )
        .bind(&hash)
        .fetch_optional(&mut *tx)
        .await?;

        let Some((session_id, user_id, family_id, revoked_at, revoked_reason)) = row else {
            tx.commit().await?;
            return Err(SessionError::Unknown);
        };

        if revoked_at.is_some() {
            // REUSE DETECTED: kill the whole family.
            sqlx::query(
                "UPDATE user_sessions
                 SET revoked_at = now(), revoked_reason = 'rotation_reuse'
                 WHERE family_id = $1 AND revoked_at IS NULL",
            )
            .bind(family_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            tracing::warn!(family = %family_id, "refresh-token reuse: family revoked");
            let reason = revoked_reason.as_deref().unwrap_or("rotated");
            return Err(match reason {
                "logout" | "logout_all" | "admin" => SessionError::Revoked(reason.to_string()),
                _ => SessionError::Reused,
            });
        }

        // Issue sibling + revoke old row as 'rotated'.
        let new_refresh = new_refresh();
        let new_session_id: i64 = sqlx::query_scalar(
            "INSERT INTO user_sessions (user_id, family_id, refresh_hash)
             VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(user_id)
        .bind(family_id)
        .bind(&new_refresh.hash)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE user_sessions SET revoked_at = now(), revoked_reason = 'rotated'
             WHERE id = $1",
        )
        .bind(session_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        Ok((
            IssuedSession {
                session_id: new_session_id,
                user_id,
                refresh: new_refresh,
            },
            RefreshRow {
                session_id,
                user_id,
                family_id,
            },
        ))
    }

    /// Revoke one session (logout) — idempotent.
    pub async fn revoke(&self, session_id: i64, reason: &str) -> Result<(), SessionError> {
        let mut tx = self.pool.begin().await?;
        Self::revoke_in_tx(&mut tx, session_id, reason).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn revoke_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        session_id: i64,
        reason: &str,
    ) -> Result<(), SessionError> {
        sqlx::query(
            "UPDATE user_sessions
             SET revoked_at = now(), revoked_reason = $2
             WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(session_id)
        .bind(reason)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Revoke every active session of a user (logout-all / admin action).
    pub async fn revoke_all(&self, user_id: i64, reason: &str) -> Result<u64, SessionError> {
        let mut tx = self.pool.begin().await?;
        let n = sqlx::query(
            "UPDATE user_sessions
             SET revoked_at = now(), revoked_reason = $2
             WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .bind(reason)
        .execute(&mut *tx) // owned transaction: deref once → &mut PgConnection
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(n)
    }
}

/// sha256 for IP minimization (DPDP): the server never stores raw IPs for
/// sessions; callers hash before persisting.
pub fn hash_ip(ip: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(ip.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
