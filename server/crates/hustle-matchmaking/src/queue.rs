//! Redis queue data structures and atomic claim script.

use hustle_domain::{Pool, TimeControl};
use serde::{Deserialize, Serialize};

pub const QUEUE_TTL_SECONDS: u64 = 1800; // 30 minutes

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum QueueMode {
    Rated,
    Casual,
}

impl QueueMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            QueueMode::Rated => "rated",
            QueueMode::Casual => "casual",
        }
    }

    pub fn from_rated(rated: bool) -> Self {
        if rated {
            QueueMode::Rated
        } else {
            QueueMode::Casual
        }
    }

    pub fn is_rated(&self) -> bool {
        matches!(self, QueueMode::Rated)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueueEntry {
    pub user_id: i64,
    pub public_id: String,
    pub handle: String,
    pub pool: Pool,
    pub mode: QueueMode,
    pub tc: TimeControl,
    pub rating: f64,
    pub rd: f64,
    pub provisional: bool,
    pub enqueued_at_unix_s: i64,
}

pub fn pool_queue_key(pool: Pool, mode: QueueMode) -> String {
    format!("mm:queue:{}:{}", pool.as_str(), mode.as_str())
}

pub fn user_queue_key(user_id: i64) -> String {
    format!("mm:user:{user_id}")
}

/// Lua script for atomic claim of two queued players.
/// KEYS[1] = pool queue ZSET key
/// KEYS[2] = user p key (`mm:user:{p}`)
/// KEYS[3] = user q key (`mm:user:{q}`)
/// ARGV[1] = user_id p
/// ARGV[2] = user_id q
/// Returns 1 if claimed, 0 otherwise.
pub const ATOMIC_CLAIM_LUA: &str = r#"
local q_key = KEYS[1]
local u_p = KEYS[2]
local u_q = KEYS[3]
local id_p = ARGV[1]
local id_q = ARGV[2]

local s_p = redis.call('ZSCORE', q_key, id_p)
local s_q = redis.call('ZSCORE', q_key, id_q)

if s_p and s_q then
    redis.call('ZREM', q_key, id_p, id_q)
    redis.call('DEL', u_p, u_q)
    return 1
else
    return 0
end
"#;
