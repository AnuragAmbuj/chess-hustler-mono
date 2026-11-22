//! Game REST endpoints (docs/03-api/rest.md §2.3).
//!
//! - `GET /v1/games/{id}`: full game snapshot
//! - `GET /v1/games/{id}/moves`: list of plies
//! - `GET /v1/games/{id}/pgn`: export PGN string
//! - `POST /v1/games/{id}/resign`: resign active game
//! - `POST /v1/games/{id}/abort`: abort game if move_count <= 4

use crate::auth::Authed;
use crate::error::ApiError;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use time::format_description::well_known::Rfc3339;

#[derive(Debug, Deserialize)]
pub struct MovesQuery {
    pub cursor: Option<i32>,
    pub limit: Option<usize>,
}

/// Helper to fetch player handle and pool rating.
async fn get_player_info(
    pool: &sqlx::PgPool,
    user_id: i64,
    game_pool: &str,
) -> Result<(String, f64), ApiError> {
    let handle: Option<String> =
        sqlx::query_scalar("SELECT handle FROM profiles WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;

    let rating: Option<f32> =
        sqlx::query_scalar("SELECT rating FROM ratings WHERE user_id = $1 AND pool = $2")
            .bind(user_id)
            .bind(game_pool)
            .fetch_optional(pool)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok((
        handle.unwrap_or_else(|| format!("player_{user_id}")),
        rating.unwrap_or(1200.0) as f64,
    ))
}

/// GET /v1/games/{id} — Full game snapshot (fen, clocks, players, status, moves).
pub async fn get_game(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let game = state
        .game_service
        .get_game_by_public_id(&id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("game.not_found", "game not found"))?;

    let moves = state
        .game_service
        .get_moves_by_game_id(game.id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let (white_handle, white_rating) =
        get_player_info(&state.pool, game.white_user_id, &game.pool).await?;
    let (black_handle, black_rating) =
        get_player_info(&state.pool, game.black_user_id, &game.pool).await?;

    let initial_ms = game
        .time_control
        .get("initial_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(180_000);
    let increment_ms = game
        .time_control
        .get("increment_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let initial_s = (initial_ms / 1000) as u32;
    let increment_s = (increment_ms / 1000) as u32;

    let white_ms = game.w_remaining_ms.unwrap_or(initial_ms as i64);
    let black_ms = game.b_remaining_ms.unwrap_or(initial_ms as i64);
    let server_ts = game
        .last_tick_ts
        .map(|t| t.format(&Rfc3339).unwrap_or_default());

    let moves_list: Vec<serde_json::Value> = moves
        .iter()
        .map(|m| {
            json!({
                "ply": m.ply,
                "uci": m.uci,
                "san": m.san,
            })
        })
        .collect();

    Ok(Json(json!({
        "id": game.public_id,
        "rated": game.rated,
        "pool": game.pool,
        "tc": {
            "initial_s": initial_s,
            "increment_s": increment_s,
        },
        "white": {
            "handle": white_handle,
            "rating": white_rating as i64,
        },
        "black": {
            "handle": black_handle,
            "rating": black_rating as i64,
        },
        "status": game.status,
        "result": game.result,
        "termination": game.termination,
        "fen": game.fen_current,
        "ply": game.move_count,
        "version": game.version,
        "clocks": {
            "white_ms": white_ms,
            "black_ms": black_ms,
            "server_ts": server_ts,
        },
        "moves": moves_list,
        "stream_id": serde_json::Value::Null,
        "draw_offer": serde_json::Value::Null,
    })))
}

/// GET /v1/games/{id}/moves — Ply list (cursor by ply).
pub async fn get_game_moves(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<MovesQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let game = state
        .game_service
        .get_game_by_public_id(&id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("game.not_found", "game not found"))?;

    let all_moves = state
        .game_service
        .get_moves_by_game_id(game.id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let cursor = query.cursor.unwrap_or(0);
    let limit = query.limit.unwrap_or(25).min(100);

    let filtered: Vec<_> = all_moves.into_iter().filter(|m| m.ply > cursor).collect();
    let has_more = filtered.len() > limit;
    let items: Vec<_> = filtered.into_iter().take(limit).collect();
    let next_cursor = if has_more {
        items.last().map(|m| m.ply)
    } else {
        None
    };

    let items_json: Vec<_> = items
        .into_iter()
        .map(|m| {
            json!({
                "ply": m.ply,
                "uci": m.uci,
                "san": m.san,
                "fen_after": m.fen_after,
                "w_remaining_ms": m.w_remaining_ms,
                "b_remaining_ms": m.b_remaining_ms,
                "ts": m.ts.format(&Rfc3339).unwrap_or_default(),
            })
        })
        .collect();

    Ok(Json(json!({
        "items": items_json,
        "next_cursor": next_cursor,
    })))
}

/// GET /v1/games/{id}/pgn — PGN export with full headers.
pub async fn get_game_pgn(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let game = state
        .game_service
        .get_game_by_public_id(&id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("game.not_found", "game not found"))?;

    let moves = state
        .game_service
        .get_moves_by_game_id(game.id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let (white_handle, white_rating) =
        get_player_info(&state.pool, game.white_user_id, &game.pool).await?;
    let (black_handle, black_rating) =
        get_player_info(&state.pool, game.black_user_id, &game.pool).await?;

    let initial_s = game
        .time_control
        .get("initial_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(180_000)
        / 1000;
    let increment_s = game
        .time_control
        .get("increment_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        / 1000;

    let result_str = match game.result.as_deref() {
        Some("white") => "1-0",
        Some("black") => "0-1",
        Some("draw") => "1/2-1/2",
        _ => "*",
    };

    let date_str = format!(
        "{:04}.{:02}.{:02}",
        game.created_at.year(),
        game.created_at.month() as u8,
        game.created_at.day()
    );

    let mut pgn = String::with_capacity(512);
    pgn.push_str(&format!("[Event \"Chess Hustle {}\"]\n", game.pool));
    pgn.push_str("[Site \"Chess Hustle\"]\n");
    pgn.push_str(&format!("[Date \"{}\"]\n", date_str));
    pgn.push_str("[Round \"-\"]\n");
    pgn.push_str(&format!("[White \"{}\"]\n", white_handle));
    pgn.push_str(&format!("[Black \"{}\"]\n", black_handle));
    pgn.push_str(&format!("[Result \"{}\"]\n", result_str));
    pgn.push_str(&format!("[WhiteElo \"{}\"]\n", white_rating as i64));
    pgn.push_str(&format!("[BlackElo \"{}\"]\n", black_rating as i64));
    pgn.push_str(&format!("[TimeControl \"{}+{increment_s}\"]\n", initial_s));
    if let Some(term) = &game.termination {
        pgn.push_str(&format!("[Termination \"{}\"]\n", term));
    }
    pgn.push('\n');

    let mut move_text = String::new();
    for m in &moves {
        if m.ply % 2 == 1 {
            let move_num = (m.ply + 1) / 2;
            if !move_text.is_empty() {
                move_text.push(' ');
            }
            move_text.push_str(&format!("{}. {}", move_num, m.san));
        } else {
            move_text.push_str(&format!(" {}", m.san));
        }
    }

    if !move_text.is_empty() {
        pgn.push_str(&move_text);
        pgn.push(' ');
    }
    pgn.push_str(result_str);
    pgn.push('\n');

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/x-chess-pgn; charset=utf-8",
        )
        .body(axum::body::Body::from(pgn))
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(response)
}

/// POST /v1/games/{id}/resign — Resign active game.
pub async fn post_game_resign(
    authed: Authed,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    match state.game_service.resign_game(&id, authed.user_id).await {
        Ok(res) => {
            let ch = format!("g:{id}");
            let ratings_json = res.ratings.as_ref().map(|r| {
                json!({
                    "white": { "before": r.white.before, "after": r.white.after, "delta": r.white.delta },
                    "black": { "before": r.black.before, "after": r.black.after, "delta": r.black.delta },
                })
            });

            // Publish game.result to Redis stream g:{id} and ws:g:{id}
            let _ = state
                .publisher
                .publish(
                    &ch,
                    "game.result",
                    json!({
                        "result": res.result,
                        "termination": res.termination,
                        "ratings": ratings_json,
                        "gold": {
                            "white_earned": if authed.user_id == res.game.white_user_id { res.user_gold_earned } else { 25 },
                            "black_earned": if authed.user_id == res.game.black_user_id { res.user_gold_earned } else { 25 },
                        }
                    }),
                )
                .await;

            let winner_id = if authed.user_id == res.game.white_user_id {
                Some(res.game.black_user_id)
            } else {
                Some(res.game.white_user_id)
            };
            let _ = state
                .reward_service
                .record_game_finished(
                    &hustle_rewards::GameFinishedEvent {
                        white_user_id: res.game.white_user_id,
                        black_user_id: res.game.black_user_id,
                        winner_user_id: winner_id,
                        checkmate: false,
                        draw: false,
                    },
                    time::OffsetDateTime::now_utc(),
                )
                .await;

            let mod_svc = state.moderation_service.clone();
            let g_id = res.game.id;
            tokio::spawn(async move {
                let _ = mod_svc.run_anti_cheat_analysis(g_id).await;
            });

            Ok(Json(json!({
                "status": "finished",
                "result": res.result,
                "termination": res.termination,
                "ratings": ratings_json,
                "gold": {
                    "earned": res.user_gold_earned,
                    "reason": "gameplay",
                    "balance": res.user_gold_balance,
                }
            })))
        }
        Err(hustle_chess::GameServiceError::NotFound(id)) => Err(ApiError::not_found(
            "game.not_found",
            format!("game {id} not found"),
        )),
        Err(hustle_chess::GameServiceError::NotParticipant) => Err(ApiError::forbidden(
            "game.not_participant",
            "user is not a participant in this game",
        )),
        Err(hustle_chess::GameServiceError::GameFinished(status)) => Err(ApiError::conflict(
            "game.finished",
            format!("game already finished: {status}"),
        )),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

/// POST /v1/games/{id}/draw-offer — Offer a draw to opponent.
pub async fn post_draw_offer(
    authed: Authed,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    match state.game_service.offer_draw(&id, authed.user_id).await {
        Ok(res) => {
            let ch = format!("g:{id}");
            let side = if authed.user_id == res.white_user_id {
                "white"
            } else {
                "black"
            };
            let _ = state
                .publisher
                .publish(
                    &ch,
                    "game.draw_offer",
                    json!({
                        "by": side,
                        "user_id": authed.user_id,
                    }),
                )
                .await;

            Ok(Json(json!({
                "id": res.public_id,
                "draw_offer": side,
            })))
        }
        Err(hustle_chess::GameServiceError::NotFound(id)) => Err(ApiError::not_found(
            "game.not_found",
            format!("game {id} not found"),
        )),
        Err(hustle_chess::GameServiceError::NotParticipant) => Err(ApiError::forbidden(
            "game.not_participant",
            "user is not a participant in this game",
        )),
        Err(hustle_chess::GameServiceError::GameFinished(status)) => Err(ApiError::conflict(
            "game.finished",
            format!("game already finished: {status}"),
        )),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

/// POST /v1/games/{id}/draw-accept — Accept opponent's draw offer.
pub async fn post_draw_accept(
    authed: Authed,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    match state.game_service.accept_draw(&id, authed.user_id).await {
        Ok(res) => {
            let ch = format!("g:{id}");
            let ratings_json = res.ratings.as_ref().map(|r| {
                json!({
                    "white": { "before": r.white.before, "after": r.white.after, "delta": r.white.delta },
                    "black": { "before": r.black.before, "after": r.black.after, "delta": r.black.delta },
                })
            });

            let comp_event = json!({
                "result": res.result,
                "termination": res.termination,
                "ratings": ratings_json,
                "final_fen": res.game.fen_current,
            });
            let _ = state
                .publisher
                .publish(&ch, "game.result", comp_event)
                .await;

            let _ = state
                .reward_service
                .record_game_finished(
                    &hustle_rewards::GameFinishedEvent {
                        white_user_id: res.game.white_user_id,
                        black_user_id: res.game.black_user_id,
                        winner_user_id: None,
                        checkmate: false,
                        draw: true,
                    },
                    time::OffsetDateTime::now_utc(),
                )
                .await;

            let mod_svc = state.moderation_service.clone();
            let g_id = res.game.id;
            tokio::spawn(async move {
                let _ = mod_svc.run_anti_cheat_analysis(g_id).await;
            });

            Ok(Json(json!({
                "status": "finished",
                "result": res.result,
                "termination": res.termination,
                "ratings": ratings_json,
                "gold": {
                    "earned": res.user_gold_earned,
                    "reason": "gameplay",
                    "balance": res.user_gold_balance,
                }
            })))
        }
        Err(hustle_chess::GameServiceError::NotFound(id)) => Err(ApiError::not_found(
            "game.not_found",
            format!("game {id} not found"),
        )),
        Err(hustle_chess::GameServiceError::NotParticipant) => Err(ApiError::forbidden(
            "game.not_participant",
            "user is not a participant in this game",
        )),
        Err(hustle_chess::GameServiceError::NoDrawOffer) => Err(ApiError::conflict(
            "draw.no_offer",
            "no active draw offer to accept",
        )),
        Err(hustle_chess::GameServiceError::OwnDrawOffer) => Err(ApiError::forbidden(
            "draw.own_offer",
            "cannot accept your own draw offer",
        )),
        Err(hustle_chess::GameServiceError::GameFinished(status)) => Err(ApiError::conflict(
            "game.finished",
            format!("game already finished: {status}"),
        )),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

/// POST /v1/games/{id}/draw-decline — Decline opponent's draw offer.
pub async fn post_draw_decline(
    authed: Authed,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    match state.game_service.decline_draw(&id, authed.user_id).await {
        Ok(res) => {
            let ch = format!("g:{id}");
            let _ = state
                .publisher
                .publish(
                    &ch,
                    "game.draw_declined",
                    json!({
                        "declined_by": authed.user_id,
                    }),
                )
                .await;

            Ok(Json(json!({
                "id": res.public_id,
                "draw_offer": null,
            })))
        }
        Err(hustle_chess::GameServiceError::NotFound(id)) => Err(ApiError::not_found(
            "game.not_found",
            format!("game {id} not found"),
        )),
        Err(hustle_chess::GameServiceError::NotParticipant) => Err(ApiError::forbidden(
            "game.not_participant",
            "user is not a participant in this game",
        )),
        Err(hustle_chess::GameServiceError::NoDrawOffer) => Err(ApiError::conflict(
            "draw.no_offer",
            "no active draw offer to decline",
        )),
        Err(hustle_chess::GameServiceError::OwnDrawOffer) => Err(ApiError::forbidden(
            "draw.own_offer",
            "cannot decline your own draw offer",
        )),
        Err(hustle_chess::GameServiceError::GameFinished(status)) => Err(ApiError::conflict(
            "game.finished",
            format!("game already finished: {status}"),
        )),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

/// POST /v1/games/{id}/claim-timeout — Claim win/draw on opponent flag fall.
pub async fn post_claim_timeout(
    authed: Authed,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    match state.game_service.claim_timeout(&id, authed.user_id).await {
        Ok(res) => {
            let ch = format!("g:{id}");
            let ratings_json = res.ratings.as_ref().map(|r| {
                json!({
                    "white": { "before": r.white.before, "after": r.white.after, "delta": r.white.delta },
                    "black": { "before": r.black.before, "after": r.black.after, "delta": r.black.delta },
                })
            });

            let comp_event = json!({
                "result": res.result,
                "termination": res.termination,
                "ratings": ratings_json,
                "final_fen": res.game.fen_current,
            });
            let _ = state
                .publisher
                .publish(&ch, "game.result", comp_event)
                .await;

            let _ = state
                .reward_service
                .record_game_finished(
                    &hustle_rewards::GameFinishedEvent {
                        white_user_id: res.game.white_user_id,
                        black_user_id: res.game.black_user_id,
                        winner_user_id: Some(authed.user_id),
                        checkmate: false,
                        draw: false,
                    },
                    time::OffsetDateTime::now_utc(),
                )
                .await;

            let mod_svc = state.moderation_service.clone();
            let g_id = res.game.id;
            tokio::spawn(async move {
                let _ = mod_svc.run_anti_cheat_analysis(g_id).await;
            });

            Ok(Json(json!({
                "status": "finished",
                "result": res.result,
                "termination": res.termination,
                "ratings": ratings_json,
                "gold": {
                    "earned": res.user_gold_earned,
                    "reason": "gameplay",
                    "balance": res.user_gold_balance,
                }
            })))
        }
        Err(hustle_chess::GameServiceError::NotFound(id)) => Err(ApiError::not_found(
            "game.not_found",
            format!("game {id} not found"),
        )),
        Err(hustle_chess::GameServiceError::NotParticipant) => Err(ApiError::forbidden(
            "game.not_participant",
            "user is not a participant in this game",
        )),
        Err(hustle_chess::GameServiceError::NotOpponentTurn) => Err(ApiError::conflict(
            "clock.not_opponent_turn",
            "cannot claim timeout when it is not opponent's turn",
        )),
        Err(hustle_chess::GameServiceError::ClockNotExpired) => Err(ApiError::conflict(
            "clock.not_expired",
            "opponent clock has not expired yet",
        )),
        Err(hustle_chess::GameServiceError::GameFinished(status)) => Err(ApiError::conflict(
            "game.finished",
            format!("game already finished: {status}"),
        )),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

/// POST /v1/games/{id}/abort — Abort game (valid <= 4 plies).
pub async fn post_game_abort(
    authed: Authed,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    match state.game_service.abort_game(&id, authed.user_id).await {
        Ok(res) => {
            let ch = format!("g:{id}");
            let _ = state
                .publisher
                .publish(
                    &ch,
                    "game.abort",
                    json!({
                        "by": authed.user_id,
                        "reason": "aborted",
                    }),
                )
                .await;

            Ok(Json(json!({
                "id": res.public_id,
                "status": "aborted",
                "termination": "aborted",
            })))
        }
        Err(hustle_chess::GameServiceError::NotFound(id)) => Err(ApiError::not_found(
            "game.not_found",
            format!("game {id} not found"),
        )),
        Err(hustle_chess::GameServiceError::NotParticipant) => Err(ApiError::forbidden(
            "game.not_participant",
            "user is not a participant in this game",
        )),
        Err(hustle_chess::GameServiceError::CannotAbort(msg)) => {
            Err(ApiError::validation(msg, json!(["move_count"])))
        }
        Err(hustle_chess::GameServiceError::GameFinished(status)) => Err(ApiError::conflict(
            "game.finished",
            format!("game already finished: {status}"),
        )),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}
