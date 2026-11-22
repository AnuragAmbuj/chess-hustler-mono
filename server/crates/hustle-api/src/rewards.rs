//! Rewards REST endpoints (rest.md §2.4):
//! - `GET /v1/me/daily`: returns `{ available: bool, streak: u32, next_reset_ist: String, next_reward: i64 }`
//! - `POST /v1/me/daily/claim`: returns `{ claimed: bool, amount: i64, streak: u32, new_balance: i64 }`. Idempotent for same IST day.
//! - `GET /v1/me/gold/summary`: returns `{ balance: i64, total_earned: i64, total_spent: i64, recent_30d_breakdown: [...] }`

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use hustle_rewards::{AchievementItem, ClaimMissionResult, MissionItem, RewardError};
use serde_json::json;
use time::OffsetDateTime;

use crate::auth::Authed;
use crate::error::ApiError;
use crate::AppState;

impl From<RewardError> for ApiError {
    fn from(err: RewardError) -> Self {
        match err {
            RewardError::MissionNotFound(_) => ApiError::not_found("not_found", err.to_string()),
            RewardError::NotCompleted { .. } => ApiError::new(
                StatusCode::BAD_REQUEST,
                "reward.not_completed",
                err.to_string(),
            ),
            RewardError::AlreadyClaimed(_) => {
                ApiError::conflict("reward.already_claimed", err.to_string())
            }
            RewardError::Db(e) => ApiError::internal(e.to_string()),
            RewardError::Ledger(e) => ApiError::internal(e.to_string()),
        }
    }
}

/// `GET /v1/me/daily` — daily streak status, next reset in IST, next reward.
pub async fn get_daily(
    State(state): State<AppState>,
    authed: Authed,
) -> Result<Json<serde_json::Value>, ApiError> {
    let now = OffsetDateTime::now_utc();
    let status = state
        .ledger
        .daily_status(authed.user_id, now)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, user = authed.user_id, "failed to fetch daily streak status");
            ApiError::internal("failed to fetch daily streak status")
        })?;

    Ok(Json(json!({
        "available": status.available,
        "streak": status.streak,
        "next_reset_ist": status.next_reset_ist,
        "next_reward": status.next_reward,
    })))
}

/// `POST /v1/me/daily/claim` — claim daily streak reward for today (IST). Idempotent for same IST day.
pub async fn claim_daily(
    State(state): State<AppState>,
    authed: Authed,
) -> Result<Json<serde_json::Value>, ApiError> {
    let now = OffsetDateTime::now_utc();
    let claim = state
        .ledger
        .claim_daily(authed.user_id, now)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, user = authed.user_id, "failed to claim daily streak");
            ApiError::internal("failed to claim daily streak")
        })?;

    Ok(Json(json!({
        "claimed": claim.claimed,
        "amount": claim.amount,
        "streak": claim.streak,
        "new_balance": claim.new_balance,
    })))
}

/// `GET /v1/me/gold/summary` — gold balance and 30-day breakdown for transparency view.
pub async fn get_gold_summary(
    State(state): State<AppState>,
    authed: Authed,
) -> Result<Json<serde_json::Value>, ApiError> {
    let summary = state.ledger.summary(authed.user_id).await.map_err(|e| {
        tracing::error!(error = %e, user = authed.user_id, "failed to fetch gold summary");
        ApiError::internal("failed to fetch gold summary")
    })?;

    Ok(Json(json!({
        "balance": summary.balance,
        "total_earned": summary.total_earned,
        "total_spent": summary.total_spent,
        "recent_30d_breakdown": summary.recent_30d_breakdown,
    })))
}

/// `GET /v1/me/missions` — active daily and weekly missions with current progress.
pub async fn get_my_missions(
    State(state): State<AppState>,
    authed: Authed,
) -> Result<Json<Vec<MissionItem>>, ApiError> {
    let now = OffsetDateTime::now_utc();
    let missions = state
        .reward_service
        .get_user_missions(authed.user_id, now)
        .await?;
    Ok(Json(missions))
}

/// `POST /v1/me/missions/{id}/claim` — claim a completed mission reward.
pub async fn claim_my_mission(
    State(state): State<AppState>,
    authed: Authed,
    Path(id): Path<String>,
) -> Result<Json<ClaimMissionResult>, ApiError> {
    let now = OffsetDateTime::now_utc();
    let result = state
        .reward_service
        .claim_mission(authed.user_id, &id, now)
        .await?;
    Ok(Json(result))
}

/// `GET /v1/me/achievements` — milestone achievements list with progress and unlock status.
pub async fn get_my_achievements(
    State(state): State<AppState>,
    authed: Authed,
) -> Result<Json<Vec<AchievementItem>>, ApiError> {
    let achievements = state
        .reward_service
        .get_user_achievements(authed.user_id)
        .await?;
    Ok(Json(achievements))
}
