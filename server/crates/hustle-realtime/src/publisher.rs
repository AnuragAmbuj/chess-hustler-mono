//! EventPublisher: Publishes envelopes to Redis Streams (durable resume)
//! and Redis PubSub (live fanout) per ADR-004.

use crate::envelope::Envelope;
use redis::AsyncCommands;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PublisherError {
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone)]
pub struct EventPublisher {
    redis: redis::Client,
}

impl EventPublisher {
    pub fn new(redis: redis::Client) -> Self {
        Self { redis }
    }

    /// Append an event to the durable Redis Stream and broadcast it over Redis PubSub.
    pub async fn publish(
        &self,
        ch: &str,
        event_type: &str,
        data: serde_json::Value,
    ) -> Result<Envelope, PublisherError> {
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

        let data_str = serde_json::to_string(&data)?;
        let mut conn = self.redis.get_multiplexed_async_connection().await?;

        // 1. Append to Redis Stream
        let stream_id: String = redis::cmd("XADD")
            .arg(ch)
            .arg("*")
            .arg("type")
            .arg(event_type)
            .arg("ts")
            .arg(&now)
            .arg("data")
            .arg(&data_str)
            .query_async(&mut conn)
            .await?;

        // Bounded retention (ADR-004): trim stream to ~2000 events
        let _: Result<i64, _> = redis::cmd("XTRIM")
            .arg(ch)
            .arg("MAXLEN")
            .arg("~")
            .arg(2000)
            .query_async(&mut conn)
            .await;

        let envelope = Envelope {
            id: stream_id,
            ts: now,
            ch: ch.to_string(),
            event_type: event_type.to_string(),
            data,
        };

        // 2. Broadcast to PubSub channel `ws:{ch}` for connected WebSocket sessions
        let env_json = serde_json::to_string(&envelope)?;
        let pubsub_ch = format!("ws:{ch}");
        let _: Result<i64, _> = conn.publish(pubsub_ch, &env_json).await;

        Ok(envelope)
    }

    /// Read gap events from a channel stream starting after `from_id`.
    /// Used during `hello` resume handshake.
    pub async fn read_gap(
        &self,
        ch: &str,
        from_id: &str,
        limit: usize,
    ) -> Result<Vec<Envelope>, PublisherError> {
        let mut conn = self.redis.get_multiplexed_async_connection().await?;

        // In Redis 6+, fetch range from `from_id` up to `+`
        let raw_val: redis::Value = redis::cmd("XRANGE")
            .arg(ch)
            .arg(from_id)
            .arg("+")
            .arg("COUNT")
            .arg(limit + 1)
            .query_async(&mut conn)
            .await?;

        let mut envelopes = Vec::new();
        if let redis::Value::Array(entries) = raw_val {
            for entry in entries {
                if let redis::Value::Array(ref parts) = entry {
                    if parts.len() >= 2 {
                        let id_str = match &parts[0] {
                            redis::Value::BulkString(bytes) => {
                                String::from_utf8_lossy(bytes).to_string()
                            }
                            _ => continue,
                        };

                        // Skip the start boundary if equal to from_id
                        if id_str == from_id {
                            continue;
                        }

                        let mut ev_type = String::new();
                        let mut ev_ts = String::new();
                        let mut ev_data = serde_json::Value::Null;

                        if let redis::Value::Array(ref kvs) = parts[1] {
                            for chunk in kvs.chunks(2) {
                                if chunk.len() == 2 {
                                    let key = match &chunk[0] {
                                        redis::Value::BulkString(b) => {
                                            String::from_utf8_lossy(b).to_string()
                                        }
                                        _ => continue,
                                    };
                                    let val = match &chunk[1] {
                                        redis::Value::BulkString(b) => {
                                            String::from_utf8_lossy(b).to_string()
                                        }
                                        _ => continue,
                                    };

                                    match key.as_str() {
                                        "type" => ev_type = val,
                                        "ts" => ev_ts = val,
                                        "data" => {
                                            if let Ok(parsed) = serde_json::from_str(&val) {
                                                ev_data = parsed;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }

                        if !ev_type.is_empty() {
                            envelopes.push(Envelope {
                                id: id_str,
                                ts: ev_ts,
                                ch: ch.to_string(),
                                event_type: ev_type,
                                data: ev_data,
                            });
                        }
                    }
                }
            }
        }

        Ok(envelopes)
    }
}
