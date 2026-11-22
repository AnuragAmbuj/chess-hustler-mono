use axum::{
    extract::{Path, RawQuery, State},
    http::StatusCode,
    Json,
};
use hustle_ads::{AdError, AdReject, AdSessionResponse, AdStatus, RawCallback};
use serde_json::json;

use crate::{auth::Authed, error::ApiError, AppState};

pub async fn create_ad_session(
    State(state): State<AppState>,
    authed: Authed,
) -> Result<Json<AdSessionResponse>, ApiError> {
    match state.ads_service.create_session(authed.user_id, &state.config).await {
        Ok(session) => Ok(Json(session)),
        Err(AdError::InActiveGame) => Err(ApiError::conflict(
            "ads.in_active_game",
            "Cannot request rewarded ad while in an active game",
        )),
        Err(AdError::Ineligible(reason)) => Err(ApiError::forbidden(
            "ads.ineligible",
            reason,
        )),
        Err(AdError::QuotaExceeded { max_per_day, used }) => {
            Err(ApiError::new(
                StatusCode::CONFLICT,
                "ads.quota_exceeded",
                "Daily rewarded-ad limit reached",
            )
            .with_details(json!({
                "used_today": used,
                "max_per_day": max_per_day,
            })))
        }
        Err(AdError::Cooldown(remaining)) => {
            Err(ApiError::new(
                StatusCode::CONFLICT,
                "ads.cooldown",
                format!("Ad cooldown active; wait {remaining}s"),
            )
            .with_details(json!({ "cooldown_remaining_s": remaining })))
        }
        Err(AdError::Reject(r)) => Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "ads.rejected",
            r.to_string(),
        )),
        Err(AdError::Database(e)) => Err(ApiError::internal(e.to_string())),
        Err(AdError::Ledger(e)) => Err(ApiError::internal(e)),
    }
}

pub async fn get_ad_status(
    State(state): State<AppState>,
    authed: Authed,
) -> Result<Json<AdStatus>, ApiError> {
    match state.ads_service.get_status(authed.user_id, &state.config).await {
        Ok(status) => Ok(Json(status)),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

pub async fn handle_ad_ssv_callback(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    raw_query: RawQuery,
) -> Result<Json<serde_json::Value>, ApiError> {
    if provider != "admob" {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "ads.unsupported_provider",
            format!("Provider '{provider}' is not supported"),
        ));
    }

    let query_string = raw_query.0.unwrap_or_default();
    let raw = RawCallback::from_query_string(&query_string);

    match state.ads_service.process_ssv_callback(&raw, &state.config).await {
        Ok(verified) => Ok(Json(json!({
            "status": "ok",
            "transaction_id": verified.transaction_id,
        }))),
        Err(AdError::Reject(r)) => match r {
            AdReject::InvalidSignature => Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "ads.invalid_signature",
                "Signature verification failed",
            )),
            AdReject::ExpiredTimestamp(msg) => Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "ads.expired_timestamp",
                msg,
            )),
            AdReject::UnknownKeyId(key_id) => Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "ads.unknown_key_id",
                format!("Key ID '{key_id}' not found"),
            )),
            AdReject::MalformedCallback(msg) => Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "ads.malformed_callback",
                msg,
            )),
            AdReject::SessionNotFound(session_id) => Err(ApiError::new(
                StatusCode::NOT_FOUND,
                "ads.session_not_found",
                format!("Session '{session_id}' not found"),
            )),
            AdReject::SessionExpired(session_id) => Err(ApiError::new(
                StatusCode::CONFLICT,
                "ads.session_expired",
                format!("Session '{session_id}' expired"),
            )),
            AdReject::SessionInvalidStatus(status) => Err(ApiError::new(
                StatusCode::CONFLICT,
                "ads.session_invalid_status",
                format!("Session is already '{status}'"),
            )),
            AdReject::UserMismatch => Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "ads.user_mismatch",
                "Callback user ID does not match session user ID",
            )),
            AdReject::Replay => Ok(Json(json!({
                "status": "ok",
                "message": "replay ignored",
            }))),
        },
        Err(AdError::QuotaExceeded { max_per_day, used }) => {
            Err(ApiError::new(
                StatusCode::CONFLICT,
                "ads.quota_exceeded",
                "Daily quota exceeded at callback verification time",
            )
            .with_details(json!({ "used_today": used, "max_per_day": max_per_day })))
        }
        Err(AdError::Database(e)) => Err(ApiError::internal(e.to_string())),
        Err(AdError::Ledger(e)) => Err(ApiError::internal(e)),
        Err(other) => Err(ApiError::internal(other.to_string())),
    }
}
