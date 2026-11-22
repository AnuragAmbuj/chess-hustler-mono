//! hustle-matchmaking — Redis queue, pairing sweeper, and atomic claim (ADR-004).
//!
//! Manages player matchmaking queues across pools and rating modes.
//! Pairs players using widening rating windows and performs atomic claims
//! via Redis Lua scripts before creating starting games in PostgreSQL.

pub mod queue;
pub mod service;
pub mod sweeper;

pub use queue::{QueueEntry, QueueMode};
pub use service::{MatchResult, MatchmakingError, MatchmakingService, QueueReceipt, QueueStatus};
pub use sweeper::{can_pair, rating_window};
