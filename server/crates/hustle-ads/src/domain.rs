use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdSession {
    pub id: i64,
    pub public_id: String,
    pub user_id: i64,
    pub provider: String,
    pub status: String,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdSessionResponse {
    pub session_id: String,
    pub provider: String,
    pub reward_gold: i64,
    pub remaining_today: i32,
    pub cooldown_s: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdEligibility {
    pub eligible: bool,
    pub reason: Option<String>,
    pub account_age_ok: bool,
    pub games_completed_ok: bool,
    pub no_active_game: bool,
    pub not_sanctioned: bool,
    pub age_ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdStatus {
    pub enabled: bool,
    pub gold_per_ad: i64,
    pub max_per_day: i32,
    pub used_today: i32,
    pub cooldown_until: Option<OffsetDateTime>,
    pub eligibility: AdEligibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawCallback {
    pub query_string: String,
    pub params: HashMap<String, String>,
}

impl RawCallback {
    pub fn from_query_string(query_string: &str) -> Self {
        let mut params = HashMap::new();
        for pair in query_string.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                if let (Ok(dk), Ok(dv)) = (
                    urlencoding_decode(k),
                    urlencoding_decode(v),
                ) {
                    params.insert(dk, dv);
                } else {
                    params.insert(k.to_string(), v.to_string());
                }
            }
        }
        Self {
            query_string: query_string.to_string(),
            params,
        }
    }
}

fn urlencoding_decode(s: &str) -> Result<String, ()> {
    // Basic url-decoder for query strings
    let mut out = String::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        match b {
            b'+' => out.push(' '),
            b'%' => {
                let h1 = bytes.next().ok_or(())?;
                let h2 = bytes.next().ok_or(())?;
                let hex_str = [h1, h2];
                let hex_val = std::str::from_utf8(&hex_str).map_err(|_| ())?;
                let byte = u8::from_str_radix(hex_val, 16).map_err(|_| ())?;
                out.push(byte as char);
            }
            _ => out.push(b as char),
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedReward {
    pub provider: String,
    pub transaction_id: String,
    pub user_id: i64,
    pub session_id: String,
    pub raw_digest: [u8; 32],
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AdReject {
    #[error("invalid signature")]
    InvalidSignature,
    #[error("expired timestamp: {0}")]
    ExpiredTimestamp(String),
    #[error("unknown key id: {0}")]
    UnknownKeyId(String),
    #[error("malformed callback: {0}")]
    MalformedCallback(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("session expired: {0}")]
    SessionExpired(String),
    #[error("session already resolved: {0}")]
    SessionInvalidStatus(String),
    #[error("user mismatch: callback user_id does not match session user_id")]
    UserMismatch,
    #[error("replay detected: transaction already processed")]
    Replay,
}

#[derive(Debug, Error)]
pub enum AdError {
    #[error("ad reject: {0}")]
    Reject(#[from] AdReject),
    #[error("in active game")]
    InActiveGame,
    #[error("ineligible: {0}")]
    Ineligible(String),
    #[error("quota exceeded for today (max {max_per_day})")]
    QuotaExceeded { max_per_day: i32, used: i32 },
    #[error("cooldown active: {0}s remaining")]
    Cooldown(u64),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("ledger error: {0}")]
    Ledger(String),
}
