//! Pure server-authoritative clock math (lifecycle §3 — normative).
//!
//! The server is the ONLY clock that terminates games (I-3). Clients render
//! `(server_ts, remaining)`; they never decide flags. All values are unix
//! milliseconds; `i64` arithmetic with saturating helpers — a malformed
//! timestamp can never invert time.

use serde::{Deserialize, Serialize};

/// Minimum interval between accepted moves of the same player (anti-spam).
pub const MIN_MOVE_INTERVAL_MS: i64 = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    White,
    Black,
}

impl Side {
    pub fn opponent(self) -> Side {
        match self {
            Side::White => Side::Black,
            Side::Black => Side::White,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Side::White => "white",
            Side::Black => "black",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockError {
    /// The mover's clock already hit zero — service finalizes timeout instead.
    Flagged,
    /// Server-side timestamp sanity (now before last tick).
    BadClock,
}

/// Remaining time per side + the anchor timestamp of the last accepted
/// mutation (or `game.start`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockState {
    pub white_ms: i64,
    pub black_ms: i64,
    pub last_tick_ms: i64,
}

impl ClockState {
    pub fn start(initial_ms: i64, now_ms: i64) -> Self {
        Self {
            white_ms: initial_ms,
            black_ms: initial_ms,
            last_tick_ms: now_ms,
        }
    }

    fn ms(&self, side: Side) -> i64 {
        match side {
            Side::White => self.white_ms,
            Side::Black => self.black_ms,
        }
    }

    fn set(&mut self, side: Side, value_ms: i64) {
        match side {
            Side::White => self.white_ms = value_ms,
            Side::Black => self.black_ms = value_ms,
        }
    }

    /// Remaining for `side`. Only the side to move is ticking; the other side's
    /// value is frozen until they move (lifecycle §3).
    pub fn remaining(&self, side: Side, to_move: Side, now_ms: i64) -> i64 {
        let stored = self.ms(side);
        if side == to_move {
            (stored - (now_ms - self.last_tick_ms)).max(0)
        } else {
            stored.max(0)
        }
    }

    /// Deadline of the side to move — ZSET score for `clocks:active`.
    pub fn deadline(&self, to_move: Side) -> i64 {
        self.last_tick_ms + self.ms(to_move)
    }

    /// Apply a completed move by `mover` at `now_ms`: deduct elapsed, add
    /// increment, advance the anchor. Rejects a move after flag (service then
    /// finalizes timeout — flagging is normally the sweeper's job).
    pub fn advance(
        self,
        mover: Side,
        now_ms: i64,
        increment_ms: i64,
    ) -> Result<ClockState, ClockError> {
        let elapsed = now_ms - self.last_tick_ms;
        if elapsed < 0 {
            return Err(ClockError::BadClock);
        }
        let remaining = self.ms(mover) - elapsed;
        if remaining <= 0 {
            return Err(ClockError::Flagged);
        }
        let mut next = self;
        next.set(mover, remaining + increment_ms);
        next.last_tick_ms = now_ms;
        Ok(next)
    }

    /// Rate-limit check: `now_ms` must be ≥ `MIN_MOVE_INTERVAL_MS` after the
    /// last accepted move of this player.
    pub fn rate_ok(last_accepted_ms: i64, now_ms: i64) -> bool {
        now_ms - last_accepted_ms >= MIN_MOVE_INTERVAL_MS
    }
}

/// What the sweeper decides at a deadline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagDecision {
    /// Side to move loses on time (opponent retains mating material).
    Win(Side),
    /// Draw — opponent has insufficient mating material (incl. helpmate).
    Draw,
}

/// Pure flag resolution given precomputed remaining time and the mating-material
/// fact (computed by the rules engine — keeps this module chess-free).
pub fn resolve_flag(
    remaining_to_move_ms: i64,
    opponent_can_mate: bool,
    to_move: Side,
) -> Option<FlagDecision> {
    if remaining_to_move_ms > 0 {
        return None; // not flagged
    }
    Some(if opponent_can_mate {
        FlagDecision::Win(to_move.opponent())
    } else {
        FlagDecision::Draw
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(w: i64, b: i64, tick: i64) -> ClockState {
        ClockState {
            white_ms: w,
            black_ms: b,
            last_tick_ms: tick,
        }
    }

    #[test]
    fn start_sets_both_to_initial() {
        let c = ClockState::start(180_000, 1_000);
        assert_eq!(c.white_ms, 180_000);
        assert_eq!(c.black_ms, 180_000);
        assert_eq!(c.deadline(Side::White), 181_000);
    }

    #[test]
    fn advance_deducts_elapsed_and_adds_increment() {
        // 180+2: white thinks 1.5s on move 1
        let c = state(180_000, 180_000, 0)
            .advance(Side::White, 1_500, 2_000)
            .unwrap();
        assert_eq!(c.white_ms, 180_500); // 180000 - 1500 + 2000
        assert_eq!(c.black_ms, 180_000); // frozen
        assert_eq!(c.last_tick_ms, 1_500);
        // black replies 2s later
        let c = c.advance(Side::Black, 3_500, 2_000).unwrap();
        assert_eq!(c.black_ms, 180_000);
        assert_eq!(c.white_ms, 180_500); // untouched until white moves
    }

    #[test]
    fn move_after_flag_is_rejected() {
        let c = state(1_000, 100_000, 0);
        assert_eq!(
            c.advance(Side::White, 1_001, 2_000),
            Err(ClockError::Flagged)
        );
    }

    #[test]
    fn remaining_clamps_at_zero_for_ticking_side_only() {
        let c = state(100, 200, 0);
        assert_eq!(c.remaining(Side::White, Side::White, 1_000), 0);
        assert_eq!(c.remaining(Side::Black, Side::White, 1_000), 200);
    }

    #[test]
    fn deadline_is_anchor_plus_remaining() {
        let c = state(180_000, 90_000, 5_000);
        assert_eq!(c.deadline(Side::White), 185_000);
        assert_eq!(c.deadline(Side::Black), 95_000);
    }

    #[test]
    fn rate_limit_requires_40ms() {
        assert!(!ClockState::rate_ok(1_000, 1_039));
        assert!(ClockState::rate_ok(1_000, 1_040));
        assert!(ClockState::rate_ok(1_000, 1_500));
    }

    #[test]
    fn negative_elapsed_is_rejected() {
        let c = state(180_000, 180_000, 1_000);
        assert_eq!(
            c.advance(Side::White, 999, 2_000),
            Err(ClockError::BadClock)
        );
    }

    #[test]
    fn flag_resolution_matrix() {
        // not yet flagged
        assert_eq!(resolve_flag(1, true, Side::White), None);
        // flagged, opponent can mate → win for opponent
        assert_eq!(
            resolve_flag(0, true, Side::White),
            Some(FlagDecision::Win(Side::Black))
        );
        assert_eq!(
            resolve_flag(-5, true, Side::Black),
            Some(FlagDecision::Win(Side::White))
        );
        // flagged, opponent cannot mate → draw
        assert_eq!(
            resolve_flag(0, false, Side::White),
            Some(FlagDecision::Draw)
        );
        assert_eq!(
            resolve_flag(-1, false, Side::Black),
            Some(FlagDecision::Draw)
        );
    }

    #[test]
    fn opponent_helper() {
        assert_eq!(Side::White.opponent(), Side::Black);
        assert_eq!(Side::Black.opponent(), Side::White);
    }
}
