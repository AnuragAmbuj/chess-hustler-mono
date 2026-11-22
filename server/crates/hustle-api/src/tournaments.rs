use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use hustle_tournaments::{
    CreateTournamentRequest, Tournament, TournamentDetail, TournamentEntry, TournamentError,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    auth::Authed,
    error::ApiError,
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct TournamentListQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn get_tournaments(
    State(state): State<AppState>,
    Query(query): Query<TournamentListQuery>,
) -> Result<Json<Vec<Tournament>>, ApiError> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);
    let tournaments = state
        .tournament_service
        .list_tournaments(query.status.as_deref(), limit, offset)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(tournaments))
}

pub async fn get_tournament_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TournamentDetail>, ApiError> {
    let detail = state
        .tournament_service
        .get_tournament_detail(&id, None)
        .await
        .map_err(|e| match e {
            TournamentError::NotFound(id) => ApiError::not_found("tn.not_found", format!("Tournament '{id}' not found")),
            other => ApiError::internal(other.to_string()),
        })?;

    Ok(Json(detail))
}

pub async fn post_join_tournament(
    State(state): State<AppState>,
    Path(id): Path<String>,
    authed: Authed,
) -> Result<Json<TournamentEntry>, ApiError> {
    let entry = state
        .tournament_service
        .join_tournament(&id, authed.user_id)
        .await
        .map_err(|e| match e {
            TournamentError::NotFound(id) => ApiError::not_found("tn.not_found", format!("Tournament '{id}' not found")),
            TournamentError::Full => ApiError::conflict("tn.full", "Tournament capacity reached"),
            TournamentError::RegistrationClosed => ApiError::conflict("tn.closed", "Registration has closed for this tournament"),
            TournamentError::InvalidState(msg) => ApiError::conflict("tn.invalid_state", msg),
            other => ApiError::internal(other.to_string()),
        })?;

    Ok(Json(entry))
}

pub async fn post_withdraw_tournament(
    State(state): State<AppState>,
    Path(id): Path<String>,
    authed: Authed,
) -> Result<StatusCode, ApiError> {
    state
        .tournament_service
        .withdraw_tournament(&id, authed.user_id)
        .await
        .map_err(|e| match e {
            TournamentError::NotFound(id) => ApiError::not_found("tn.not_found", format!("Tournament '{id}' not found")),
            TournamentError::NotJoined => ApiError::not_found("tn.not_joined", "Not registered in this tournament"),
            TournamentError::InvalidState(msg) => ApiError::conflict("tn.invalid_state", msg),
            other => ApiError::internal(other.to_string()),
        })?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn post_create_tournament(
    State(state): State<AppState>,
    authed: Authed,
    Json(req): Json<CreateTournamentRequest>,
) -> Result<(StatusCode, Json<Tournament>), ApiError> {
    if authed.role != "admin" {
        return Err(ApiError::forbidden(
            "auth.forbidden",
            "Admin role required to create tournaments",
        ));
    }

    let tournament = state
        .tournament_service
        .create_tournament(authed.user_id, req)
        .await
        .map_err(|e| match e {
            TournamentError::Validation(msg) => ApiError::validation(msg, json!({})),
            other => ApiError::internal(other.to_string()),
        })?;

    Ok((StatusCode::CREATED, Json(tournament)))
}
