//! WsSession: Manages an active WebSocket client connection (ADR-004, websocket.md).

use crate::codec::{read_frame, write_frame, Frame};
use crate::envelope::{ClientCommand, Envelope};
use crate::publisher::EventPublisher;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, RwLock};
use tokio_stream::StreamExt;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("codec error: {0}")]
    Codec(#[from] crate::codec::WsCodecError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),
}

#[async_trait::async_trait]
pub trait ChannelAuthorizer: Send + Sync + 'static {
    async fn can_subscribe(&self, user_id: i64, ch: &str) -> bool;
}

/// Default authorizer: allows personal channel `u:{user_id}` and any channel
/// approved by custom logic.
pub struct AllowUserAndCallback<F> {
    callback: F,
}

impl<F> AllowUserAndCallback<F> {
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
}

#[async_trait::async_trait]
impl<F, Fut> ChannelAuthorizer for AllowUserAndCallback<F>
where
    F: Fn(i64, String) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = bool> + Send + 'static,
{
    async fn can_subscribe(&self, user_id: i64, ch: &str) -> bool {
        if ch == format!("u:{user_id}") {
            return true;
        }
        (self.callback)(user_id, ch.to_string()).await
    }
}

pub struct AllowAllAuthorizer;

#[async_trait::async_trait]
impl ChannelAuthorizer for AllowAllAuthorizer {
    async fn can_subscribe(&self, _user_id: i64, _ch: &str) -> bool {
        true
    }
}

#[async_trait::async_trait]
pub trait MoveHandler: Send + Sync + 'static {
    async fn handle_move(&self, user_id: i64, game_id: &str, uci: &str);
}

pub struct WsSession<A> {
    user_id: i64,
    publisher: EventPublisher,
    authorizer: Arc<A>,
    redis_client: redis::Client,
    move_handler: Option<Arc<dyn MoveHandler>>,
}

impl<A: ChannelAuthorizer> WsSession<A> {
    pub fn new(
        user_id: i64,
        publisher: EventPublisher,
        authorizer: Arc<A>,
        redis_client: redis::Client,
    ) -> Self {
        Self {
            user_id,
            publisher,
            authorizer,
            redis_client,
            move_handler: None,
        }
    }

    pub fn with_move_handler(mut self, handler: Arc<dyn MoveHandler>) -> Self {
        self.move_handler = Some(handler);
        self
    }

    pub fn set_move_handler(&mut self, handler: Arc<dyn MoveHandler>) {
        self.move_handler = Some(handler);
    }

    /// Run the WebSocket session over an upgraded stream until disconnect.
    pub async fn run<S>(self: Arc<Self>, stream: S)
    where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        let (mut reader, mut writer) = tokio::io::split(stream);
        let (outbound_tx, mut outbound_rx) = mpsc::channel::<Frame>(128);

        // Task 1: Dedicated writer loop
        let writer_handle = tokio::spawn(async move {
            while let Some(frame) = outbound_rx.recv().await {
                if write_frame(&mut writer, &frame).await.is_err() {
                    break;
                }
            }
        });

        // Set of active channel subscriptions (e.g. "u:123", "g:01JD...")
        let subscribed_channels = Arc::new(RwLock::new(HashSet::new()));
        {
            let mut set = subscribed_channels.write().await;
            set.insert(format!("u:{}", self.user_id));
        }

        // Task 2: Redis PubSub forwarder loop
        let user_id = self.user_id;
        let redis_client = self.redis_client.clone();
        let pubsub_outbound_tx = outbound_tx.clone();
        let pubsub_subs = Arc::clone(&subscribed_channels);

        let pubsub_handle = tokio::spawn(async move {
            let conn = match redis_client.get_async_pubsub().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("failed to create pubsub conn: {e}");
                    return;
                }
            };
            let mut pubsub = conn;

            // Pattern-subscribe to user's personal channel and all game channels
            let u_pattern = format!("ws:u:{user_id}");
            let g_pattern = "ws:g:*";

