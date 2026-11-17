//! hustle-gold — the Gold Economy ledger (ADR-006).
//!
//! Owns `gold_balances` + `gold_ledger` EXCLUSIVELY (ADR-001): every Gold
//! mutation anywhere in the system flows through this crate. The core
//! [`PgGoldLedger::apply_in_tx`] runs INSIDE the caller's transaction so the
//! T3 finalize (games + ratings + gold) stays atomic — see schema §14.
//! Rules:
//! - append-only ledger; balance updates only follow a ledger row;
//! - `idem_key` unique → replay-safe grants (`ON CONFLICT DO NOTHING`);
//! - overdraft structurally impossible (domain check + DB CHECK backstop).

use async_trait::async_trait;
use hustle_domain::economy::{self, GoldError};
use serde_json::Value;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use thiserror::Error;

/// Ledger reason vocabulary — MUST match the `gold_ledger.reason` CHECK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoldReason {
    Welcome,
    DailyStreak,
    Mission,
    Achievement,
    Gameplay,
    AdReward,
    CosmeticPurchase,
    SupportGrant,
    Adjustment,
}

impl GoldReason {
    pub fn as_str(self) -> &'static str {
        match self {
            GoldReason::Welcome => "welcome",
            GoldReason::DailyStreak => "daily_streak",
            GoldReason::Mission => "mission",
            GoldReason::Achievement => "achievement",
            GoldReason::Gameplay => "gameplay",
            GoldReason::AdReward => "ad_reward",
            GoldReason::CosmeticPurchase => "cosmetic_purchase",
            GoldReason::SupportGrant => "support_grant",
            GoldReason::Adjustment => "adjustment",
        }
    }

    /// Mirror of the DB sign check for early, clear errors.
    pub fn allowed_delta(self, delta: i64) -> bool {
        match self {
            GoldReason::CosmeticPurchase => delta < 0,
            GoldReason::Adjustment => delta != 0,
            _ => delta > 0,
        }
    }
}

#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("insufficient balance: have {balance}, need {needed}")]
    Insufficient { balance: i64, needed: i64 },
    #[error("delta {delta} not allowed for reason {reason}")]
    BadDelta { delta: i64, reason: String },
    #[error("database error")]
    Db(#[from] sqlx::Error),
}

impl From<GoldError> for LedgerError {
    fn from(e: GoldError) -> Self {
        match e {
            GoldError::Insufficient { balance, needed } => {
                LedgerError::Insufficient { balance, needed }
            }
        }
    }
}

/// Outcome of an idempotent ledger operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantResult {
    /// Fresh ledger row inserted; new balance.
    Applied { ledger_id: i64, balance_after: i64 },
    /// `idem_key` already seen — returns the ORIGINAL effect's balance.
    Duplicate { balance_after: i64 },
}

/// One ledger operation, fully specified.
#[derive(Debug, Clone)]
pub struct LedgerOp {
    pub user_id: i64,
    pub delta: i64,
    pub reason: GoldReason,
    pub ref_type: Option<String>,
    pub ref_id: Option<String>,
    pub idem_key: Option<String>,
    pub meta: Value,
}

/// A ledger row as served to clients (`/me/gold`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LedgerEntry {
    pub id: i64,
    pub delta: i64,
    pub balance_after: i64,
    pub reason: String,
    pub ref_type: Option<String>,
    pub ref_id: Option<String>,
    pub created_at: time::OffsetDateTime,
}

/// Port implemented by the Postgres ledger; used inside finalize transactions
/// (hustle-chess) and standalone service calls (welcome, daily, purchase).
#[async_trait]
pub trait GoldLedger: Send + Sync {
    async fn balance(&self, user_id: i64) -> Result<i64, LedgerError>;
    async fn grant(&self, op: LedgerOp) -> Result<GrantResult, LedgerError>;
    async fn page(
        &self,
        user_id: i64,
        before_id: Option<i64>,
        limit: i32,
    ) -> Result<Vec<LedgerEntry>, LedgerError>;
}

/// Postgres implementation. Cheap to clone (internally a pool/Arc).
#[derive(Clone)]
pub struct PgGoldLedger {
    pool: PgPool,
}

impl PgGoldLedger {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Ensure a balance row exists (zero) so `FOR UPDATE` always has a row.
    async fn ensure_row(conn: &mut PgConnection, user_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO gold_balances (user_id, balance) VALUES ($1, 0) ON CONFLICT (user_id) DO NOTHING")
            .bind(user_id)
            .execute(conn)
            .await?;
        Ok(())
    }

