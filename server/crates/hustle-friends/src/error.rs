use thiserror::Error;

#[derive(Debug, Error)]
pub enum FriendError {
    #[error("cannot perform friend or block action on yourself")]
    SelfActionNotAllowed,

    #[error("user not found: {0}")]
    UserNotFound(String),

    #[error("already friends with user")]
    AlreadyFriends,

    #[error("friend request already pending")]
    RequestAlreadyPending,

    #[error("action blocked due to user block")]
    BlockedByUser,

    #[error("friend request not found: {0}")]
    RequestNotFound(i64),

    #[error("challenge not found or expired")]
    ChallengeNotFound,

    #[error("not authorized to perform action on this resource")]
    NotAuthorized,

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}
