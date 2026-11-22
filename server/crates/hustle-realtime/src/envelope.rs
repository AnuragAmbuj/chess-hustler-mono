//! WebSocket Envelopes and Inbound Client Commands (websocket.md §2 & §4).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Normative server→client envelope (ADR-004, websocket.md §2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Envelope {
    /// Redis stream ID of the channel — the resume cursor (e.g. "1731837164-0")
    pub id: String,
    /// ISO-8601 UTC timestamp
    pub ts: String,
    /// Channel name: "u:<user_id>" | "g:<game_id>" | "t:<tournament_id>"
    pub ch: String,
    /// Namespaced event type (e.g. "game.start", "game.move", "matchmaking.matched")
    #[serde(rename = "type")]
    pub event_type: String,
    /// Structured payload
    pub data: serde_json::Value,
}

impl Envelope {
    pub fn new(
        id: impl Into<String>,
        ch: impl Into<String>,
        event_type: impl Into<String>,
        data: serde_json::Value,
    ) -> Self {
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
        Self {
            id: id.into(),
            ts: now,
            ch: ch.into(),
            event_type: event_type.into(),
            data,
        }
    }
}

/// Inbound client command envelopes sent over WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
pub enum ClientCommand {
    #[serde(rename = "hello")]
    Hello {
        #[serde(default)]
        client: Option<String>,
        #[serde(default)]
        last_ids: Option<HashMap<String, String>>,
    },
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "subscribe")]
    Subscribe { ch: String },
    #[serde(rename = "unsubscribe")]
    Unsubscribe { ch: String },
    #[serde(rename = "game.move")]
    GameMove { game_id: String, uci: String },
    #[serde(rename = "game.resign")]
    GameResign { game_id: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_serialization_roundtrip() {
        let env = Envelope::new(
            "1001-0",
            "u:01JD",
            "matchmaking.matched",
            serde_json::json!({"game_id": "01JDQ7", "color": "white"}),
        );
        let serialized = serde_json::to_string(&env).unwrap();
        let parsed: Envelope = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed.id, "1001-0");
        assert_eq!(parsed.ch, "u:01JD");
        assert_eq!(parsed.event_type, "matchmaking.matched");
        assert_eq!(parsed.data["color"], "white");
    }

    #[test]
    fn client_command_hello_deserialization() {
        let json = r#"{"type":"hello","data":{"client":"web@1.0.0","last_ids":{"u:123":"0-0"}}}"#;
        let cmd: ClientCommand = serde_json::from_str(json).unwrap();
        match cmd {
            ClientCommand::Hello { client, last_ids } => {
                assert_eq!(client.as_deref(), Some("web@1.0.0"));
                assert_eq!(last_ids.unwrap().get("u:123").unwrap(), "0-0");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn client_command_ping_deserialization() {
        let json = r#"{"type":"ping"}"#;
        let cmd: ClientCommand = serde_json::from_str(json).unwrap();
        assert_eq!(cmd, ClientCommand::Ping);
    }
}
