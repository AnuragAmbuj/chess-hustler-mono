use thiserror::Error;

#[derive(Debug, Error)]
pub enum RewardError {
    #[error("mission not found or not active: {0}")]
    MissionNotFound(String),

    #[error("mission goal not reached: have {current}, need {goal}")]
    NotCompleted { current: i32, goal: i32 },

    #[error("mission already claimed for this period: {0}")]
    AlreadyClaimed(String),

    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("ledger error: {0}")]
    Ledger(#[from] hustle_gold::LedgerError),
}
