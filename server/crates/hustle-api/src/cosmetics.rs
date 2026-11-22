//! Cosmetics & Loadout endpoints (rest.md §2.6).

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::Json;
use hustle_cosmetics::{
    CosmeticItem, CosmeticsError, PurchaseResult, UpdateLoadoutRequest, UserLoadout,
};
use serde::Deserialize;

use crate::auth::{Authed, OptionalAuthed};
use crate::error::ApiError;
use crate::AppState;

impl From<CosmeticsError> for ApiError {
    fn from(err: CosmeticsError) -> Self {
        match err {
            CosmeticsError::NotFound(_) => ApiError::not_found("not_found", err.to_string()),
            CosmeticsError::Unavailable(_) => ApiError::new(
                StatusCode::BAD_REQUEST,
                "cosmetic.unavailable",
                err.to_string(),
            ),
            CosmeticsError::AlreadyOwned(_) => {
                ApiError::conflict("cosmetic.already_owned", err.to_string())
            }
            CosmeticsError::InsufficientFunds { .. } => ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "economy.insufficient_balance",
                err.to_string(),
            ),
            CosmeticsError::NotOwned(_) => {
                ApiError::forbidden("cosmetic.not_owned", err.to_string())
            }
            CosmeticsError::InvalidCategory { .. } => ApiError::new(
                StatusCode::BAD_REQUEST,
                "cosmetic.invalid_category",
                err.to_string(),
            ),
            CosmeticsError::Ledger(hustle_gold::LedgerError::Insufficient { .. }) => ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "economy.insufficient_balance",
                err.to_string(),
            ),
            CosmeticsError::Ledger(e) => ApiError::internal(e.to_string()),
            CosmeticsError::Db(e) => ApiError::internal(e.to_string()),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct CatalogQuery {
    pub category: Option<String>,
}

/// `GET /v1/cosmetics` — catalog of active, available cosmetics.
pub async fn get_cosmetics(
    State(state): State<AppState>,
    OptionalAuthed(authed): OptionalAuthed,
    Query(query): Query<CatalogQuery>,
) -> Result<Json<Vec<CosmeticItem>>, ApiError> {
    let user_id = authed.map(|a| a.user_id);
    let items = state
        .cosmetics_service
        .list_catalog(user_id, query.category.as_deref())
        .await?;
    Ok(Json(items))
}

/// `GET /v1/cosmetics/{id}` — detailed item status with ownership check.
pub async fn get_cosmetic_item(
    State(state): State<AppState>,
    OptionalAuthed(authed): OptionalAuthed,
    Path(id): Path<String>,
) -> Result<Json<CosmeticItem>, ApiError> {
    let user_id = authed.map(|a| a.user_id);
    let item = state.cosmetics_service.get_item(user_id, &id).await?;
    Ok(Json(item))
}

/// `POST /v1/cosmetics/{id}/purchase` — purchase item; idempotent atomic ledger transaction.
pub async fn post_purchase_cosmetic(
    State(state): State<AppState>,
    authed: Authed,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<(HeaderMap, Json<PurchaseResult>), ApiError> {
    let idem_key = headers
        .get("Idempotency-Key")
        .or_else(|| headers.get("idempotency-key"))
        .and_then(|v| v.to_str().ok());

    let res = state
        .cosmetics_service
        .purchase(authed.user_id, &id, idem_key)
        .await?;

    let mut resp_headers = HeaderMap::new();
    if res.replayed {
        resp_headers.insert("Idempotent-Replay", HeaderValue::from_static("true"));
    }

    Ok((resp_headers, Json(res)))
}

/// `GET /v1/me/loadout` — currently equipped cosmetics.
pub async fn get_my_loadout(
    State(state): State<AppState>,
    authed: Authed,
) -> Result<Json<UserLoadout>, ApiError> {
    let loadout = state.cosmetics_service.get_loadout(authed.user_id).await?;
    Ok(Json(loadout))
}

/// `PUT /v1/me/loadout` — equip owned cosmetics.
pub async fn put_my_loadout(
    State(state): State<AppState>,
    authed: Authed,
    Json(req): Json<UpdateLoadoutRequest>,
) -> Result<Json<UserLoadout>, ApiError> {
    let loadout = state
        .cosmetics_service
        .update_loadout(authed.user_id, req)
        .await?;
    Ok(Json(loadout))
}