    /// Core mutation, executed inside the CALLER's transaction (T1/T3/T4/T6
    /// all compose gold into their own boundaries).
    pub async fn apply_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        op: &LedgerOp,
    ) -> Result<GrantResult, LedgerError> {
        if !op.reason.allowed_delta(op.delta) {
            return Err(LedgerError::BadDelta {
                delta: op.delta,
                reason: op.reason.as_str().to_string(),
            });
        }
        Self::ensure_row(tx, op.user_id).await?;

        // Serialize per user; the row lock is what makes concurrent grants safe.
        let current: i64 =
            sqlx::query_scalar("SELECT balance FROM gold_balances WHERE user_id = $1 FOR UPDATE")
                .bind(op.user_id)
                .fetch_one(&mut **tx)
                .await?;

        let balance_after = economy::apply_delta(current, op.delta)?; // overdraft impossible

        let inserted: Option<i64> = sqlx::query_scalar(
            "INSERT INTO gold_ledger (user_id, delta, balance_after, reason, ref_type, ref_id, idem_key, meta)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (idem_key) DO NOTHING
             RETURNING id",
        )
        .bind(op.user_id)
        .bind(op.delta)
        .bind(balance_after)
        .bind(op.reason.as_str())
        .bind(op.ref_type.as_deref())
        .bind(op.ref_id.as_deref())
        .bind(op.idem_key.as_deref())
        .bind(&op.meta)
        .fetch_optional(&mut **tx)
        .await?;

        match inserted {
            Some(ledger_id) => {
                sqlx::query(
                    "UPDATE gold_balances SET balance = $2, updated_at = now() WHERE user_id = $1",
                )
                .bind(op.user_id)
                .bind(balance_after)
                .execute(&mut **tx)
                .await?;
                Ok(GrantResult::Applied {
                    ledger_id,
                    balance_after,
                })
            }
            None => {
                // Replay: recover the original effect's ending balance.
                let original: i64 =
                    sqlx::query_scalar("SELECT balance_after FROM gold_ledger WHERE idem_key = $1")
                        .bind(op.idem_key.as_deref().expect("conflict requires idem_key"))
                        .fetch_one(&mut **tx)
                        .await?;
                Ok(GrantResult::Duplicate {
                    balance_after: original,
                })
            }
        }
    }

    /// Read-only balance for API surfaces.
    pub async fn balance_of(&self, user_id: i64) -> Result<i64, LedgerError> {
        let mut conn = self.pool.acquire().await?;
        let bal: Option<i64> =
            sqlx::query_scalar("SELECT balance FROM gold_balances WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(&mut *conn)
                .await?;
        Ok(bal.unwrap_or(0))
    }

    /// Ledger page (newest first, cursor = last-seen id), `/me/gold`.
    pub async fn page_entries(
        &self,
        user_id: i64,
        before_id: Option<i64>,
        limit: i32,
    ) -> Result<Vec<LedgerEntry>, LedgerError> {
        let limit = limit.clamp(1, 100);
        let mut conn = self.pool.acquire().await?;
        let rows = match before_id {
            Some(before) => {
                sqlx::query_as::<_, LedgerEntry>(
                    "SELECT id, delta, balance_after, reason, ref_type, ref_id, created_at
                     FROM gold_ledger WHERE user_id = $1 AND id < $2
                     ORDER BY id DESC LIMIT $3",
                )
                .bind(user_id)
                .bind(before)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?
            }
            None => {
                sqlx::query_as::<_, LedgerEntry>(
                    "SELECT id, delta, balance_after, reason, ref_type, ref_id, created_at
                     FROM gold_ledger WHERE user_id = $1
                     ORDER BY id DESC LIMIT $2",
                )
                .bind(user_id)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await?
            }
        };
        Ok(rows)
    }
}

#[async_trait]
impl GoldLedger for PgGoldLedger {
    async fn balance(&self, user_id: i64) -> Result<i64, LedgerError> {
        self.balance_of(user_id).await
    }

    async fn grant(&self, op: LedgerOp) -> Result<GrantResult, LedgerError> {
        let mut tx = self.pool.begin().await?;
        let out = Self::apply_in_tx(&mut tx, &op).await?;
        tx.commit().await?;
        Ok(out)
    }

    async fn page(
        &self,
        user_id: i64,
        before_id: Option<i64>,
        limit: i32,
    ) -> Result<Vec<LedgerEntry>, LedgerError> {
        self.page_entries(user_id, before_id, limit).await
    }
}
