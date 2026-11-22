//! hustle-rewards — Progression engine, rotating daily/weekly missions, and milestone achievements.

pub mod error;
pub mod models;
pub mod service;

pub use error::RewardError;
pub use models::{AchievementItem, ClaimMissionResult, GameFinishedEvent, MissionItem};
pub use service::{daily_period_key, weekly_period_key, RewardService};
