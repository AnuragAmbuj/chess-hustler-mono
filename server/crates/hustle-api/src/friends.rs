//! Friends, Blocks & Direct Challenges routes (rest.md §2.7):
//! - `GET /v1/friends`: list accepted friends with live presence
//! - `GET /v1/friends/requests`: list pending incoming and outgoing requests
//! - `POST /v1/friends/requests`: send friend request by handle
//! - `POST /v1/friends/requests/{id}/accept`: accept friend request
//! - `POST /v1/friends/requests/{id}/decline`: decline friend request
//! - `DELETE /v1/friends/{id}`: unfriend
//! - `GET /v1/blocks`: list blocked users
//! - `POST /v1/blocks`: block user by handle
//! - `DELETE /v1/blocks/{handle}`: unblock user
//! - `POST /v1/challenge`: send direct challenge
//! - `POST /v1/challenge/{id}/accept`: accept direct challenge
//! - `POST /v1/challenge/{id}/decline`: decline direct challenge

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use hustle_friends::{
    BlockedUserItem, DirectChallenge, FriendError, FriendInfo, FriendRequestItem,
    FriendRequestsResponse,
};
use serde::Deserialize;
use serde_json::json;

use crate::auth::Authed;
use crate::error::ApiError;
use crate::AppState;

impl From<FriendError> for ApiError {
    fn from(err: FriendError) -> Self {
        match err {
            FriendError::SelfActionNotAllowed => ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation.failed",
                err.to_string(),
            ),
            FriendError::UserNotFound(_) => ApiError::not_found("not_found", err.to_string()),
            FriendError::AlreadyFriends => ApiError::conflict("conflict", err.to_string()),
            FriendError::RequestAlreadyPending => ApiError::conflict("conflict", err.to_string()),
            FriendError::BlockedByUser => ApiError::forbidden("md.blocked", err.to_string()),
            FriendError::RequestNotFound(_) => ApiError::not_found("not_found", err.to_string()),
            FriendError::ChallengeNotFound => ApiError::not_found("not_found", err.to_string()),
            FriendError::NotAuthorized => ApiError::forbidden("forbidden", err.to_string()),
            FriendError::Database(e) => ApiError::internal(e.to_string()),
            FriendError::Redis(e) => ApiError::internal(e.to_string()),
            FriendError::Json(e) => ApiError::internal(e.to_string()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct HandleRequest {
    pub handle: String,
}

#[derive(Debug, Deserialize)]
pub struct ChallengeTcInput {
    pub initial_s: u32,
    pub increment_s: u32,
}

#[derive(Debug, Deserialize)]
pub struct CreateChallengeRequest {
    pub handle: String,
    pub tc: ChallengeTcInput,
    #[serde(default)]
    pub rated: bool,
}

/// `GET /v1/friends` — list accepted friends with live presence status.
pub async fn get_friends(
    State(state): State<AppState>,
    authed: Authed,
) -> Result<Json<Vec<FriendInfo>>, ApiError> {
    let friends = state.friend_service.get_friends(authed.user_id).await?;
    Ok(Json(friends))
}

/// `GET /v1/friends/requests` — list incoming and outgoing pending friend requests.
pub async fn get_requests(
    State(state): State<AppState>,
    authed: Authed,
) -> Result<Json<FriendRequestsResponse>, ApiError> {
    let bundle = state.friend_service.get_requests(authed.user_id).await?;
    Ok(Json(bundle))
}

/// `POST /v1/friends/requests` — send friend request by handle.
pub async fn post_request(
    State(state): State<AppState>,
    authed: Authed,
    Json(body): Json<HandleRequest>,
) -> Result<(StatusCode, Json<FriendRequestItem>), ApiError> {
    let item = state
        .friend_service
        .send_request(authed.user_id, &body.handle)
        .await?;
    Ok((StatusCode::CREATED, Json(item)))
}

/// `POST /v1/friends/requests/{id}/accept` — accept friend request.
pub async fn post_accept_request(
    State(state): State<AppState>,
    authed: Authed,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .friend_service
        .accept_request(authed.user_id, id)
        .await?;
    Ok(Json(json!({ "id": id, "status": "accepted" })))
}

/// `POST /v1/friends/requests/{id}/decline` — decline friend request.
pub async fn post_decline_request(
    State(state): State<AppState>,
    authed: Authed,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .friend_service
        .decline_request(authed.user_id, id)
        .await?;
    Ok(Json(json!({ "id": id, "status": "declined" })))
}

/// `DELETE /v1/friends/{id}` — unfriend / remove friendship.
pub async fn delete_friend(
    State(state): State<AppState>,
    authed: Authed,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    state
        .friend_service
        .remove_friend(authed.user_id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/blocks` — list blocked users.
pub async fn get_blocks(
    State(state): State<AppState>,
    authed: Authed,
) -> Result<Json<Vec<BlockedUserItem>>, ApiError> {
    let blocks = state.friend_service.get_blocks(authed.user_id).await?;
    Ok(Json(blocks))
}

/// `POST /v1/blocks` — block user by handle.
pub async fn post_block(
    State(state): State<AppState>,
    authed: Authed,
    Json(body): Json<HandleRequest>,
) -> Result<(StatusCode, Json<BlockedUserItem>), ApiError> {
    let item = state
        .friend_service
        .block_user(authed.user_id, &body.handle)
        .await?;
    Ok((StatusCode::CREATED, Json(item)))
}

/// `DELETE /v1/blocks/{handle}` — unblock user by handle.
pub async fn delete_block(
    State(state): State<AppState>,
    authed: Authed,
    Path(handle): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .friend_service
        .unblock_user(authed.user_id, &handle)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/challenge` — send direct challenge.
pub async fn post_challenge(
    State(state): State<AppState>,
    authed: Authed,
    Json(body): Json<CreateChallengeRequest>,
) -> Result<(StatusCode, Json<DirectChallenge>), ApiError> {
    let challenge = state
        .challenge_service
        .create_challenge(
            authed.user_id,
            &body.handle,
            body.tc.initial_s,
            body.tc.increment_s,
            body.rated,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(challenge)))
}

/// `POST /v1/challenge/{id}/accept` — accept direct challenge.
pub async fn post_accept_challenge(
    State(state): State<AppState>,
    authed: Authed,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let game_id = state
        .challenge_service
        .accept_challenge(authed.user_id, &id)
        .await?;
    Ok(Json(json!({ "game_id": game_id })))
}

/// `POST /v1/challenge/{id}/decline` — decline direct challenge.
pub async fn post_decline_challenge(
    State(state): State<AppState>,
    authed: Authed,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .challenge_service
        .decline_challenge(authed.user_id, &id)
        .await?;
    Ok(Json(json!({ "status": "declined" })))
}
