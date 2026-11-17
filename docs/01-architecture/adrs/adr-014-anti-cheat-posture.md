# ADR-014 — Anti-cheat: detect + human review; sanctions ladder; no stat-only auto-bans

**Status:** Proposed · **Date:** 2026-11-17

## Context
Chess cheating is adversarial and statistical. False positives destroy trust faster than cheaters do. Client-side anti-cheat is theater; server-side statistical detection has irreducible uncertainty.

## Decision
- **Layer 1 (deterministic):** server authority makes illegal moves, fake results, and clock manipulation impossible (ADR-003). This is the load-bearing defense.
- **Layer 2 (statistical, offline):** worker analyzes completed games for engine-match percentage (local Stockfish eval at shallow depth vs player moves), move-time regularity, opening-line divergence, and pairwise patterns (repeated same-opponent farming, draw farming, multi-account correlation via shared IP/device + win-ratio asymmetry). Findings land in `anomaly_flags` with evidence, never auto-actioned.
- **Layer 3 (human):** moderator queue with evidence bundles; sanctions ladder — warning → mute → matchmaking suspension → **ad-earning suspension** (economy-only, preserves play) → account suspension. Appeals channel exists (grievance officer).
- **Ad-farming:** per-account gates (account age, games played), per-IP session caps, device-anomaly stats → same flag queue; ad-earning suspension first (revenue protection without punishing play).
- Publish a periodic fairness summary (flags received/upheld/false) — transparency as trust strategy.

## Consequences
- + Wrongful bans are rare and appealable; appeals data improves detectors.
- − Moderation staffing needed before "growth mode"; sized in roadmap (P2 exit criteria).
- − Deterministic layer alone cannot stop engine assistance mid-game — accepted, industry-standard limitation.

## Alternatives
- Auto-ban on engine-match threshold: rejected (false-positive risk on blitz time-pressure play).
- Kernel-level client anti-cheat: rejected (privacy posture, DPDP minimal-processing, platform hostility).
- Do nothing beyond legality: rejected (economy ad-fraud alone justifies Layer 2/3).
