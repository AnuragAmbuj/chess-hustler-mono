use thiserror::Error;

#[derive(Debug, Error)]
pub enum CosmeticsError {
    #[error("cosmetic not found: {0}")]
    NotFound(String),

    #[error("cosmetic item is not currently available: {0}")]
    Unavailable(String),

    #[error("cosmetic already owned: {0}")]
    AlreadyOwned(String),

    #[error("insufficient gold balance: have {balance}, needed {needed}")]
    InsufficientFunds { balance: i64, needed: i64 },

    #[error("cannot equip unowned cosmetic: {0}")]
    NotOwned(String),

    #[error("cosmetic {item_id} has category {actual}, expected {expected}")]
    InvalidCategory {
        item_id: String,
        expected: &'static str,
        actual: String,
    },

    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("ledger error: {0}")]
    Ledger(#[from] hustle_gold::LedgerError),
}
