//! TicketStore: 30s single-use WebSocket tickets stored in Redis (ADR-008).

use redis::AsyncCommands;
use thiserror::Error;
use ulid::Ulid;

#[derive(Debug, Error)]
pub enum TicketError {
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("invalid user id stored in ticket: {0}")]
    InvalidUserId(#[from] std::num::ParseIntError),
}

const TICKET_TTL_SECONDS: u64 = 30;

const CONSUME_TICKET_LUA: &str = r#"
local val = redis.call('GET', KEYS[1])
if val then
    redis.call('DEL', KEYS[1])
    return val
else
    return nil
end
"#;

#[derive(Clone)]
pub struct TicketStore {
    redis: redis::Client,
}

impl TicketStore {
    pub fn new(redis: redis::Client) -> Self {
        Self { redis }
    }

    /// Issue a single-use 30s ticket for a given user.
    pub async fn issue_ticket(&self, user_id: i64) -> Result<String, TicketError> {
        let ticket = Ulid::new().to_string();
        let key = format!("ticket:{ticket}");
        let mut conn = self.redis.get_multiplexed_async_connection().await?;

        // SET key val EX 30 NX
        let _: () = conn.set_ex(&key, user_id, TICKET_TTL_SECONDS).await?;

        Ok(ticket)
    }

    /// Consume a ticket atomically.
    /// Returns `Some(user_id)` on first valid consumption within 30s.
    /// Returns `None` if ticket is expired, not found, or already consumed.
    pub async fn consume_ticket(&self, ticket: &str) -> Result<Option<i64>, TicketError> {
        let key = format!("ticket:{ticket}");
        let mut conn = self.redis.get_multiplexed_async_connection().await?;

        let script = redis::Script::new(CONSUME_TICKET_LUA);
        let val: Option<String> = script.key(&key).invoke_async(&mut conn).await?;

        match val {
            Some(s) => {
                let user_id = s.parse::<i64>()?;
                Ok(Some(user_id))
            }
            None => Ok(None),
        }
    }
}
