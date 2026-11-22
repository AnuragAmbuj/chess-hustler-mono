//! Me & Profile routes (rest.md §2.2):
//! - `GET /v1/me`: user + profile + settings + ratings + gold
//! - `PATCH /v1/me`: update display_name, bio, user settings
//! - `DELETE /v1/me`: DPDP deletion request (30-day grace)
//! - `POST /v1/me/deletion-cancel`: cancel deletion within grace window
//! - `GET /v1/me/gold`: gold balance and paginated ledger history
//! - `GET /v1/players/{handle}`: public player profile

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::auth::Authed;
use crate::error::ApiError;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct PatchMeRequest {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub settings: Option<PatchSettingsRequest>,
}

#[derive(Debug, Deserialize)]
pub struct PatchSettingsRequest {
    pub chat_enabled: Option<bool>,
    pub sound_enabled: Option<bool>,
    pub premove_enabled: Option<bool>,
    pub quiet_hours_start: Option<i32>,
    pub quiet_hours_end: Option<i32>,
    pub marketing_opt_in: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct GoldPageQuery {
    pub cursor: Option<i64>,
    #[serde(default = "default_gold_limit")]
    pub limit: i32,
}

fn default_gold_limit() -> i32 {
    25
}

/// `GET /v1/me` — current authenticated user profile, ratings, gold, and settings.
pub async fn get_me(
    State(state): State<AppState>,
    authed: Authed,
) -> Result<Json<serde_json::Value>, ApiError> {
    let me = build_me_response(&state, authed.user_id).await?;
    Ok(Json(me))
}

/// `PATCH /v1/me` — update display_name, bio, or settings.
pub async fn patch_me(
    State(state): State<AppState>,
    authed: Authed,
    Json(req): Json<PatchMeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(ref name) = req.display_name {
        let trimmed = name.trim();
        if trimmed.len() < 2 || trimmed.len() > 30 {
            return Err(ApiError::validation(
                "display_name must be between 2 and 30 characters",
                json!(["display_name"]),
            ));
        }
    }
    if let Some(ref bio) = req.bio {
        if bio.len() > 200 {
            return Err(ApiError::validation(
                "bio must be at most 200 characters",
                json!(["bio"]),
            ));
        }
    }
    if let Some(ref s) = req.settings {
        if let Some(start) = s.quiet_hours_start {
            if !(0..=23).contains(&start) {
                return Err(ApiError::validation(
                    "quiet_hours_start must be between 0 and 23",
                    json!(["quiet_hours_start"]),
                ));
            }
        }
        if let Some(end) = s.quiet_hours_end {
            if !(0..=23).contains(&end) {
                return Err(ApiError::validation(
                    "quiet_hours_end must be between 0 and 23",
                    json!(["quiet_hours_end"]),
                ));
            }
        }
    }

    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!(error = %e, "tx begin failed");
        ApiError::internal("transaction failed")
    })?;

    if req.display_name.is_some() || req.bio.is_some() {
        sqlx::query(
            "UPDATE profiles
             SET display_name = COALESCE($1, display_name),
                 bio = COALESCE($2, bio)
             WHERE user_id = $3",
        )
        .bind(req.display_name.as_deref())
        .bind(req.bio.as_deref())
        .bind(authed.user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "profile update failed");
            ApiError::internal("profile update failed")
        })?;
    }

    if let Some(ref s) = req.settings {
        sqlx::query(
            "UPDATE user_settings
             SET chat_enabled = COALESCE($1, chat_enabled),
                 sound_enabled = COALESCE($2, sound_enabled),
                 premove_enabled = COALESCE($3, premove_enabled),
                 quiet_hours_start = COALESCE($4, quiet_hours_start),
                 quiet_hours_end = COALESCE($5, quiet_hours_end),
                 marketing_opt_in = COALESCE($6, marketing_opt_in),
                 updated_at = now()
             WHERE user_id = $7",
        )
        .bind(s.chat_enabled)
        .bind(s.sound_enabled)
        .bind(s.premove_enabled)
        .bind(s.quiet_hours_start)
        .bind(s.quiet_hours_end)
        .bind(s.marketing_opt_in)
        .bind(authed.user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "settings update failed");
            ApiError::internal("settings update failed")
        })?;

        // DPDP compliance: if marketing_opt_in changed, record new consent_record
        if let Some(marketing) = s.marketing_opt_in {
            sqlx::query(
                "INSERT INTO consent_records (user_id, policy_version, purpose, granted)
                 VALUES ($1, 'v1.0', 'marketing', $2)",
            )
            .bind(authed.user_id)
            .bind(marketing)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "consent_record insert failed");
                ApiError::internal("consent update failed")
            })?;
        }
    }

    tx.commit().await.map_err(|e| {
        tracing::error!(error = %e, "tx commit failed");
        ApiError::internal("transaction failed")
    })?;

    let me = build_me_response(&state, authed.user_id).await?;
    Ok(Json(me))
}

