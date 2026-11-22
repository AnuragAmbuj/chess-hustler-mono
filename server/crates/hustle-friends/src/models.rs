use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendPresence {
    pub online: bool,
    pub status: String, // "idle" | "playing" | "offline"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendInfo {
    pub id: i64,         // friendship id
    pub user_id: String, // public_id
    pub handle: String,
    pub display_name: String,
    pub avatar_cosmetic_id: Option<String>,
    pub rating: i32,
    pub presence: FriendPresence,
    #[serde(with = "time::serde::rfc3339::option")]
    pub accepted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendRequestItem {
    pub id: i64,
    pub handle: String,
    pub display_name: String,
    pub rating: i32,
    pub avatar_cosmetic_id: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendRequestsResponse {
    pub incoming: Vec<FriendRequestItem>,
    pub outgoing: Vec<FriendRequestItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedUserItem {
    pub handle: String,
    pub display_name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub blocked_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectChallenge {
    pub id: String,
    pub challenger_handle: String,
    pub challenger_rating: i32,
    pub target_handle: String,
    pub initial_s: u32,
    pub increment_s: u32,
    pub rated: bool,
    pub created_at: String,
    pub expires_at: String,
}
