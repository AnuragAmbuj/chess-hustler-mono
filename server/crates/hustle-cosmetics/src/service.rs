use crate::error::CosmeticsError;
use crate::models::{
    CosmeticItem, PurchaseLedgerInfo, PurchaseResult, UpdateLoadoutRequest, UserLoadout,
};
use hustle_gold::{GoldReason, LedgerOp, PgGoldLedger};
use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;

#[derive(Clone)]
pub struct CosmeticsService {
    pool: PgPool,
}

impl CosmeticsService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List catalog items, optionally filtered by category.
    /// If user_id is provided, includes `owned` flag for each item.
    pub async fn list_catalog(
        &self,
        user_id: Option<i64>,
        category: Option<&str>,
    ) -> Result<Vec<CosmeticItem>, CosmeticsError> {
        let items = sqlx::query_as::<_, CosmeticItem>(
            r#"
            SELECT
                c.id,
                c.category,
                c.name_key,
                c.price_gold,
                c.rarity,
                c.preview,
                c.available_from,
                c.available_to,
                c.active,
                CASE WHEN uc.cosmetic_id IS NOT NULL THEN true ELSE false END AS owned
            FROM cosmetics c
            LEFT JOIN user_cosmetics uc ON uc.cosmetic_id = c.id AND uc.user_id = $1
            WHERE c.active = true
              AND (c.available_from IS NULL OR c.available_from <= now())
              AND (c.available_to IS NULL OR c.available_to >= now())
              AND ($2::text IS NULL OR c.category = $2)
            ORDER BY c.category, c.price_gold ASC, c.id ASC
            "#,
        )
        .bind(user_id)
        .bind(category)
        .fetch_all(&self.pool)
        .await?;

        Ok(items)
    }

    /// Retrieve a single cosmetic item with ownership status.
    pub async fn get_item(
        &self,
        user_id: Option<i64>,
        cosmetic_id: &str,
    ) -> Result<CosmeticItem, CosmeticsError> {
        let item = sqlx::query_as::<_, CosmeticItem>(
            r#"
            SELECT
                c.id,
                c.category,
                c.name_key,
                c.price_gold,
                c.rarity,
                c.preview,
                c.available_from,
                c.available_to,
                c.active,
                CASE WHEN uc.cosmetic_id IS NOT NULL THEN true ELSE false END AS owned
            FROM cosmetics c
            LEFT JOIN user_cosmetics uc ON uc.cosmetic_id = c.id AND uc.user_id = $1
            WHERE c.id = $2
              AND c.active = true
            "#,
        )
        .bind(user_id)
        .bind(cosmetic_id)
        .fetch_optional(&self.pool)
        .await?;

        item.ok_or_else(|| CosmeticsError::NotFound(cosmetic_id.to_string()))
    }

    /// Purchase a cosmetic item with idempotency and atomic gold deduction.
    pub async fn purchase(
        &self,
        user_id: i64,
        cosmetic_id: &str,
        idem_key: Option<&str>,
    ) -> Result<PurchaseResult, CosmeticsError> {
        // Idempotency fast path: if idem_key is provided and already recorded in gold_ledger
        if let Some(key) = idem_key {
            let existing: Option<(i64, i64)> = sqlx::query_as(
                "SELECT delta, balance_after FROM gold_ledger WHERE user_id = $1 AND idem_key = $2",
            )
            .bind(user_id)
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;

            if let Some((delta, balance_after)) = existing {
                return Ok(PurchaseResult {
                    owned: true,
                    balance: balance_after,
                    ledger: PurchaseLedgerInfo {
                        reason: "cosmetic_purchase".to_string(),
                        delta,
                    },
                    replayed: true,
                });
            }
        }

        let mut tx = self.pool.begin().await?;

        // 1. Fetch item definition with row lock
        type CosmeticRow = (
            String,
            i64,
            bool,
            Option<OffsetDateTime>,
            Option<OffsetDateTime>,
        );
        let item: Option<CosmeticRow> = sqlx::query_as(
            r#"
                SELECT id, price_gold, active, available_from, available_to
                FROM cosmetics
                WHERE id = $1
                FOR SHARE
                "#,
        )
        .bind(cosmetic_id)
        .fetch_optional(&mut *tx)
        .await?;

        let (_id, price_gold, active, available_from, available_to) = match item {
            Some(row) => row,
            None => return Err(CosmeticsError::NotFound(cosmetic_id.to_string())),
        };

        if !active {
            return Err(CosmeticsError::Unavailable(cosmetic_id.to_string()));
        }

        let now = OffsetDateTime::now_utc();
        if let Some(from) = available_from {
            if now < from {
                return Err(CosmeticsError::Unavailable(cosmetic_id.to_string()));
            }
        }
        if let Some(to) = available_to {
            if now > to {
                return Err(CosmeticsError::Unavailable(cosmetic_id.to_string()));
            }
        }

        // 2. Check if already owned
        let already_owned: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM user_cosmetics WHERE user_id = $1 AND cosmetic_id = $2)",
        )
        .bind(user_id)
        .bind(cosmetic_id)
        .fetch_one(&mut *tx)
        .await?;

        if already_owned {
            return Err(CosmeticsError::AlreadyOwned(cosmetic_id.to_string()));
        }

        // 3. Deduct Gold in ledger inside caller transaction
        let op = LedgerOp {
            user_id,
            delta: -price_gold,
            reason: GoldReason::CosmeticPurchase,
            ref_type: Some("cosmetic".to_string()),
            ref_id: Some(cosmetic_id.to_string()),
            idem_key: idem_key.map(|k| k.to_string()),
            meta: serde_json::json!({ "cosmetic_id": cosmetic_id }),
        };

        let grant_res = PgGoldLedger::apply_in_tx(&mut tx, &op).await?;
        let (balance_after, replayed) = match grant_res {
            hustle_gold::GrantResult::Applied { balance_after, .. } => (balance_after, false),
            hustle_gold::GrantResult::Duplicate { balance_after } => (balance_after, true),
        };

        // 4. Record item in user inventory
        sqlx::query(
            r#"
            INSERT INTO user_cosmetics (user_id, cosmetic_id, acquired_at)
            VALUES ($1, $2, now())
            ON CONFLICT (user_id, cosmetic_id) DO NOTHING
            "#,
        )
        .bind(user_id)
        .bind(cosmetic_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(PurchaseResult {
            owned: true,
            balance: balance_after,
            ledger: PurchaseLedgerInfo {
                reason: "cosmetic_purchase".to_string(),
                delta: -price_gold,
            },
            replayed,
        })
    }

    /// Retrieve the equipped loadout for a user.
    pub async fn get_loadout(&self, user_id: i64) -> Result<UserLoadout, CosmeticsError> {
        let loadout = sqlx::query_as::<_, UserLoadout>(
            r#"
            SELECT board_id, pieces_id, avatar_id, frame_id
            FROM user_loadout
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(loadout.unwrap_or_default())
    }

    /// Update user loadout slots, verifying ownership and slot category compatibility.
    pub async fn update_loadout(
        &self,
        user_id: i64,
        req: UpdateLoadoutRequest,
    ) -> Result<UserLoadout, CosmeticsError> {
        let mut tx = self.pool.begin().await?;

        let current = self.get_loadout(user_id).await?;

        let final_board = match req.board_id {
            Some(ref id) if !id.trim().is_empty() => {
                Self::validate_slot(&mut tx, user_id, id, "board").await?;
                Some(id.clone())
            }
            Some(_) => None, // empty string clears slot
            None => current.board_id,
        };

        let final_pieces = match req.pieces_id {
            Some(ref id) if !id.trim().is_empty() => {
                Self::validate_slot(&mut tx, user_id, id, "pieces").await?;
                Some(id.clone())
            }
            Some(_) => None,
            None => current.pieces_id,
        };

        let final_avatar = match req.avatar_id {
            Some(ref id) if !id.trim().is_empty() => {
                Self::validate_slot(&mut tx, user_id, id, "avatar").await?;
                Some(id.clone())
            }
            Some(_) => None,
            None => current.avatar_id,
        };

        let final_frame = match req.frame_id {
            Some(ref id) if !id.trim().is_empty() => {
                Self::validate_slot(&mut tx, user_id, id, "frame").await?;
                Some(id.clone())
            }
            Some(_) => None,
            None => current.frame_id,
        };

        let updated = sqlx::query_as::<_, UserLoadout>(
            r#"
            INSERT INTO user_loadout (user_id, board_id, pieces_id, avatar_id, frame_id)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id) DO UPDATE SET
                board_id = EXCLUDED.board_id,
                pieces_id = EXCLUDED.pieces_id,
                avatar_id = EXCLUDED.avatar_id,
                frame_id = EXCLUDED.frame_id
            RETURNING board_id, pieces_id, avatar_id, frame_id
            "#,
        )
        .bind(user_id)
        .bind(&final_board)
        .bind(&final_pieces)
        .bind(&final_avatar)
        .bind(&final_frame)
        .fetch_one(&mut *tx)
        .await?;

        // Also sync profile avatar if avatar was explicitly set or cleared
        if req.avatar_id.is_some() {
            sqlx::query("UPDATE profiles SET avatar_cosmetic_id = $2 WHERE user_id = $1")
                .bind(user_id)
                .bind(&final_avatar)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;

        Ok(updated)
    }

    async fn validate_slot(
        conn: &mut PgConnection,
        user_id: i64,
        item_id: &str,
        expected_category: &'static str,
    ) -> Result<(), CosmeticsError> {
        let category: Option<String> =
            sqlx::query_scalar("SELECT category FROM cosmetics WHERE id = $1")
                .bind(item_id)
                .fetch_optional(&mut *conn)
                .await?;

        let category = match category {
            Some(c) => c,
            None => return Err(CosmeticsError::NotFound(item_id.to_string())),
        };

        if category != expected_category {
            return Err(CosmeticsError::InvalidCategory {
                item_id: item_id.to_string(),
                expected: expected_category,
                actual: category,
            });
        }

        let owned: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM user_cosmetics WHERE user_id = $1 AND cosmetic_id = $2)",
        )
        .bind(user_id)
        .bind(item_id)
        .fetch_one(&mut *conn)
        .await?;

        if !owned {
            return Err(CosmeticsError::NotOwned(item_id.to_string()));
        }

        Ok(())
    }
}
