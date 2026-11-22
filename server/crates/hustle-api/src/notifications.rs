//! Notifications subsystem (docs/03-api/rest.md §2.9).
//!
//! Provides user notifications feed with cursor/limit pagination
//! and batch/bulk mark-as-read.

use crate::auth::Authed;
use crate::error::ApiError;
use crate::AppState;
use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct NotificationsQuery {
    pub cursor: Option<i64>,
    pub limit: Option<i64>,
    pub unread_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationItem {
    pub id: i64,
    pub kind: String,
    pub payload: serde_json::Value,
    pub read_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationsFeedResponse {
    pub items: Vec<NotificationItem>,
    pub next_cursor: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct MarkNotificationsReadRequest {
    pub ids: Option<Vec<i64>>,
    #[serde(default)]
    pub all: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationsReadResponse {
    pub status: String,
    pub updated: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct NotificationDbRow {
    id: i64,
    kind: String,
    payload: serde_json::Value,
    read_at: Option<time::OffsetDateTime>,
    created_at: time::OffsetDateTime,
}

/// `GET /v1/notifications` — Notification feed with cursor/limit.
pub async fn get_notifications(
    State(state): State<AppState>,
    authed: Authed,
    Query(query): Query<NotificationsQuery>,
) -> Result<Json<NotificationsFeedResponse>, ApiError> {
    let limit = query.limit.unwrap_or(25).clamp(1, 100);
    let fetch_limit = limit + 1;
    let cursor = query.cursor.unwrap_or(i64::MAX);
    let unread_only = query.unread_only.unwrap_or(false);

    let rows: Vec<NotificationDbRow> = if unread_only {
        sqlx::query_as(
            r#"
            SELECT id, kind, payload, read_at, created_at
            FROM notifications
            WHERE user_id = $1 AND read_at IS NULL AND id < $2
            ORDER BY id DESC
            LIMIT $3
            "#,
        )
        .bind(authed.user_id)
        .bind(cursor)
        .bind(fetch_limit)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
    } else {
        sqlx::query_as(
            r#"
            SELECT id, kind, payload, read_at, created_at
            FROM notifications
            WHERE user_id = $1 AND id < $2
            ORDER BY id DESC
            LIMIT $3
            "#,
        )
        .bind(authed.user_id)
        .bind(cursor)
        .bind(fetch_limit)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
    };

    let has_more = rows.len() as i64 > limit;
    let item_rows = if has_more {
        &rows[..limit as usize]
    } else {
        &rows[..]
    };
    let next_cursor = if has_more {
        item_rows.last().map(|r| r.id)
    } else {
        None
    };

    let items = item_rows
        .iter()
        .map(|r| NotificationItem {
            id: r.id,
            kind: r.kind.clone(),
            payload: r.payload.clone(),
            read_at: r.read_at.map(|t| {
                t.format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default()
            }),
            created_at: r
                .created_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        })
        .collect();

    Ok(Json(NotificationsFeedResponse { items, next_cursor }))
}

/// `POST /v1/notifications/read` — Mark notifications read by IDs or all.
pub async fn post_notifications_read(
    State(state): State<AppState>,
    authed: Authed,
    Json(req): Json<MarkNotificationsReadRequest>,
) -> Result<Json<NotificationsReadResponse>, ApiError> {
    let updated = if req.all.unwrap_or(false) {
        let result = sqlx::query(
            r#"
            UPDATE notifications
            SET read_at = now()
            WHERE user_id = $1 AND read_at IS NULL
            "#,
        )
        .bind(authed.user_id)
        .execute(&state.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

        result.rows_affected() as i64
    } else if let Some(ids) = req.ids {
        if ids.is_empty() {
            0
        } else {
            let result = sqlx::query(
                r#"
                UPDATE notifications
                SET read_at = now()
                WHERE user_id = $1 AND id = ANY($2) AND read_at IS NULL
                "#,
            )
            .bind(authed.user_id)
            .bind(&ids)
            .execute(&state.pool)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;

            result.rows_affected() as i64
        }
    } else {
        return Err(ApiError::validation(
            "Either 'ids' or 'all: true' must be specified",
            serde_json::json!({
                "ids": "array of notification ids",
                "all": "boolean flag to mark all read"
            }),
        ));
    };

    Ok(Json(NotificationsReadResponse {
        status: "ok".to_string(),
        updated,
    }))
}
