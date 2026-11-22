//! Matchmaking endpoints (ADR-004, docs/03-api/rest.md §2.3).
//!
//! - `POST /v1/matchmaking/queue`: join queue (202 Accepted)
//! - `DELETE /v1/matchmaking/queue`: leave queue (204 No Content)
//! - `GET /v1/matchmaking/status`: current queue status

use crate::auth::Authed;
use crate::error::ApiError;
use crate::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use hustle_domain::TimeControl;
use hustle_matchmaking::QueueStatus;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Deserialize, Serialize)]
pub struct QueueJoinRequest {
    pub tc: TcRequest,
    #[serde(default)]
    pub rated: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TcRequest {
    pub initial_s: u32,
    pub increment_s: u32,
}

/// POST /v1/matchmaking/queue -> 202 Accepted
pub async fn queue_join(
    authed: Authed,
    State(state): State<AppState>,
    Json(payload): Json<QueueJoinRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if payload.tc.initial_s < 10 || payload.tc.initial_s > 7200 {
        return Err(ApiError::validation(
            "initial_s must be between 10 and 7200 seconds",
            json!(["tc.initial_s"]),
        ));
    }

    let tc = TimeControl {
        initial_ms: payload.tc.initial_s * 1000,
        increment_ms: payload.tc.increment_s * 1000,
    };

    let receipt = state
        .matchmaking
        .join_queue(authed.user_id, tc, payload.rated)
        .await?;

    let response = json!({
        "pool": receipt.pool,
        "est_wait_s": receipt.est_wait_s,
        "ratings": {
            &receipt.pool: receipt.ratings.rating,
            "rating": receipt.ratings.rating,
            "rd": receipt.ratings.rd,
        }
    });

    Ok((StatusCode::ACCEPTED, Json(response)))
}

/// DELETE /v1/matchmaking/queue -> 204 No Content
pub async fn queue_leave(
    authed: Authed,
    State(state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    state.matchmaking.leave_queue(authed.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /v1/matchmaking/status -> 200 OK
pub async fn queue_status(
    authed: Authed,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let status = state.matchmaking.get_status(authed.user_id).await?;

    let body = match status {
        QueueStatus::Queued { pool, waited_s } => json!({
            "queued": true,
            "pool": pool,
            "waited_s": waited_s,
        }),
        QueueStatus::NotQueued => json!({
            "queued": false,
        }),
    };

    Ok((StatusCode::OK, Json(body)))
}
