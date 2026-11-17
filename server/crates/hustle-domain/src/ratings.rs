//! Glicko-2 (Glickman, *"Example of the Glicko-2 system"*, 2012).
//!
//! Pure implementation. The golden test reproduces the worked example from the
//! paper (r=1500, RD=200, σ=0.06; opponents 1400/30 win, 1550/100 loss,
//! 1700/300 loss; τ=0.5 → r'≈1464.06, RD'≈151.52, σ'≈0.059996).
//! Applied server-side per pool in the game-finalization transaction (ADR-005).

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlickoRating {
    /// r — rating on the 1500-centered scale.
    pub rating: f64,
    /// RD — rating deviation.
    pub deviation: f64,
    /// σ — volatility.
    pub volatility: f64,
}

/// Default for a brand-new player in a pool (docs PRD RA-1 / schema §6).
pub const DEFAULT_RATING: GlickoRating = GlickoRating {
    rating: 1200.0,
    deviation: 350.0,
    volatility: 0.06,
};

/// System constant τ (volatility change limiter). Paper example value.
const TAU: f64 = 0.5;
const SCALE: f64 = 173.7178;

#[derive(Debug, Clone, Copy)]
pub struct Opponent {
    /// Opponent's pre-game rating facts.
    pub rating: GlickoRating,
    /// 1.0 = win, 0.5 = draw, 0.0 = loss (from *this* player's perspective).
    pub score: f64,
}

impl GlickoRating {
    /// Platform bounds per schema CHECK (rd between 30 and 350).
    pub fn clamped(self) -> Self {
        Self {
            rating: self.rating,
            deviation: self.deviation.clamp(30.0, 350.0),
            volatility: self.volatility,
        }
    }
}

/// A player with fewer than 8 rated games in a pool is provisional (MM-2 / RA-2).
pub fn is_provisional(r: &GlickoRating) -> bool {
    r.deviation >= 350.0
}

fn g(phi: f64) -> f64 {
    let pi_sq = std::f64::consts::PI * std::f64::consts::PI;
    1.0 / (1.0 + 3.0 * phi * phi / pi_sq).sqrt()
}

fn expected(mu: f64, mu_j: f64, phi_j: f64) -> f64 {
    1.0 / (1.0 + (-g(phi_j) * (mu - mu_j)).exp())
}

/// Advance one rating period: applies the given opponent results.
/// A player's games within one finalization batch count as one period.
pub fn update(player: GlickoRating, opponents: &[Opponent]) -> GlickoRating {
    if opponents.is_empty() {
        // No games this period: only deviation grows (paper §5: φ' = sqrt(φ² + σ²)).
        let mu = (player.rating - 1500.0) / SCALE;
        let phi = player.deviation / SCALE;
        let phi_prime = (phi * phi + player.volatility * player.volatility).sqrt();
        return GlickoRating {
            rating: mu * SCALE + 1500.0,
            deviation: phi_prime * SCALE,
            volatility: player.volatility,
        }
        .clamped();
    }

    let mu = (player.rating - 1500.0) / SCALE;
    let phi = player.deviation / SCALE;

    let mut v_inverse_sum = 0.0;
    let mut outcome_sum = 0.0;
    for opp in opponents {
        let mu_j = (opp.rating.rating - 1500.0) / SCALE;
        let phi_j = opp.rating.deviation / SCALE;
        let g_j = g(phi_j);
        let e_j = expected(mu, mu_j, phi_j);
        v_inverse_sum += g_j * g_j * e_j * (1.0 - e_j);
        outcome_sum += g_j * (opp.score - e_j);
    }
    let v = 1.0 / v_inverse_sum;
    let delta = v * outcome_sum;

    let sigma_prime = compute_new_volatility(player.volatility, delta, phi, v);
    let phi_star = (phi * phi + sigma_prime * sigma_prime).sqrt();
    let phi_prime = 1.0 / (1.0 / (phi_star * phi_star) + 1.0 / v).sqrt();
    let mu_prime = mu + phi_prime * phi_prime * outcome_sum;

    GlickoRating {
        rating: mu_prime * SCALE + 1500.0,
        deviation: phi_prime * SCALE,
        volatility: sigma_prime,
    }
    .clamped()
}

/// f(x) from the paper's step 5.4b.
fn volatility_f(x: f64, delta: f64, phi: f64, v: f64, a: f64) -> f64 {
    let ex = x.exp();
    let num = ex * (delta * delta - phi * phi - v - ex);
    let den = 2.0 * (phi * phi + v + ex) * (phi * phi + v + ex);
    num / den - (x - a) / (TAU * TAU)
}

/// Illinois-method root find for the new volatility (paper steps 5.4a–5.4e).
fn compute_new_volatility(sigma: f64, delta: f64, phi: f64, v: f64) -> f64 {
    let eps = 1e-6;
    let a0 = (sigma * sigma).ln();
    let delta_sq = delta * delta;
    let phi_sq = phi * phi;

    let mut a_cur = a0;
    let mut b;
    if delta_sq > phi_sq + v {
        b = (delta_sq - phi_sq - v).ln();
    } else {
        let mut k = 1.0;
        b = a0 - k * TAU;
        while volatility_f(b, delta, phi, v, a0) < 0.0 {
            k += 1.0;
            b = a0 - k * TAU;
        }
    }

    let mut f_a = volatility_f(a_cur, delta, phi, v, a0);
    let mut f_b = volatility_f(b, delta, phi, v, a0);
    while (b - a_cur).abs() > eps {
        let c = a_cur + (a_cur - b) * f_a / (f_b - f_a);
        let f_c = volatility_f(c, delta, phi, v, a0);
        if f_c * f_b <= 0.0 {
            a_cur = b;
            f_a = f_b;
        } else {
            f_a /= 2.0;
        }
        b = c;
        f_b = f_c;
    }
    (a_cur / 2.0).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opp(r: f64, rd: f64, score: f64) -> Opponent {
        Opponent {
            rating: GlickoRating {
                rating: r,
                deviation: rd,
                volatility: 0.06,
            },
            score,
        }
    }

    #[test]
    fn paper_worked_example_matches_reference() {
        let player = GlickoRating {
            rating: 1500.0,
            deviation: 200.0,
            volatility: 0.06,
        };
        let opponents = vec![
            opp(1400.0, 30.0, 1.0),
            opp(1550.0, 100.0, 0.0),
            opp(1700.0, 300.0, 0.0),
        ];
        let out = update(player, &opponents);
        assert!(
            (out.rating - 1464.06).abs() < 0.05,
            "rating = {}",
            out.rating
        );
        assert!(
            (out.deviation - 151.52).abs() < 0.05,
            "rd = {}",
            out.deviation
        );
        assert!(
            (out.volatility - 0.059996).abs() < 0.0005,
            "sigma = {}",
            out.volatility
        );
    }

    #[test]
    fn no_games_grows_deviation_only() {
        let player = GlickoRating {
            rating: 1500.0,
            deviation: 200.0,
            volatility: 0.06,
        };
        let out = update(player, &[]);
        assert!((out.rating - 1500.0).abs() < 1e-9);
        assert!(out.deviation > 200.0 && out.deviation < 201.0);
        assert_eq!(out.volatility, 0.06);
    }

    #[test]
    fn defaults_are_provisional_and_bounded() {
        assert!(is_provisional(&DEFAULT_RATING));
        let clamped = GlickoRating {
            rating: 2500.0,
            deviation: 5.0,
            volatility: 0.06,
        }
        .clamped();
        assert_eq!(clamped.deviation, 30.0);
    }
}
