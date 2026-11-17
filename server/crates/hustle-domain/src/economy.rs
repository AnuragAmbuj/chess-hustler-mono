//! Pure Gold-economy math (ADR-006). The DB layer (`hustle-gold`, Phase 1)
//! applies these results inside the ledger transaction; no IO here.
//! Default rates mirror `app_config` seeds and docs/04-economy/gold-economy.md.

#[derive(Debug, PartialEq, Eq)]
pub enum GoldError {
    /// Balance would go negative — structurally impossible to persist.
    Insufficient { balance: i64, needed: i64 },
}

/// Overdraft-proof balance arithmetic. The DB CHECK (balance >= 0) is the
/// enforcement backstop; this mirrors it so domain logic never produces a
/// negative result even in tests/simulations.
pub fn apply_delta(balance: i64, delta: i64) -> Result<i64, GoldError> {
    let next = balance + delta;
    if next < 0 {
        Err(GoldError::Insufficient {
            balance,
            needed: -delta,
        })
    } else {
        Ok(next)
    }
}

/// 7-day daily-streak ladder (day 1..=7); day 8 wraps to day 1 (gold-economy §3.2).
pub const STREAK_REWARDS: [i64; 7] = [500, 750, 1_000, 1_500, 2_000, 3_000, 5_000];

/// Reward for the Nth consecutive claim day (1-based). Missing a day resets the
/// counter upstream; a wrap after day 7 starts the ladder again.
pub fn streak_reward(consecutive_day: u32) -> i64 {
    debug_assert!(consecutive_day >= 1, "streak days are 1-based");
    STREAK_REWARDS[((consecutive_day - 1) % 7) as usize]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameOutcome {
    Win,
    Draw,
    Loss,
}

/// Gameplay earning rates (gold-economy §3.5).
#[derive(Debug, Clone, Copy)]
pub struct GameplayRates {
    pub win: i64,
    pub draw: i64,
    pub loss: i64,
    pub daily_cap: i64,
}

impl Default for GameplayRates {
    fn default() -> Self {
        Self {
            win: 25,
            draw: 10,
            loss: 5,
            daily_cap: 500,
        }
    }
}

/// Reward for one completed game (>= 8 plies enforced upstream), respecting the
/// per-day gameplay cap. Returns the granted amount (may be 0 once capped).
pub fn gameplay_reward(
    outcome: GameOutcome,
    rates: GameplayRates,
    already_earned_today: i64,
) -> i64 {
    let base = match outcome {
        GameOutcome::Win => rates.win,
        GameOutcome::Draw => rates.draw,
        GameOutcome::Loss => rates.loss,
    };
    (already_earned_today + base).min(rates.daily_cap) - already_earned_today
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overdraft_is_rejected_not_clamped() {
        assert_eq!(apply_delta(100, 50).unwrap(), 150);
        assert_eq!(
            apply_delta(100, -150),
            Err(GoldError::Insufficient {
                balance: 100,
                needed: 150
            })
        );
        assert_eq!(apply_delta(0, 0).unwrap(), 0);
    }

    #[test]
    fn streak_ladder_wraps_after_day_seven() {
        assert_eq!(streak_reward(1), 500);
        assert_eq!(streak_reward(7), 5_000);
        assert_eq!(streak_reward(8), 500);
        assert_eq!(streak_reward(14), 5_000);
    }

    #[test]
    fn gameplay_cap_is_respected() {
        let rates = GameplayRates::default();
        assert_eq!(gameplay_reward(GameOutcome::Win, rates, 0), 25);
        assert_eq!(gameplay_reward(GameOutcome::Loss, rates, 495), 5);
        assert_eq!(gameplay_reward(GameOutcome::Win, rates, 500), 0);
        assert_eq!(gameplay_reward(GameOutcome::Draw, rates, 10), 10);
    }
}
