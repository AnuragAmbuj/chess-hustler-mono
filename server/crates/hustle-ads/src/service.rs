use crate::domain::{
    AdEligibility, AdError, AdReject, AdSessionResponse, AdStatus, RawCallback, VerifiedReward,
};
use crate::verifier::AdVerifier;
use hustle_gold::{GoldReason, LedgerOp, PgGoldLedger};
use hustle_kernel::ServerConfig;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use time::{Duration, OffsetDateTime, UtcOffset};

#[derive(Clone)]
pub struct AdsService {
    pool: PgPool,
    verifier: Arc<dyn AdVerifier>,
}

impl AdsService {
    pub fn new(pool: PgPool, verifier: Arc<dyn AdVerifier>) -> Self {
        Self { pool, verifier }
    }

    pub fn verifier(&self) -> &Arc<dyn AdVerifier> {
        &self.verifier
    }

    fn today_ist() -> time::Date {
        let ist = UtcOffset::from_hms(5, 30, 0).expect("valid IST offset");
        OffsetDateTime::now_utc().to_offset(ist).date()
    }

    /// Check player eligibility and mint a new 30-min ad session.
    pub async fn create_session(
        &self,
        user_id: i64,
        config: &ServerConfig,
    ) -> Result<AdSessionResponse, AdError> {
        let now = OffsetDateTime::now_utc();

        // 1. Check active games
        let in_game: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM games
                WHERE (white_user_id = $1 OR black_user_id = $1)
                  AND status IN ('created', 'active')
            )
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        if in_game {
            return Err(AdError::InActiveGame);
        }