            if let Err(e) = pubsub.psubscribe(&u_pattern).await {
                tracing::error!("failed to psubscribe to {u_pattern}: {e}");
                return;
            }
            if let Err(e) = pubsub.psubscribe(g_pattern).await {
                tracing::error!("failed to psubscribe to {g_pattern}: {e}");
                return;
            }

            let mut msg_stream = pubsub.into_on_message();

            while let Some(msg) = msg_stream.next().await {
                let channel_name = msg.get_channel_name();
                // Strip "ws:" prefix to get logical channel (e.g. "u:123" or "g:01JD...")
                let logical_ch = if let Some(stripped) = channel_name.strip_prefix("ws:") {
                    stripped
                } else {
                    channel_name
                };

                // Check if client is subscribed to this channel
                let is_sub = {
                    let set = pubsub_subs.read().await;
                    set.contains(logical_ch)
                };

                if is_sub {
                    let payload: String = match msg.get_payload() {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    let frame = Frame::Text(payload);
                    if pubsub_outbound_tx.send(frame).await.is_err() {
                        break;
                    }
                }
            }
        });

        // Set user online presence in Redis
        let mut presence_conn = self
            .redis_client
            .get_multiplexed_async_connection()
            .await
            .ok();
        if let Some(ref mut c) = presence_conn {
            let _: Result<(), _> = redis::pipe()
                .cmd("SADD")
                .arg("presence:online")
                .arg(user_id)
                .cmd("SET")
                .arg(format!("presence:status:{user_id}"))
                .arg("idle")
                .query_async(c)
                .await;
        }

        // Task 3: Inbound reader loop
        while let Ok(Some(frame)) = read_frame(&mut reader).await {
            match frame {
                Frame::Ping(data) => {
                    let _ = outbound_tx.send(Frame::Pong(data)).await;
                }
                Frame::Pong(_) => {}
                Frame::Close(_) => {
                    let _ = outbound_tx
                        .send(Frame::Close(Some((1000, "normal".into()))))
                        .await;
                    break;
                }
                Frame::Text(text) => {
                    let cmd: ClientCommand = match serde_json::from_str(&text) {
                        Ok(c) => c,
                        Err(e) => {
                            let err_env = Envelope::new(
                                "0",
                                format!("u:{user_id}"),
                                "ws.error",
                                serde_json::json!({
                                    "code": "bad_message",
                                    "message": e.to_string(),
                                }),
                            );
                            if let Ok(json_str) = serde_json::to_string(&err_env) {
                                let _ = outbound_tx.send(Frame::Text(json_str)).await;
                            }
                            continue;
                        }
                    };

                    match cmd {
                        ClientCommand::Hello {
                            client: _,
                            last_ids,
                        } => {
                            let now_iso = time::OffsetDateTime::now_utc()
                                .format(&time::format_description::well_known::Rfc3339)
                                .unwrap_or_default();

                            let ack = Envelope::new(
                                "0",
                                format!("u:{user_id}"),
                                "hello.ack",
                                serde_json::json!({
                                    "user_id": user_id,
                                    "server_time": now_iso,
                                    "config_rev": 1,
                                }),
                            );
                            if let Ok(ack_json) = serde_json::to_string(&ack) {
                                let _ = outbound_tx.send(Frame::Text(ack_json)).await;
                            }

                            // Gap replay (ADR-004)
                            if let Some(cursors) = last_ids {
                                let mut resume_stats = HashMap::new();
                                for (ch, cursor) in cursors {
                                    if self.authorizer.can_subscribe(user_id, &ch).await {
                                        if let Ok(gap_events) =
                                            self.publisher.read_gap(&ch, &cursor, 100).await
                                        {
                                            resume_stats.insert(
                                                ch.clone(),
                                                serde_json::json!({
                                                    "from_id": cursor,
                                                    "replayed": gap_events.len(),
                                                }),
                                            );
                                            for ev in gap_events {
                                                if let Ok(ev_json) = serde_json::to_string(&ev) {
                                                    let _ = outbound_tx
                                                        .send(Frame::Text(ev_json))
                                                        .await;
                                                }
                                            }
                                        }
                                    }
                                }

                                let resume_ack = Envelope::new(
                                    "0",
                                    format!("u:{user_id}"),
                                    "resume.ack",
                                    serde_json::json!({ "per_channel": resume_stats }),
                                );
                                if let Ok(res_json) = serde_json::to_string(&resume_ack) {
                                    let _ = outbound_tx.send(Frame::Text(res_json)).await;
                                }
                            }
                        }
                        ClientCommand::Ping => {
                            let now_iso = time::OffsetDateTime::now_utc()
                                .format(&time::format_description::well_known::Rfc3339)
                                .unwrap_or_default();
                            let pong_json = serde_json::json!({
                                "type": "pong",
                                "data": { "ts": now_iso },
                            });
                            let _ = outbound_tx.send(Frame::Text(pong_json.to_string())).await;
                        }
                        ClientCommand::Subscribe { ch } => {
                            if self.authorizer.can_subscribe(user_id, &ch).await {
                                {
                                    let mut set = subscribed_channels.write().await;
                                    set.insert(ch.clone());
                                }
                                if ch.starts_with("g:") {
                                    if let Some(ref mut c) = presence_conn {
                                        let _: Result<(), _> = redis::cmd("SET")
                                            .arg(format!("presence:status:{user_id}"))
                                            .arg("playing")
                                            .query_async(c)
                                            .await;
                                    }
                                }

                                let ack = Envelope::new(
                                    "0",
                                    &ch,
                                    "subscribe.ack",
                                    serde_json::json!({ "ch": ch }),
                                );
                                if let Ok(ack_json) = serde_json::to_string(&ack) {
                                    let _ = outbound_tx.send(Frame::Text(ack_json)).await;
                                }
                            } else {
                                let err_env = Envelope::new(
                                    "0",
                                    &ch,
                                    "ws.error",
                                    serde_json::json!({
                                        "code": "auth.forbidden",
                                        "message": "not authorized to subscribe to channel",
                                    }),
                                );
                                if let Ok(err_json) = serde_json::to_string(&err_env) {
                                    let _ = outbound_tx.send(Frame::Text(err_json)).await;
                                }
                            }
                        }
                        ClientCommand::Unsubscribe { ch } => {
                            let mut set = subscribed_channels.write().await;
                            set.remove(&ch);
                        }
                        ClientCommand::GameMove { game_id, uci } => {
                            if let Some(handler) = &self.move_handler {
                                handler.handle_move(user_id, &game_id, &uci).await;
                            } else {
                                let ack = Envelope::new(
                                    "0",
                                    format!("g:{game_id}"),
                                    "game.move_received",
                                    serde_json::json!({
                                        "game_id": game_id,
                                        "uci": uci,
                                    }),
                                );
                                if let Ok(ack_json) = serde_json::to_string(&ack) {
                                    let _ = outbound_tx.send(Frame::Text(ack_json)).await;
                                }
                            }
                        }
                        ClientCommand::GameResign { game_id } => {
                            let ack = Envelope::new(
                                "0",
                                format!("g:{game_id}"),
                                "game.resigned",
                                serde_json::json!({
                                    "game_id": game_id,
                                    "user_id": user_id,
                                }),
                            );
                            if let Ok(ack_json) = serde_json::to_string(&ack) {
                                let _ = outbound_tx.send(Frame::Text(ack_json)).await;
                            }
                        }
                    }
                }
                Frame::Binary(_) => {}
            }
        }

        // Cleanup: remove presence from Redis
        if let Some(ref mut c) = presence_conn {
            let _: Result<(), _> = redis::pipe()
                .cmd("SREM")
                .arg("presence:online")
                .arg(user_id)
                .cmd("DEL")
                .arg(format!("presence:status:{user_id}"))
                .query_async(c)
                .await;
        }
        pubsub_handle.abort();
        writer_handle.abort();
    }
}
