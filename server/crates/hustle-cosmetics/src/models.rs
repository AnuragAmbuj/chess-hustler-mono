use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CosmeticItem {
    pub id: String,
    pub category: String,
    pub name_key: String,
    pub price_gold: i64,
    pub rarity: String,
    pub preview: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_from: Option<OffsetDateTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_to: Option<OffsetDateTime>,
    pub active: bool,
    #[serde(default)]
    pub owned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseLedgerInfo {
    pub reason: String,
    pub delta: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseResult {
    pub owned: bool,
    pub balance: i64,
    pub ledger: PurchaseLedgerInfo,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, sqlx::FromRow)]
pub struct UserLoadout {
    pub board_id: Option<String>,
    pub pieces_id: Option<String>,
    pub avatar_id: Option<String>,
    pub frame_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateLoadoutRequest {
    #[serde(default)]
    pub board_id: Option<String>,
    #[serde(default)]
    pub pieces_id: Option<String>,
    #[serde(default)]
    pub avatar_id: Option<String>,
    #[serde(default)]
    pub frame_id: Option<String>,
}