/// `DELETE /v1/me` — initiates DPDP right to erasure (30-day grace period).
pub async fn delete_me(
    State(state): State<AppState>,
    authed: Authed,
) -> Result<Json<serde_json::Value>, ApiError> {
    let now: time::OffsetDateTime = sqlx::query_scalar(
        "UPDATE users SET deletion_requested_at = now() WHERE id = $1 RETURNING deletion_requested_at",
    )
    .bind(authed.user_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "deletion request failed");
        ApiError::internal("deletion request failed")
    })?;

    Ok(Json(json!({
        "status": "deletion_scheduled",
        "deletion_requested_at": now.to_string(),
        "grace_period_days": 30,
    })))
}

/// `POST /v1/me/deletion-cancel` — cancels deletion request within grace window.
pub async fn cancel_deletion(
    State(state): State<AppState>,
    authed: Authed,
) -> Result<StatusCode, ApiError> {
    sqlx::query("UPDATE users SET deletion_requested_at = NULL WHERE id = $1")
        .bind(authed.user_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "deletion cancel failed");
            ApiError::internal("deletion cancel failed")
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/me/gold` — balance + paginated ledger history (newest first).
pub async fn get_me_gold(
    State(state): State<AppState>,
    authed: Authed,
    Query(query): Query<GoldPageQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let balance = state.ledger.balance_of(authed.user_id).await?;
    let entries = state
        .ledger
        .page_entries(authed.user_id, query.cursor, query.limit)
        .await?;

    let next_cursor = if entries.len() as i32 == query.limit {
        entries.last().map(|e| e.id)
    } else {
        None
    };

    let items: Vec<serde_json::Value> = entries
        .into_iter()
        .map(|e| {
            json!({
                "id": e.id,
                "delta": e.delta,
                "balance_after": e.balance_after,
                "reason": e.reason,
                "ref_type": e.ref_type,
                "ref_id": e.ref_id,
                "created_at": e.created_at.to_string(),
            })
        })
        .collect();

    Ok(Json(json!({
        "balance": balance,
        "items": items,
        "next_cursor": next_cursor,
    })))
}

#[derive(sqlx::FromRow)]
struct PlayerRow {
    id: i64,
    public_id: String,
    handle: String,
    display_name: String,
    bio: Option<String>,
    country: String,
    avatar_cosmetic_id: Option<String>,
    games_count: i32,
    wins: i32,
    losses: i32,
    draws: i32,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    public_id: String,
    phone_e164: Option<String>,
    email: Option<String>,
    deletion_requested_at: Option<time::OffsetDateTime>,
    created_at: time::OffsetDateTime,
}

#[derive(sqlx::FromRow)]
struct ProfileRow {
    handle: String,
    display_name: String,
    bio: Option<String>,
    country: String,
    avatar_cosmetic_id: Option<String>,
    games_count: i32,
    wins: i32,
    losses: i32,
    draws: i32,
}

#[derive(sqlx::FromRow)]
struct SettingsRow {
    chat_enabled: bool,
    sound_enabled: bool,
    premove_enabled: bool,
    quiet_hours_start: i32,
    quiet_hours_end: i32,
    marketing_opt_in: bool,
}

/// `GET /v1/players/{handle}` — public profile for any user.
pub async fn get_player_profile(
    State(state): State<AppState>,
    Path(handle): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let row: Option<PlayerRow> = sqlx::query_as(
        "SELECT u.id, u.public_id, p.handle, p.display_name, p.bio, p.country, p.avatar_cosmetic_id,
                p.games_count, p.wins, p.losses, p.draws
         FROM profiles p
         JOIN users u ON u.id = p.user_id
         WHERE lower(p.handle) = lower($1)",
    )
    .bind(&handle)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "player lookup failed");
        ApiError::internal("player lookup failed")
    })?;

    let p = row.ok_or_else(|| ApiError::not_found("player.not_found", "player not found"))?;

    let mut ratings = json!({
        "bullet": 1200, "blitz": 1200, "rapid": 1200, "classical": 1200,
    });
    let mut provisional = true;
    for r in hustle_ratings::get_all(&state.pool, p.id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "ratings lookup failed");
            ApiError::internal("ratings lookup failed")
        })?
    {
        ratings[r.pool.as_str()] = json!(r.rating as i64);
        provisional &= r.games_count == 0;
    }

    Ok(Json(json!({
        "id": p.public_id,
        "handle": p.handle,
        "display_name": p.display_name,
        "bio": p.bio,
        "country": p.country,
        "avatar_cosmetic_id": p.avatar_cosmetic_id,
        "games_count": p.games_count,
        "wins": p.wins,
        "losses": p.losses,
        "draws": p.draws,
        "ratings": ratings,
        "provisional": provisional,
    })))
}

#[derive(Debug, Deserialize)]
pub struct PlayerGamesQuery {
    pub cursor: Option<i64>,
    pub limit: Option<usize>,
}

#[derive(Debug, sqlx::FromRow)]
struct PlayerGameDbRow {
    pub id: i64,
    pub public_id: String,
    pub pool: String,
    pub rated: bool,
    pub status: String,
    pub result: Option<String>,
    pub termination: Option<String>,
    pub move_count: i32,
    pub fen_current: String,
    pub created_at: time::OffsetDateTime,
    pub white_handle: String,
    pub black_handle: String,
    pub white_rating: f32,
    pub black_rating: f32,
}

/// `GET /v1/players/{handle}/games` — paginated public games feed for a player.
pub async fn get_player_games(
    State(state): State<AppState>,
    Path(handle): Path<String>,
    Query(query): Query<PlayerGamesQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id: Option<i64> =
        sqlx::query_scalar("SELECT user_id FROM profiles WHERE lower(handle) = lower($1)")
            .bind(&handle)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "handle lookup failed");
                ApiError::internal("handle lookup failed")
            })?;

    let user_id =
        user_id.ok_or_else(|| ApiError::not_found("player.not_found", "player not found"))?;

    let limit = query.limit.unwrap_or(25).min(100);
    let fetch_limit = (limit + 1) as i64;
    let cursor = query.cursor.unwrap_or(i64::MAX);

    let rows: Vec<PlayerGameDbRow> = sqlx::query_as(
        "SELECT g.id, g.public_id, g.pool, g.rated, g.status, g.result, g.termination,
                g.move_count, g.fen_current, g.created_at,
                pw.handle AS white_handle, pb.handle AS black_handle,
                COALESCE(rw.rating, 1200.0) AS white_rating,
                COALESCE(rb.rating, 1200.0) AS black_rating
         FROM games g
         JOIN profiles pw ON pw.user_id = g.white_user_id
         JOIN profiles pb ON pb.user_id = g.black_user_id
         LEFT JOIN ratings rw ON rw.user_id = g.white_user_id AND rw.pool = g.pool
         LEFT JOIN ratings rb ON rb.user_id = g.black_user_id AND rb.pool = g.pool
         WHERE (g.white_user_id = $1 OR g.black_user_id = $1)
           AND g.id < $2
         ORDER BY g.id DESC
         LIMIT $3",
    )
    .bind(user_id)
    .bind(cursor)
    .bind(fetch_limit)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "player games query failed");
        ApiError::internal("player games query failed")
    })?;

    let has_more = rows.len() > limit;
    let items_rows = if has_more { &rows[..limit] } else { &rows[..] };
    let next_cursor = if has_more {
        items_rows.last().map(|r| r.id)
    } else {
        None
    };

    let items = items_rows
        .iter()
        .map(|r| {
            json!({
                "id": r.public_id,
                "pool": r.pool,
                "rated": r.rated,
                "status": r.status,
                "result": r.result,
                "termination": r.termination,
                "move_count": r.move_count,
                "white": {
                    "handle": r.white_handle,
                    "rating": r.white_rating as i64,
                },
                "black": {
                    "handle": r.black_handle,
                    "rating": r.black_rating as i64,
                },
                "fen": r.fen_current,
                "played_at": r.created_at.format(&time::format_description::well_known::Rfc3339).unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();

    Ok(Json(json!({
        "items": items,
        "next_cursor": next_cursor,
    })))
}

#[derive(Debug, sqlx::FromRow)]
struct ExportGameRow {
    public_id: String,
    pool: String,
    rated: bool,
    status: String,
    result: Option<String>,
    termination: Option<String>,
    move_count: i32,
    created_at: time::OffsetDateTime,
}

/// `GET /v1/me/export` — DPDP Act right to data portability export.
pub async fn export_me(
    State(state): State<AppState>,
    authed: Authed,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = authed.user_id;

    let me_data = build_me_response(&state, user_id).await?;

    let consents: Vec<(String, bool, time::OffsetDateTime)> = sqlx::query_as(
        "SELECT purpose, granted, created_at FROM consent_records WHERE user_id = $1 ORDER BY id ASC",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let balance = state.ledger.balance_of(user_id).await.unwrap_or(0);
    let ledger_entries = state
        .ledger
        .page_entries(user_id, None, 100)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|e| {
            json!({
                "id": e.id,
                "delta": e.delta,
                "balance_after": e.balance_after,
                "reason": e.reason,
                "created_at": e.created_at.to_string(),
            })
        })
        .collect::<Vec<_>>();

    let mut ratings = json!({
        "bullet": 1200, "blitz": 1200, "rapid": 1200, "classical": 1200,
    });
    for r in hustle_ratings::get_all(&state.pool, user_id)
        .await
        .unwrap_or_default()
    {
        ratings[r.pool.as_str()] = json!(r.rating as i64);
    }

    let games: Vec<ExportGameRow> = sqlx::query_as(
        "SELECT public_id, pool, rated, status, result, termination, move_count, created_at
         FROM games
         WHERE white_user_id = $1 OR black_user_id = $1
         ORDER BY id DESC LIMIT 100",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let job_id = format!("export_{}_{}", ulid::Ulid::new(), user_id);

    Ok(Json(json!({
        "job_id": job_id,
        "status": "ready",
        "exported_at": time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_default(),
        "bundle": {
            "profile": me_data,
            "consents": consents.into_iter().map(|(t, g, ts)| json!({
                "type": t,
                "granted": g,
                "timestamp": ts.format(&time::format_description::well_known::Rfc3339).unwrap_or_default(),
            })).collect::<Vec<_>>(),
            "gold": {
                "balance": balance,
                "ledger_sample": ledger_entries,
            },
            "ratings": ratings,
            "games_sample": games.into_iter().map(|g| json!({
                "id": g.public_id,
                "pool": g.pool,
                "rated": g.rated,
                "status": g.status,
                "result": g.result,
                "termination": g.termination,
                "move_count": g.move_count,
                "played_at": g.created_at.format(&time::format_description::well_known::Rfc3339).unwrap_or_default(),
            })).collect::<Vec<_>>(),
        }
    })))
}

// ---- helpers ---------------------------------------------------------------

async fn build_me_response(state: &AppState, user_id: i64) -> Result<serde_json::Value, ApiError> {
    let user_row: Option<UserRow> = sqlx::query_as(
        "SELECT public_id, phone_e164, email, deletion_requested_at, created_at FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "user lookup failed");
        ApiError::internal("user lookup failed")
    })?;

    let u = user_row.ok_or_else(|| ApiError::not_found("user.not_found", "user not found"))?;

    let profile_row: Option<ProfileRow> = sqlx::query_as(
        "SELECT handle, display_name, bio, country, avatar_cosmetic_id, games_count, wins, losses, draws FROM profiles WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "profile lookup failed");
        ApiError::internal("profile lookup failed")
    })?;

    let p =
        profile_row.ok_or_else(|| ApiError::not_found("profile.not_found", "profile not found"))?;

    let settings_row: Option<SettingsRow> = sqlx::query_as(
        "SELECT chat_enabled, sound_enabled, premove_enabled, quiet_hours_start, quiet_hours_end, marketing_opt_in FROM user_settings WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "settings lookup failed");
        ApiError::internal("settings lookup failed")
    })?;

    let (
        chat_enabled,
        sound_enabled,
        premove_enabled,
        quiet_hours_start,
        quiet_hours_end,
        marketing_opt_in,
    ) = match settings_row {
        Some(s) => (
            s.chat_enabled,
            s.sound_enabled,
            s.premove_enabled,
            s.quiet_hours_start,
            s.quiet_hours_end,
            s.marketing_opt_in,
        ),
        None => (true, true, true, 22, 7, false),
    };

    let gold = state.ledger.balance_of(user_id).await?;

    let mut ratings = json!({
        "bullet": 1200, "blitz": 1200, "rapid": 1200, "classical": 1200,
    });
    let mut provisional = true;
    for r in hustle_ratings::get_all(&state.pool, user_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "ratings lookup failed");
            ApiError::internal("ratings lookup failed")
        })?
    {
        ratings[r.pool.as_str()] = json!(r.rating as i64);
        provisional &= r.games_count == 0;
    }

    Ok(json!({
        "id": u.public_id,
        "phone": u.phone_e164,
        "email": u.email,
        "handle": p.handle,
        "display_name": p.display_name,
        "bio": p.bio,
        "country": p.country,
        "avatar_cosmetic_id": p.avatar_cosmetic_id,
        "games_count": p.games_count,
        "wins": p.wins,
        "losses": p.losses,
        "draws": p.draws,
        "ratings": ratings,
        "provisional": provisional,
        "gold": gold,
        "settings": {
            "chat_enabled": chat_enabled,
            "sound_enabled": sound_enabled,
            "premove_enabled": premove_enabled,
            "quiet_hours_start": quiet_hours_start,
            "quiet_hours_end": quiet_hours_end,
            "marketing_opt_in": marketing_opt_in,
        },
        "deletion_requested_at": u.deletion_requested_at.map(|t| t.to_string()),
        "created_at": u.created_at.to_string(),
    }))
}