        // 2. Check active sanctions (ad_suspension or suspension)
        let sanctioned: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM sanctions
                WHERE user_id = $1
                  AND sanction_type IN ('ad_suspension', 'suspension')
                  AND (expires_at IS NULL OR expires_at > now())
            )
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        if sanctioned {
            return Err(AdError::Ineligible(
                "Active sanction prevents rewarded ads".into(),
            ));
        }

        // 3. Check account age (>= 24 hours)
        let user_created_at: Option<OffsetDateTime> =
            sqlx::query_scalar("SELECT created_at FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await?;

        let user_created_at =
            user_created_at.ok_or_else(|| AdError::Ineligible("User not found".into()))?;

        if (now - user_created_at) < Duration::hours(24) {
            return Err(AdError::Ineligible(
                "Account must be at least 24 hours old".into(),
            ));
        }

        // 4. Check completed games (>= 5 lifetime)
        let completed_games: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*) FROM games
            WHERE (white_user_id = $1 OR black_user_id = $1)
              AND status = 'finished'
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        if completed_games < 5 {
            return Err(AdError::Ineligible(
                "Must have completed at least 5 games".into(),
            ));
        }

        // 5. Check daily quota
        let ist_date = Self::today_ist();
        let used_today: i32 = sqlx::query_scalar(
            "SELECT used FROM ad_quota_daily WHERE user_id = $1 AND ist_date = $2",
        )
        .bind(user_id)
        .bind(ist_date)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(0);

        let max_per_day = config.ads_max_per_day as i32;
        if used_today >= max_per_day {
            return Err(AdError::QuotaExceeded {
                max_per_day,
                used: used_today,
            });
        }

        // 6. Check cooldown (60s since last session)
        let last_session_created: Option<OffsetDateTime> = sqlx::query_scalar(
            "SELECT created_at FROM ad_sessions WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(last_created) = last_session_created {
            let elapsed = (now - last_created).whole_seconds();
            if elapsed >= 0 && (elapsed as u64) < config.ads_cooldown_s {
                let remaining = config.ads_cooldown_s - (elapsed as u64);
                return Err(AdError::Cooldown(remaining));
            }
        }

        // Mint session: public_id, expires in 30 minutes
        let public_id = format!("as_{}", ulid::Ulid::new());
        let expires_at = now + Duration::minutes(30);

        sqlx::query(
            r#"
            INSERT INTO ad_sessions (public_id, user_id, provider, status, created_at, expires_at)
            VALUES ($1, $2, 'admob', 'pending', $3, $4)
            "#,
        )
        .bind(&public_id)
        .bind(user_id)
        .bind(now)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;

        Ok(AdSessionResponse {
            session_id: public_id,
            provider: "admob".to_string(),
            reward_gold: config.ads_gold_per_ad,
            remaining_today: max_per_day - used_today,
            cooldown_s: config.ads_cooldown_s,
        })
    }

    /// Retrieve ad status and eligibility for a user.
    pub async fn get_status(&self, user_id: i64, config: &ServerConfig) -> Result<AdStatus, AdError> {
        let now = OffsetDateTime::now_utc();
        let ist_date = Self::today_ist();

        let in_game: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM games
                WHERE (white_user_id = $1 OR black_user_id = $1)
                  AND status IN ('created', 'active')
            )
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        let sanctioned: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM sanctions
                WHERE user_id = $1
                  AND sanction_type IN ('ad_suspension', 'suspension')
                  AND (expires_at IS NULL OR expires_at > now())
            )
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        let user_created_at: Option<OffsetDateTime> =
            sqlx::query_scalar("SELECT created_at FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await?;

        let account_age_ok = user_created_at
            .map(|ca| (now - ca) >= Duration::hours(24))
            .unwrap_or(false);

        let completed_games: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*) FROM games
            WHERE (white_user_id = $1 OR black_user_id = $1)
              AND status = 'finished'
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        let games_completed_ok = completed_games >= 5;

        let used_today: i32 = sqlx::query_scalar(
            "SELECT used FROM ad_quota_daily WHERE user_id = $1 AND ist_date = $2",
        )
        .bind(user_id)
        .bind(ist_date)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(0);

        let max_per_day = config.ads_max_per_day as i32;
        let quota_ok = used_today < max_per_day;

        let last_session_created: Option<OffsetDateTime> = sqlx::query_scalar(
            "SELECT created_at FROM ad_sessions WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        let mut cooldown_until = None;
        let mut cooldown_ok = true;
        if let Some(last_created) = last_session_created {
            let elapsed = (now - last_created).whole_seconds();
            if elapsed >= 0 && (elapsed as u64) < config.ads_cooldown_s {
                cooldown_ok = false;
                cooldown_until = Some(last_created + Duration::seconds(config.ads_cooldown_s as i64));
            }
        }

        let no_active_game = !in_game;
        let not_sanctioned = !sanctioned;
        let age_ok = true; // Users must declare 18+ to sign up

        let eligible = account_age_ok
            && games_completed_ok
            && no_active_game
            && not_sanctioned
            && quota_ok
            && cooldown_ok;

        let reason = if !quota_ok {
            Some("Daily limit reached".to_string())
        } else if !cooldown_ok {
            Some("Cooldown active".to_string())
        } else if in_game {
            Some("Active game in progress".to_string())
        } else if sanctioned {
            Some("Active sanction".to_string())
        } else if !account_age_ok {
            Some("Account under 24 hours old".to_string())
        } else if !games_completed_ok {
            Some("Complete at least 5 games".to_string())
        } else {
            None
        };

        Ok(AdStatus {
            enabled: true,
            gold_per_ad: config.ads_gold_per_ad,
            max_per_day,
            used_today,
            cooldown_until,
            eligibility: AdEligibility {
                eligible,
                reason,
                account_age_ok,
                games_completed_ok,
                no_active_game,
                not_sanctioned,
                age_ok,
            },
        })
    }

    /// Process incoming provider SSV callback (T5 transaction).
    pub async fn process_ssv_callback(
        &self,
        raw: &RawCallback,
        config: &ServerConfig,
    ) -> Result<VerifiedReward, AdError> {
        let verified = self.verifier.verify(raw)?;

        // Begin T5 transaction
        let mut tx = self.pool.begin().await?;

        // 1. Replay check: provider + transaction_id already recorded?
        let existing_event: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM ad_reward_events WHERE provider = $1 AND transaction_id = $2",
        )
        .bind(&verified.provider)
        .bind(&verified.transaction_id)
        .fetch_optional(&mut *tx)
        .await?;

        if existing_event.is_some() {
            tx.rollback().await?;
            return Ok(verified);
        }

        // 2. Fetch and lock ad_session by public_id
        let session_row = sqlx::query(
            r#"
            SELECT id, user_id, status, expires_at
            FROM ad_sessions
            WHERE public_id = $1
            FOR UPDATE
            "#,
        )
        .bind(&verified.session_id)
        .fetch_optional(&mut *tx)
        .await?;

        let session = match session_row {
            Some(row) => {
                let id: i64 = row.get("id");
                let user_id: i64 = row.get("user_id");
                let status: String = row.get("status");
                let expires_at: OffsetDateTime = row.get("expires_at");
                (id, user_id, status, expires_at)
            }
            None => {
                tx.rollback().await?;
                return Err(AdError::Reject(AdReject::SessionNotFound(
                    verified.session_id.clone(),
                )));
            }
        };

        let (session_id, session_user_id, session_status, session_expires_at) = session;

        if session_user_id != verified.user_id {
            tx.rollback().await?;
            return Err(AdError::Reject(AdReject::UserMismatch));
        }

        if session_status != "pending" {
            tx.rollback().await?;
            return Err(AdError::Reject(AdReject::SessionInvalidStatus(
                session_status,
            )));
        }

        let now = OffsetDateTime::now_utc();
        if session_expires_at <= now {
            sqlx::query("UPDATE ad_sessions SET status = 'expired' WHERE id = $1")
                .bind(session_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Err(AdError::Reject(AdReject::SessionExpired(
                verified.session_id.clone(),
            )));
        }

        // 3. Quota check inside T5 with FOR UPDATE lock on ad_quota_daily
        let ist_date = Self::today_ist();
        sqlx::query(
            r#"
            INSERT INTO ad_quota_daily (user_id, ist_date, used)
            VALUES ($1, $2, 0)
            ON CONFLICT (user_id, ist_date) DO NOTHING
            "#,
        )
        .bind(verified.user_id)
        .bind(ist_date)
        .execute(&mut *tx)
        .await?;

        let used_today: i32 = sqlx::query_scalar(
            r#"
            SELECT used FROM ad_quota_daily
            WHERE user_id = $1 AND ist_date = $2
            FOR UPDATE
            "#,
        )
        .bind(verified.user_id)
        .bind(ist_date)
        .fetch_one(&mut *tx)
        .await?;

        let max_per_day = config.ads_max_per_day as i32;
        if used_today >= max_per_day {
            sqlx::query("UPDATE ad_sessions SET status = 'rejected' WHERE id = $1")
                .bind(session_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Err(AdError::QuotaExceeded {
                max_per_day,
                used: used_today,
            });
        }

        // Increment quota used
        sqlx::query(
            "UPDATE ad_quota_daily SET used = used + 1 WHERE user_id = $1 AND ist_date = $2",
        )
        .bind(verified.user_id)
        .bind(ist_date)
        .execute(&mut *tx)
        .await?;

        // 4. Ledger grant via PgGoldLedger::apply_in_tx
        let op = LedgerOp {
            user_id: verified.user_id,
            delta: config.ads_gold_per_ad,
            reason: GoldReason::AdReward,
            ref_type: Some("ad_session".to_string()),
            ref_id: Some(verified.session_id.clone()),
            idem_key: Some(format!("ads:{}:{}", verified.provider, verified.transaction_id)),
            meta: serde_json::json!({
                "provider": verified.provider,
                "transaction_id": verified.transaction_id,
                "session_id": verified.session_id,
            }),
        };

        PgGoldLedger::apply_in_tx(&mut tx, &op)
            .await
            .map_err(|e| AdError::Ledger(e.to_string()))?;

        // 5. Insert ad_reward_events
        let meta_json = serde_json::json!({
            "raw_digest": hex::encode(verified.raw_digest),
            "session_id": verified.session_id,
        });

        sqlx::query(
            r#"
            INSERT INTO ad_reward_events (provider, transaction_id, user_id, session_id, gold_amount, verified_at, meta)
            VALUES ($1, $2, $3, $4, $5, now(), $6)
            "#,
        )
        .bind(&verified.provider)
        .bind(&verified.transaction_id)
        .bind(verified.user_id)
        .bind(session_id)
        .bind(config.ads_gold_per_ad)
        .bind(meta_json)
        .execute(&mut *tx)
        .await?;

        // 6. Mark session completed
        sqlx::query("UPDATE ad_sessions SET status = 'completed', completed_at = now() WHERE id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(verified)
    }
}

mod hex {
    pub fn encode(bytes: [u8; 32]) -> String {
        let mut s = String::with_capacity(64);
        for b in bytes {
            use std::fmt::Write;
            let _ = write!(s, "{:02x}", b);
        }
        s
    }
}
