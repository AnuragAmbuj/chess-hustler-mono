# ADR-007 — Rewarded ads: server-side verification (SSV) behind a provider port

**Status:** Proposed · **Date:** 2026-11-17

## Context
Rewarded ads are the only v1 revenue. Client-reported rewards would be trivially faked (curl "I watched an ad"). Ad networks differ in callback formats and signing schemes. The economics (2,000 Gold / ad, 5/day) are an experiment that must be re-tunable without deploys.

## Decision
- **Server-side verification only.** The client's role: request an ad session (server checks eligibility/quota/cooldown, mints `session_id` + nonce), render the ad with the session attached as SSV custom data, and display results pushed by the server. Grant happens **only** in the provider's signed callback handler.
- Port trait `AdVerifier { verify(raw_callback) -> VerifiedReward }` with implementations per provider; **AdMob SSV** first (Android/iOS; mature public-key verifier spec). Unity LevelPlay / ironSource SSV can be added without touching economy code.
- Callback handler is: signature-verified (provider public keys fetched + cached), TTL-checked, `transaction_id`-deduped (unique index), quota re-checked under row lock, granted via idem key `ads:{provider}:{transaction_id}`.
- All knobs (gold_per_ad, max_per_day, cooldown, eligibility, rollout %) in `app_config` (ADR experiment flags), not code.
- Ads never interrupt games: server refuses ad sessions while the user has an active game; client UX never offers one mid-game.

## Consequences
- + Replay/forgery/retry-safe by construction; farmers face per-account gates + IP caps + anomaly flags.
- − Requires per-platform SDK work (Android first); web has no rewarded provider at v1 (documented platform gap; PRD AD-1).
- − Callback endpoint is public and unauthenticated-by-JWT (provider-signed instead) — hardened and rate-limited separately (threat-model T-08).

## Alternatives
- Client reports + sampled audit: rejected (anti-goal A1 — trusts client outcomes).
- Non-rewarded display ads only: rejected — materially weaker economics for a rewards-driven product; revisitable for web later.
- Subscription: explicitly out of scope for v1 monetization (mandate).
