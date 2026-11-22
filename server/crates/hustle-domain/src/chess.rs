//! Chess domain vocabulary shared across crates (pools, time controls).
//! Rules execution lands in `hustle-chess` (Phase 1) behind `shakmaty`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pool {
    Bullet,
    Blitz,
    Rapid,
    Classical,
}

impl Pool {
    pub const ALL: [Pool; 4] = [Pool::Bullet, Pool::Blitz, Pool::Rapid, Pool::Classical];

    pub fn as_str(self) -> &'static str {
        match self {
            Pool::Bullet => "bullet",
            Pool::Blitz => "blitz",
            Pool::Rapid => "rapid",
            Pool::Classical => "classical",
        }
    }

    /// Canonical parse (DB strings, API payloads).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "bullet" => Some(Pool::Bullet),
            "blitz" => Some(Pool::Blitz),
            "rapid" => Some(Pool::Rapid),
            "classical" => Some(Pool::Classical),
            _ => None,
        }
    }
}

impl std::fmt::Display for Pool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeControl {
    pub initial_ms: u32,
    pub increment_ms: u32,
}

impl TimeControl {
    pub fn initial_s(&self) -> u32 {
        self.initial_ms / 1000
    }
}

/// Pool mapping per docs/01-architecture/game-lifecycle.md §3 (initial time
/// only; increments never shift pools).
pub fn pool_for_time_control(tc: &TimeControl) -> Pool {
    match tc.initial_s() {
        0..=179 => Pool::Bullet,
        180..=599 => Pool::Blitz,
        600..=1799 => Pool::Rapid,
        _ => Pool::Classical,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tc(s: u32) -> TimeControl {
        TimeControl {
            initial_ms: s * 1000,
            increment_ms: 0,
        }
    }

    #[test]
    fn pool_mapping_matches_lifecycle_doc() {
        assert_eq!(pool_for_time_control(&tc(60)), Pool::Bullet);
        assert_eq!(pool_for_time_control(&tc(179)), Pool::Bullet);
        assert_eq!(pool_for_time_control(&tc(180)), Pool::Blitz);
        assert_eq!(pool_for_time_control(&tc(599)), Pool::Blitz);
        assert_eq!(pool_for_time_control(&tc(600)), Pool::Rapid);
        assert_eq!(pool_for_time_control(&tc(1799)), Pool::Rapid);
        assert_eq!(pool_for_time_control(&tc(1800)), Pool::Classical);
    }
}
