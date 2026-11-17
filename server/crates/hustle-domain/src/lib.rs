//! hustle-domain — PURE business rules. Zero IO dependencies (ADR-001 rule 2):
//! no sqlx, no redis, no axum. Everything here is unit- and property-testable
//! without containers. The chess rules engine (`shakmaty`) is introduced behind
//! this boundary in Phase 1.

pub mod chess;
pub mod economy;
pub mod ratings;

pub use chess::{pool_for_time_control, Pool, TimeControl};
