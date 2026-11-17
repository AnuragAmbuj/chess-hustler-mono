# ADR-005 — Glicko-2 ratings, per-time-control pools

**Status:** Proposed · **Date:** 2026-11-17

## Context
A chess platform needs ratings that behave sensibly for brand-new players (wild rating swings for accurate early placement) and remain trusted at high level. Elo's fixed K-factor mishandles uncertainty; we also need per-pool separation because skill doesn't transfer across time controls.

## Decision
- **Glicko-2** (pure implementation in `hustle-domain::ratings`, property-tested against reference vectors).
- Per-pool ratings: bullet, blitz, rapid, classical. Initial rating 1200, RD 350 (provisional until < 8 games in that pool), volatility 0.06 baseline. Provisional players are paired preferentially together (MM-2).
- Rated results require ≥ 4 plies (else auto-abort path); timeouts count as decisive losses except the insufficient-mating-material draw rule.
- Weekly top-50 leaderboard snapshots per pool + personal peak tracking; no seasonal rating resets.
- Rating math is applied in the game-finalization TXN (row-locked), so concurrent games for one player serialize safely.

## Consequences
- + New players converge fast; experienced players' ratings are stable and fair.
- − Four pools × display complexity (UI shows pool selector with per-pool rating).
- − Slightly more math than Elo, but isolated in pure code with golden tests.

## Alternatives
- Elo (fixed K): rejected — provably worse early-game convergence.
- Glicko-1: rejected — Glicko-2's volatility model is the chess-standard choice.
- TrueSkill: rejected — multi-player emphasis unneeded; heavier to explain to users.
