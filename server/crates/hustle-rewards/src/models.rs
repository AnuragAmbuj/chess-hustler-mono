use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MissionItem {
    pub id: String,
    pub kind: String,
    pub title_key: String,
    pub goal_type: String,
    pub progress: i32,
    pub goal: i32,
    pub claimed: bool,
    pub reward_gold: i64,
    pub period_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimMissionResult {
    pub claimed: bool,
    pub amount: i64,
    pub new_balance: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AchievementItem {
    pub id: String,
    pub title_key: String,
    pub goal_type: String,
    pub progress: i64,
    pub target: i64,
    pub reward_gold: i64,
    pub unlocked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameFinishedEvent {
    pub white_user_id: i64,
    pub black_user_id: i64,
    pub winner_user_id: Option<i64>,
    pub checkmate: bool,
    pub draw: bool,
}
