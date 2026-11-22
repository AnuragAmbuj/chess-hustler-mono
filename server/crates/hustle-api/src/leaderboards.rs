//! Leaderboards route (PRD §5.5, RA-3):
//! - `GET /v1/leaderboards`: snapshot of top ranked players per pool

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use hustle_domain::Pool;
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiError;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct LeaderboardQuery {
    pub pool: Option<String>,
    pub limit: Option<i64>,
}

/// `GET /v1/leaderboards` — returns top ranked players for a pool (defaults to blitz).
pub async fn get_leaderboard(
    State(state): State<AppState>,
    Query(query): Query<LeaderboardQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool_str = query.pool.as_deref().unwrap_or("blitz");
    let pool = Pool::parse(pool_str).ok_or_else(|| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid_pool",
            "pool must be bullet, blitz, rapid, or classical",
        )
    })?;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);

    let rows = hustle_ratings::get_leaderboard(&state.pool, pool, limit)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "leaderboard query failed");
            ApiError::internal("leaderboard query failed")
        })?;

    let entries: Vec<serde_json::Value> = rows
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            json!({
                "rank": (i + 1) as i64,
                "user_id": r.public_id.to_string(),
                "handle": r.handle,
                "display_name": r.display_name.unwrap_or(r.handle),
                "rating": r.rating.round() as i64,
                "rd": r.rd,
                "peak_rating": r.peak.round() as i64,
                "games_count": r.games_count,
                "win_count": r.wins,
                "loss_count": r.losses,
                "draw_count": r.draws,
            })
        })
        .collect();

    Ok(Json(json!({
        "pool": pool.as_str(),
        "updated_at": time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_default(),
        "entries": entries,
    })))
}
