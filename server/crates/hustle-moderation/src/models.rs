use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateReportRequest {
    pub category: String, // "cheating" | "abuse" | "inappropriate_name" | "other"
    pub target_handle: String,
    pub game_id: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportItem {
    pub id: i64,
    pub reporter_id: String, // public_id
    pub category: String,
    pub target_handle: String,
    pub game_id: Option<String>,
    pub note: Option<String>,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageItem {
    pub id: i64,
    pub game_id: String,
    pub user_id: String, // public_id
    pub handle: String,
    pub body: String,
    pub masked: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrievancePolicyLink {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrievanceInfo {
    pub officer_name: String,
    pub officer_title: String,
    pub officer_email: String,
    pub address: String,
    pub acknowledgment_sla_hours: u32,
    pub disposition_sla_days: u32,
    pub governing_rule: String,
    pub policies: Vec<GrievancePolicyLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyFlagItem {
    pub id: i64,
    pub kind: String, // 'engine_match' | 'move_time' | 'pair_farming' | 'draw_farming' | 'ad_farming' | 'multi_account' | 'rating_anomaly'
    pub subject_user_id: i64,
    pub subject_handle: Option<String>,
    pub related_user_id: Option<i64>,
    pub related_handle: Option<String>,
    pub evidence: serde_json::Value,
    pub score: f32,
    pub status: String, // 'open' | 'reviewing' | 'cleared' | 'confirmed'
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveFlagRequest {
    pub resolution: String, // "cleared" | "confirmed"
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanctionItem {
    pub id: i64,
    pub user_id: i64,
    pub handle: Option<String>,
    pub kind: String, // "warning" | "mute" | "mm_suspension" | "ad_suspension" | "suspension"
    pub reason: String,
    pub issued_by: String,
    pub evidence: Option<serde_json::Value>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub lifted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSanctionRequest {
    pub kind: String,
    pub reason: String,
    pub expires_at: Option<String>,
    pub evidence: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionReportRequest {
    pub action: String, // "actioned" | "dismissed"
    pub resolution: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminPlayerDetail {
    pub id: i64,
    pub public_id: String,
    pub handle: String,
    pub display_name: String,
    pub role: String,
    pub status: String,
    pub phone_e164: Option<String>,
    pub email: Option<String>,
    pub created_at: String,
    pub gold_balance: i64,
    pub ratings: serde_json::Value,
    pub active_sanctions: Vec<SanctionItem>,
    pub anomaly_flags: Vec<AnomalyFlagItem>,
    pub reports_received: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminAuditItem {
    pub id: i64,
    pub actor: String,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub payload: serde_json::Value,
    pub created_at: String,
}
