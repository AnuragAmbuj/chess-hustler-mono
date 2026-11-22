//! Moderation & In-Game Chat routes (rest.md §2.9):
//! - `POST /v1/reports`: submit player report
//! - `GET /v1/grievance`: public IT-Rules 2021 Grievance Officer details
//! - `GET /v1/games/{id}/chat`: get game chat history
//! - `POST /v1/games/{id}/chat`: send in-game chat message

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use hustle_moderation::{
    ChatMessageItem, CreateReportRequest, GrievanceInfo, ModerationError, ReportItem,
};
use serde::Deserialize;

use crate::auth::Authed;
use crate::error::ApiError;
use crate::AppState;

impl From<ModerationError> for ApiError {
    fn from(err: ModerationError) -> Self {
        match err {
            ModerationError::MessageTooLong => ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation.failed",
                err.to_string(),
            ),
            ModerationError::RateLimited(_) => ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                err.to_string(),
            ),
            ModerationError::Muted => ApiError::forbidden("chat.muted", err.to_string()),
            ModerationError::NotInGame => ApiError::forbidden("forbidden", err.to_string()),
            ModerationError::UserNotFound(_) => ApiError::not_found("not_found", err.to_string()),
            ModerationError::GameNotFound(_) => ApiError::not_found("not_found", err.to_string()),
            ModerationError::Blocked => ApiError::forbidden("md.blocked", err.to_string()),
            ModerationError::InvalidCategory(_) => ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation.failed",
                err.to_string(),
            ),
            ModerationError::Database(e) => ApiError::internal(e.to_string()),
            ModerationError::Redis(e) => ApiError::internal(e.to_string()),
            _ => ApiError::internal(err.to_string()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SendChatRequest {
    pub text: String,
}

/// `POST /v1/reports` — submit report against another user.
pub async fn post_report(
    State(state): State<AppState>,
    authed: Authed,
    Json(body): Json<CreateReportRequest>,
) -> Result<(StatusCode, Json<ReportItem>), ApiError> {
    let report = state
        .moderation_service
        .submit_report(authed.user_id, body)
        .await?;
    Ok((StatusCode::CREATED, Json(report)))
}

/// `GET /v1/grievance` — public India IT Rules 2021 Grievance Officer information.
pub async fn get_grievance(State(state): State<AppState>) -> Json<GrievanceInfo> {
    Json(state.moderation_service.get_grievance_info())
}

/// `GET /v1/games/{id}/chat` — get chat history for game.
pub async fn get_game_chat(
    State(state): State<AppState>,
    authed: Authed,
    Path(id): Path<String>,
) -> Result<Json<Vec<ChatMessageItem>>, ApiError> {
    let chat = state
        .moderation_service
        .get_game_chat(authed.user_id, &id, 50)
        .await?;
    Ok(Json(chat))
}

/// `POST /v1/games/{id}/chat` — send chat message in game.
pub async fn post_game_chat(
    State(state): State<AppState>,
    authed: Authed,
    Path(id): Path<String>,
    Json(body): Json<SendChatRequest>,
) -> Result<(StatusCode, Json<ChatMessageItem>), ApiError> {
    let msg = state
        .moderation_service
        .send_chat(authed.user_id, &id, &body.text)
        .await?;
    Ok((StatusCode::CREATED, Json(msg)))
}
