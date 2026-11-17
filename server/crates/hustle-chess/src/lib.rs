//! hustle-chess — chess rules execution (pure) and the game service layer.
//!
//! This crate owns the `games` and `moves` tables (ADR-001). The `rules` and
//! `clock` modules are PURE: no IO, no sqlx — fully unit/property testable.
//! The service layer (Phase 1 slice 2) applies them inside DB transactions.

pub mod clock;
pub mod rules;

pub use rules::{AutoOutcome, RulesEngine, Termination};
