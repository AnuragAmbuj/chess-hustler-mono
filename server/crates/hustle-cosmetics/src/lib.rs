//! hustle-cosmetics — Cosmetics catalog, store purchases, and loadout management.
//! Primary economic sink for earned Gold (ADR-006 & ADR-011).

pub mod error;
pub mod models;
pub mod service;

pub use error::CosmeticsError;
pub use models::{
    CosmeticItem, PurchaseLedgerInfo, PurchaseResult, UpdateLoadoutRequest, UserLoadout,
};
pub use service::CosmeticsService;
