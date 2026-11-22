//! Admin endpoints (docs/03-api/rest.md §2.11).
//!
//! Role-guarded: requires `admin` or `moderator` role.
//! Every mutation is audit-logged in `admin_audit_log`.

use crate::auth::Authed;
use crate::error::ApiError;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use hustle_moderation::{
    ActionReportRequest, AdminAuditItem, AdminPlayerDetail, AnomalyFlagItem, CreateSanctionRequest,
    ReportItem, ResolveFlagRequest, SanctionItem,
};
use serde::{Deserialize, Serialize};

fn require_admin_or_moderator(authed: &Authed) -> Result<(), ApiError> {
    if authed.role != "admin" && authed.role != "moderator" {
        return Err(ApiError::forbidden(
            "auth.forbidden",
            "Admin or moderator role required",
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct FlagsQuery {
    pub status: Option<String>,
    pub kind: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ReportsQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// `GET /v1/admin/flags` — Cheat detector anomaly queue.
pub async fn get_admin_flags(
    State(state): State<AppState>,
    authed: Authed,
    Query(query): Query<FlagsQuery>,
) -> Result<Json<Vec<AnomalyFlagItem>>, ApiError> {
    require_admin_or_moderator(&authed)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);

    let items = state
        .moderation_service
        .list_flags(
            query.status.as_deref(),
            query.kind.as_deref(),
            limit,
            offset,
        )
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(items))
}

/// `POST /v1/admin/flags/{id}/resolve` — Moderator decision on anomaly flag.
pub async fn post_admin_resolve_flag(
    State(state): State<AppState>,
    authed: Authed,
    Path(id): Path<i64>,
    Json(req): Json<ResolveFlagRequest>,
) -> Result<Json<AnomalyFlagItem>, ApiError> {
    require_admin_or_moderator(&authed)?;
    let actor = format!("{}:{}", authed.role, authed.user_id);

    let item = state
        .moderation_service
        .resolve_flag(id, &actor, &req)
        .await
        .map_err(|e| match e {
            hustle_moderation::ModerationError::NotFound(m) => {
                ApiError::not_found("admin.not_found", m)
            }
            hustle_moderation::ModerationError::InvalidStatus(m) => {
                ApiError::validation(m, serde_json::json!({}))
            }
            _ => ApiError::internal(e.to_string()),
        })?;

    Ok(Json(item))
}

/// `GET /v1/admin/players/{id}` — Full player inspection.
pub async fn get_admin_player(
    State(state): State<AppState>,
    authed: Authed,
    Path(id): Path<String>,
) -> Result<Json<AdminPlayerDetail>, ApiError> {
    require_admin_or_moderator(&authed)?;

    let detail = state
        .moderation_service
        .get_player_admin_detail(&id)
        .await
        .map_err(|e| match e {
            hustle_moderation::ModerationError::UserNotFound(m) => {
                ApiError::not_found("admin.user_not_found", m)
            }
            _ => ApiError::internal(e.to_string()),
        })?;

    Ok(Json(detail))
}

/// `POST /v1/admin/players/{id}/sanctions` — Apply sanction on a user.
pub async fn post_admin_player_sanction(
    State(state): State<AppState>,
    authed: Authed,
    Path(id): Path<String>,
    Json(req): Json<CreateSanctionRequest>,
) -> Result<Json<SanctionItem>, ApiError> {
    require_admin_or_moderator(&authed)?;
    let actor = format!("{}:{}", authed.role, authed.user_id);

    let user_id: i64 = if let Ok(uid) = id.parse::<i64>() {
        uid
    } else {
        let uid_opt: Option<i64> = sqlx::query_scalar(
            "SELECT u.id FROM users u LEFT JOIN profiles p ON p.user_id = u.id WHERE u.public_id = $1 OR p.handle = $1"
        )
        .bind(&id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

        uid_opt.ok_or_else(|| ApiError::not_found("admin.user_not_found", "Player not found"))?
    };

    let item = state
        .moderation_service
        .create_sanction(user_id, &actor, &req)
        .await
        .map_err(|e| match e {
            hustle_moderation::ModerationError::InvalidKind(m) => {
                ApiError::validation(m, serde_json::json!({}))
            }
            hustle_moderation::ModerationError::Validation(m) => {
                ApiError::validation(m, serde_json::json!({}))
            }
            _ => ApiError::internal(e.to_string()),
        })?;

    Ok(Json(item))
}

/// `GET /v1/admin/reports` — Moderation queue ordered by status/urgency.
pub async fn get_admin_reports(
    State(state): State<AppState>,
    authed: Authed,
    Query(query): Query<ReportsQuery>,
) -> Result<Json<Vec<ReportItem>>, ApiError> {
    require_admin_or_moderator(&authed)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);

    let items = state
        .moderation_service
        .list_reports(query.status.as_deref(), limit, offset)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(items))
}

/// `POST /v1/admin/reports/{id}/action` — Action or dismiss a user report.
pub async fn post_admin_action_report(
    State(state): State<AppState>,
    authed: Authed,
    Path(id): Path<i64>,
    Json(req): Json<ActionReportRequest>,
) -> Result<Json<ReportItem>, ApiError> {
    require_admin_or_moderator(&authed)?;
    let actor = format!("{}:{}", authed.role, authed.user_id);

    let item = state
        .moderation_service
        .action_report(id, &actor, &req)
        .await
        .map_err(|e| match e {
            hustle_moderation::ModerationError::NotFound(m) => {
                ApiError::not_found("admin.not_found", m)
            }
            hustle_moderation::ModerationError::InvalidStatus(m) => {
                ApiError::validation(m, serde_json::json!({}))
            }
            _ => ApiError::internal(e.to_string()),
        })?;

    Ok(Json(item))
}

/// `GET /v1/admin/audit` — Admin audit log feed.
pub async fn get_admin_audit(
    State(state): State<AppState>,
    authed: Authed,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AdminAuditItem>>, ApiError> {
    require_admin_or_moderator(&authed)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);

    let items = state
        .moderation_service
        .get_audit_logs(limit, offset)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(items))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigItem {
    pub key: String,
    pub value: serde_json::Value,
    pub updated_by: String,
    pub updated_at: String,
    pub version: i32,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub value: serde_json::Value,
    pub version: i32,
}

/// `GET /v1/admin/config/{key}` — Dynamic app configuration knob.
pub async fn get_admin_config(
    State(state): State<AppState>,
    authed: Authed,
    Path(key): Path<String>,
) -> Result<Json<AppConfigItem>, ApiError> {
    require_admin_or_moderator(&authed)?;

    let row: Option<(String, serde_json::Value, String, time::OffsetDateTime, i32)> =
        sqlx::query_as(
            "SELECT key, value, updated_by, updated_at, version FROM app_config WHERE key = $1",
        )
        .bind(&key)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let (key, value, updated_by, updated_at, version) = row.ok_or_else(|| {
        ApiError::not_found(
            "admin.config_not_found",
            format!("Configuration key '{key}' not found"),
        )
    })?;

    Ok(Json(AppConfigItem {
        key,
        value,
        updated_by,
        updated_at: updated_at
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
        version,
    }))
}

/// `PUT /v1/admin/config/{key}` — Update dynamic app configuration knob with optimistic locking.
pub async fn put_admin_config(
    State(state): State<AppState>,
    authed: Authed,
    Path(key): Path<String>,
    Json(req): Json<UpdateConfigRequest>,
) -> Result<Json<AppConfigItem>, ApiError> {
    require_admin_or_moderator(&authed)?;
    let actor = format!("{}:{}", authed.role, authed.user_id);

    let updated: Option<(String, serde_json::Value, String, time::OffsetDateTime, i32)> =
        sqlx::query_as(
            r#"
        UPDATE app_config
        SET value = $1, updated_by = $2, updated_at = now(), version = version + 1
        WHERE key = $3 AND version = $4
        RETURNING key, value, updated_by, updated_at, version
        "#,
        )
        .bind(&req.value)
        .bind(&actor)
        .bind(&key)
        .bind(req.version)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    if let Some((key, value, updated_by, updated_at, new_version)) = updated {
        let _ = state
            .moderation_service
            .record_audit_log(
                &actor,
                "update_config",
                Some("app_config"),
                Some(&key),
                serde_json::json!({
                    "key": key,
                    "old_version": req.version,
                    "new_version": new_version,
                    "value": value,
                }),
            )
            .await;

        return Ok(Json(AppConfigItem {
            key,
            value,
            updated_by,
            updated_at: updated_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
            version: new_version,
        }));
    }

    // Check why update failed
    let existing_version: Option<i32> =
        sqlx::query_scalar("SELECT version FROM app_config WHERE key = $1")
            .bind(&key)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;

    match existing_version {
        Some(current_version) => Err(ApiError::conflict(
            "admin.version_conflict",
            format!(
                "Optimistic locking failure: current version is {}, expected {}",
                current_version, req.version
            ),
        )),
        None => {
            if req.version == 0 {
                let inserted: (String, serde_json::Value, String, time::OffsetDateTime, i32) =
                    sqlx::query_as(
                        r#"
                    INSERT INTO app_config (key, value, updated_by, updated_at, version)
                    VALUES ($1, $2, $3, now(), 1)
                    RETURNING key, value, updated_by, updated_at, version
                    "#,
                    )
                    .bind(&key)
                    .bind(&req.value)
                    .bind(&actor)
                    .fetch_one(&state.pool)
                    .await
                    .map_err(|e| ApiError::internal(e.to_string()))?;

                let (key, value, updated_by, updated_at, version) = inserted;
                let _ = state
                    .moderation_service
                    .record_audit_log(
                        &actor,
                        "create_config",
                        Some("app_config"),
                        Some(&key),
                        serde_json::json!({
                            "key": key,
                            "version": version,
                            "value": value,
                        }),
                    )
                    .await;

                Ok(Json(AppConfigItem {
                    key,
                    value,
                    updated_by,
                    updated_at: updated_at
                        .format(&time::format_description::well_known::Rfc3339)
                        .unwrap_or_default(),
                    version,
                }))
            } else {
                Err(ApiError::not_found(
                    "admin.config_not_found",
                    format!("Configuration key '{key}' not found"),
                ))
            }
        }
    }
}
