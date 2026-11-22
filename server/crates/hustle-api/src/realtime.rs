//! Realtime Gateway: ticket issuance and WebSocket upgrade endpoint (ADR-004, ADR-008).

use crate::auth::Authed;
use crate::error::ApiError;
use crate::AppState;
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use hustle_realtime::{AllowUserAndCallback, WsSession};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
pub struct RealtimeTicketResponse {
    pub ticket: String,
    pub ws_url: String,
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub ticket: String,
}

/// GET /v1/realtime/ticket — Issue a 30s single-use WebSocket ticket (ADR-008).
pub async fn get_ticket(
    authed: Authed,
    State(state): State<AppState>,
) -> Result<Json<RealtimeTicketResponse>, ApiError> {
    let ticket = state
        .ticket_store
        .issue_ticket(authed.user_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(RealtimeTicketResponse {
        ticket,
        ws_url: "/ws".to_string(),
        expires_in: 30,
    }))
}

/// GET /ws?ticket=<ticket> — WebSocket Upgrade handler with atomic ticket consumption.
pub async fn ws_upgrade_handler(
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
    mut req: axum::extract::Request,
) -> Response {
    // 1. Consume ticket atomically from Redis
    let user_id = match state.ticket_store.consume_ticket(&query.ticket).await {
        Ok(Some(uid)) => uid,
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": {
                        "code": "auth.ticket_invalid",
                        "message": "WebSocket ticket is invalid, expired, or already used",
                    }
                })),
            )
                .into_response();
        }
    };

    // 2. Validate WebSocket upgrade headers
    let has_upgrade = req
        .headers()
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    let key = match req
        .headers()
        .get("sec-websocket-key")
        .and_then(|v| v.to_str().ok())
    {
        Some(k) => k.to_string(),
        None => {
            return (StatusCode::BAD_REQUEST, "Missing Sec-WebSocket-Key").into_response();
        }
    };

    if !has_upgrade {
        return (StatusCode::BAD_REQUEST, "Expected websocket upgrade").into_response();
    }

    // 3. Compute Sec-WebSocket-Accept
    let accept_key = compute_ws_accept(&key);

    // 4. Upgrade the connection via hyper
    let on_upgrade = hyper::upgrade::on(&mut req);

    let pool = state.pool.clone();
    let authorizer = Arc::new(AllowUserAndCallback::new(move |uid: i64, ch: String| {
        let pool = pool.clone();
        async move {
            if let Some(game_public_id) = ch.strip_prefix("g:") {
                let ok: Option<i64> = sqlx::query_scalar(
                    "SELECT id FROM games WHERE public_id = $1 AND (white_user_id = $2 OR black_user_id = $2)",
                )
                .bind(game_public_id)
                .bind(uid)
                .fetch_optional(&pool)
                .await
                .unwrap_or(None);
                ok.is_some()
            } else {
                false
            }
        }
    }));

    let move_handler = Arc::new(RealtimeMoveHandler {
        game_service: state.game_service.clone(),
        publisher: state.publisher.clone(),
        moderation_service: state.moderation_service.clone(),
    });

    let session = Arc::new(
        WsSession::new(
            user_id,
            (*state.publisher).clone(),
            authorizer,
            state.matchmaking.redis_client().clone(),
        )
        .with_move_handler(move_handler),
    );

    tokio::spawn(async move {
        match on_upgrade.await {
            Ok(upgraded) => {
                let io = hyper_util::rt::TokioIo::new(upgraded);
                session.run(io).await;
            }
            Err(e) => {
                tracing::error!("WebSocket upgrade failed: {e}");
            }
        }
    });

    Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(header::UPGRADE, "websocket")
        .header(header::CONNECTION, "Upgrade")
        .header("Sec-WebSocket-Accept", accept_key)
        .body(axum::body::Body::empty())
        .unwrap()
}

fn compute_ws_accept(key: &str) -> String {
    use sha1::Digest;
    let mut hasher = sha1::Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let hash = hasher.finalize();
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(hash)
}

struct RealtimeMoveHandler {
    game_service: Arc<hustle_chess::GameService>,
    publisher: Arc<hustle_realtime::EventPublisher>,
    moderation_service: Arc<hustle_moderation::ModerationService>,
}

#[async_trait::async_trait]
impl hustle_realtime::MoveHandler for RealtimeMoveHandler {
    async fn handle_move(&self, user_id: i64, game_id: &str, uci: &str) {
        let ch = format!("g:{game_id}");
        match self.game_service.apply_move(game_id, user_id, uci).await {
            Ok(res) => {
                let move_val = serde_json::to_value(&res.move_data).unwrap_or_default();
                if let Err(e) = self.publisher.publish(&ch, "game.move", move_val).await {
                    tracing::error!("failed to publish game.move: {e}");
                }

                if let Some(completion) = res.completion {
                    let comp_val = serde_json::to_value(&completion).unwrap_or_default();
                    if let Err(e) = self.publisher.publish(&ch, "game.result", comp_val).await {
                        tracing::error!("failed to publish game.result: {e}");
                    }

                    let mod_svc = self.moderation_service.clone();
                    let gid = res.game.id;
                    tokio::spawn(async move {
                        let _ = mod_svc.run_anti_cheat_analysis(gid).await;
                    });
                }
            }
            Err(e) => {
                tracing::warn!("game move error for user {user_id} in {game_id}: {e}");
                let code = match e {
                    hustle_chess::GameServiceError::WrongTurn => "game.wrong_turn",
                    hustle_chess::GameServiceError::IllegalMove(_) => "game.illegal_move",
                    hustle_chess::GameServiceError::RateLimited => "rate_limited",
                    hustle_chess::GameServiceError::NotParticipant => "game.not_participant",
                    hustle_chess::GameServiceError::GameFinished(_) => "game.finished",
                    _ => "game.error",
                };
                let err_val = serde_json::json!({
                    "code": code,
                    "message": e.to_string(),
                });
                let _ = self.publisher.publish(&ch, "ws.error", err_val).await;
            }
        }
    }
}
