use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModerationError {
    #[error("Message exceeds maximum length of 200 characters")]
    MessageTooLong,

    #[error("Rate limit exceeded: {0}")]
    RateLimited(String),

    #[error("You are muted from in-game chat due to an active safety sanction")]
    Muted,

    #[error("You are not a player in this game")]
    NotInGame,

    #[error("Player not found: {0}")]
    UserNotFound(String),

    #[error("Game not found: {0}")]
    GameNotFound(String),

    #[error("Cannot interact due to user block")]
    Blocked,

    #[error("Invalid report category: {0}")]
    InvalidCategory(String),

    #[error("Invalid status: {0}")]
    InvalidStatus(String),

    #[error("Invalid kind: {0}")]
    InvalidKind(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),
}
