use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Tournament {
    pub id: i64,
    pub public_id: String,
    pub name: String,
    pub tc: serde_json::Value,
    pub rounds: i32,
    pub round_minutes: i32,
    pub status: String,
    pub starts_at: OffsetDateTime,
    pub registration_closes_at: OffsetDateTime,
    pub capacity: i32,
    pub prize_config: serde_json::Value,
    pub created_by: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TournamentEntry {
    pub tournament_id: i64,
    pub user_id: i64,
    pub joined_at: OffsetDateTime,
    pub withdrawn: bool,
    pub final_rank: Option<i32>,
    pub prize_gold: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TournamentPairing {
    pub id: i64,
    pub tournament_id: i64,
    pub round: i32,
    pub board: i32,
    pub white_id: Option<i64>,
    pub black_id: Option<i64>,
    pub game_id: Option<i64>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandingItem {
    pub rank: i32,
    pub user_id: i64,
    pub handle: String,
    pub points: f32,
    pub buchholz: f32,
    pub sonneborn_berger: f32,
    pub games_played: i32,
    pub wins: i32,
    pub draws: i32,
    pub losses: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTournamentRequest {
    pub name: String,
    pub tc: serde_json::Value,
    pub rounds: i32,
    pub round_minutes: i32,
    pub starts_at: OffsetDateTime,
    pub registration_closes_at: OffsetDateTime,
    pub capacity: i32,
    pub prize_config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentDetail {
    pub tournament: Tournament,
    pub entries_count: i64,
    pub my_entry: Option<TournamentEntry>,
    pub standings: Vec<StandingItem>,
    pub pairings: Vec<TournamentPairing>,
}

#[derive(Debug, Error)]
pub enum TournamentError {
    #[error("tournament not found: {0}")]
    NotFound(String),
    #[error("tournament is full")]
    Full,
    #[error("registration closed")]
    RegistrationClosed,
    #[error("already joined tournament")]
    AlreadyJoined,
    #[error("not registered in tournament")]
    NotJoined,
    #[error("invalid tournament state: {0}")]
    InvalidState(String),
    #[error("invalid round: {0}")]
    InvalidRound(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("gold ledger error: {0}")]
    Ledger(String),
}
