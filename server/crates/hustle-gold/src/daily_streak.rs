//! Daily Streak logic and ledger summary (docs/04-economy/ §2 and docs/03-api/rest.md §2.4).
//!
//! Owns daily streak claims, reset timing in Indian Standard Time (IST, Asia/Kolkata / UTC+5:30),
//! and 30-day gold ledger breakdowns.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::OffsetDateTime;

use crate::{GoldReason, GrantResult, LedgerError, LedgerOp, PgGoldLedger};

/// Indian Standard Time (Asia/Kolkata, UTC+5:30).
pub fn ist_offset() -> time::UtcOffset {
    time::UtcOffset::from_hms(5, 30, 0).expect("valid IST offset")
}

/// Convert a UTC timestamp to IST.
pub fn to_ist(utc: OffsetDateTime) -> OffsetDateTime {
    utc.to_offset(ist_offset())
}

/// Calculate the next reset timestamp in IST (midnight of tomorrow) and format as RFC 3339.
pub fn format_next_reset_ist(today_ist: time::Date) -> String {
    let tomorrow_ist = today_ist.next_day().expect("next day in range");
    let next_reset_time = tomorrow_ist
        .with_hms(0, 0, 0)
        .expect("valid midnight time")
        .assume_offset(ist_offset());
    next_reset_time
        .format(&time::format_description::well_known::Rfc3339)
        .expect("valid rfc3339")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyStreakStatus {
    pub available: bool,
    pub streak: u32,
    pub next_reset_ist: String,
    pub next_reward: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyClaimResult {
    pub claimed: bool,
    pub amount: i64,
    pub streak: u32,
    pub new_balance: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoldBreakdownItem {
    pub reason: String,
    pub amount: i64,
    pub delta: i64,
    pub total: i64,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoldSummary {
    pub balance: i64,
    pub total_earned: i64,
    pub total_spent: i64,
    pub recent_30d_breakdown: Vec<GoldBreakdownItem>,
}

/// Query current daily streak status for a user.
pub async fn get_daily_status(
    pool: &PgPool,
    user_id: i64,
    now_utc: OffsetDateTime,
) -> Result<DailyStreakStatus, LedgerError> {
    let now_ist = to_ist(now_utc);
    let today_ist = now_ist.date();
    let next_reset_ist = format_next_reset_ist(today_ist);

    let mut conn = pool.acquire().await?;
    let last_claim: Option<(time::Date, i32)> = sqlx::query_as(
        "SELECT COALESCE(claim_date_ist, ist_date) AS claim_date, streak
         FROM daily_claims
         WHERE user_id = $1
         ORDER BY COALESCE(claim_date_ist, ist_date) DESC
         LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await?;

    let yesterday_ist = today_ist.previous_day().expect("valid yesterday");

    let (available, streak, next_reward) = match last_claim {
        None => (true, 0, hustle_domain::economy::streak_reward(1)),
        Some((last_date, last_streak)) => {
            if last_date == today_ist {
                // Already claimed today: not available; streak is current; next reward is for tomorrow
                let next_pos = (last_streak as u32 % 7) + 1;
                (
                    false,
                    last_streak as u32,
                    hustle_domain::economy::streak_reward(next_pos),
                )
            } else if last_date == yesterday_ist {
                // Claimed yesterday, available to claim today: streak is current progress
                let next_pos = (last_streak as u32 % 7) + 1;
                (
                    true,
                    last_streak as u32,
                    hustle_domain::economy::streak_reward(next_pos),
                )
            } else {
                // Streak broken (missed day)
                (true, 0, hustle_domain::economy::streak_reward(1))
            }
        }
    };

    Ok(DailyStreakStatus {
        available,
        streak,
        next_reset_ist,
        next_reward,
    })
}

/// Claim daily streak reward for today (IST). Idempotent for repeated claims on the same IST day.
pub async fn claim_daily_streak(
    pool: &PgPool,
    user_id: i64,
    now_utc: OffsetDateTime,
) -> Result<DailyClaimResult, LedgerError> {
    let now_ist = to_ist(now_utc);
    let today_ist = now_ist.date();
    let yesterday_ist = today_ist.previous_day().expect("valid yesterday");

    let mut tx = pool.begin().await?;

    // Ensure balance row exists and lock it for serialization
    PgGoldLedger::ensure_row(&mut tx, user_id).await?;
    let current_balance: i64 =
        sqlx::query_scalar("SELECT balance FROM gold_balances WHERE user_id = $1 FOR UPDATE")
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;

    // Query last claim under transaction
    let last_claim: Option<(time::Date, i32)> = sqlx::query_as(
        "SELECT COALESCE(claim_date_ist, ist_date) AS claim_date, streak
         FROM daily_claims
         WHERE user_id = $1
         ORDER BY COALESCE(claim_date_ist, ist_date) DESC
         LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some((last_date, last_streak)) = last_claim {
        if last_date == today_ist {
            // Already claimed today (idempotent replay)
            tx.commit().await?;
            return Ok(DailyClaimResult {
                claimed: false,
                amount: 0,
                streak: last_streak as u32,
                new_balance: current_balance,
            });
        }
    }

    // Determine new streak position (wraps 1..=7)
    let next_streak = match last_claim {
        Some((last_date, last_streak)) if last_date == yesterday_ist => {
            (last_streak as u32 % 7) + 1
        }
        _ => 1,
    };

    let reward_amount = hustle_domain::economy::streak_reward(next_streak);

    // Insert into daily_claims with unique constraint
    let inserted: Option<i32> = sqlx::query_scalar(
        "INSERT INTO daily_claims (user_id, ist_date, claim_date_ist, streak, claimed_at)
         VALUES ($1, $2, $2, $3, now())
         ON CONFLICT (user_id, claim_date_ist) DO NOTHING
         RETURNING streak",
    )
    .bind(user_id)
    .bind(today_ist)
    .bind(next_streak as i32)
    .fetch_optional(&mut *tx)
    .await?;

    if inserted.is_none() {
        // Concurrent claim already landed
        tx.commit().await?;
        let ledger = PgGoldLedger::new(pool.clone());
        let bal = ledger.balance_of(user_id).await?;
        return Ok(DailyClaimResult {
            claimed: false,
            amount: 0,
            streak: next_streak,
            new_balance: bal,
        });
    }

    // Credit gold via PgGoldLedger with reason "daily_streak"
    let idem_key = format!("daily:{}:{}", user_id, today_ist);
    let op = LedgerOp {
        user_id,
        delta: reward_amount,
        reason: GoldReason::DailyStreak,
        ref_type: Some("daily_claim".to_string()),
        ref_id: Some(today_ist.to_string()),
        idem_key: Some(idem_key),
        meta: serde_json::json!({
            "streak": next_streak,
            "claim_date_ist": today_ist.to_string(),
        }),
    };

    let grant_res = PgGoldLedger::apply_in_tx(&mut tx, &op).await?;
    let new_balance = match grant_res {
        GrantResult::Applied { balance_after, .. } => balance_after,
        GrantResult::Duplicate { balance_after } => balance_after,
    };

    tx.commit().await?;

    Ok(DailyClaimResult {
        claimed: true,
        amount: reward_amount,
        streak: next_streak,
        new_balance,
    })
}

/// Retrieve gold summary, totals, and 30-day breakdown for a user.
pub async fn get_gold_summary(pool: &PgPool, user_id: i64) -> Result<GoldSummary, LedgerError> {
    let mut conn = pool.acquire().await?;

    let balance: Option<i64> =
        sqlx::query_scalar("SELECT balance FROM gold_balances WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&mut *conn)
            .await?;
    let balance = balance.unwrap_or(0);

    let total_earned: Option<i64> = sqlx::query_scalar(
        "SELECT COALESCE(SUM(delta)::BIGINT, 0::BIGINT) FROM gold_ledger WHERE user_id = $1 AND delta > 0",
    )
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await?;
    let total_earned = total_earned.unwrap_or(0);

    let total_spent: Option<i64> = sqlx::query_scalar(
        "SELECT COALESCE(ABS(SUM(delta))::BIGINT, 0::BIGINT) FROM gold_ledger WHERE user_id = $1 AND delta < 0",
    )
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await?;
    let total_spent = total_spent.unwrap_or(0);

    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT reason, COALESCE(SUM(delta)::BIGINT, 0::BIGINT) AS amount, COUNT(*)::BIGINT AS count
         FROM gold_ledger
         WHERE user_id = $1 AND created_at >= now() - INTERVAL '30 days'
         GROUP BY reason
         ORDER BY amount DESC",
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?;

    let recent_30d_breakdown = rows
        .into_iter()
        .map(|(reason, amount, count)| GoldBreakdownItem {
            reason,
            amount,
            delta: amount,
            total: amount,
            count,
        })
        .collect();

    Ok(GoldSummary {
        balance,
        total_earned,
        total_spent,
        recent_30d_breakdown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ist_offset_is_plus_five_thirty() {
        let offset = ist_offset();
        assert_eq!(offset.whole_hours(), 5);
        assert_eq!(offset.minutes_past_hour(), 30);
    }

    #[test]
    fn next_reset_ist_formats_rfc3339_at_midnight() {
        let date = time::Date::from_calendar_date(2026, time::Month::November, 17).unwrap();
        let formatted = format_next_reset_ist(date);
        assert_eq!(formatted, "2026-11-18T00:00:00+05:30");
    }

    #[test]
    fn streak_ladder_wraps_at_seven() {
        assert_eq!(hustle_domain::economy::streak_reward(1), 500);
        assert_eq!(hustle_domain::economy::streak_reward(2), 750);
        assert_eq!(hustle_domain::economy::streak_reward(3), 1000);
        assert_eq!(hustle_domain::economy::streak_reward(4), 1500);
        assert_eq!(hustle_domain::economy::streak_reward(5), 2000);
        assert_eq!(hustle_domain::economy::streak_reward(6), 3000);
        assert_eq!(hustle_domain::economy::streak_reward(7), 5000);
        assert_eq!(hustle_domain::economy::streak_reward(8), 500);
    }
}
