# ADR-008 — Identity: phone OTP + Google OAuth; short JWTs + rotating refresh; WS tickets

**Status:** Proposed · **Date:** 2026-11-17

## Context
India-first signup must not require email or passwords (friction), but we need durable identity, per-device sessions, and cheap revocation. WS URLs must not carry long-lived credentials.

## Decision
- **Signup/login:** mobile OTP (MSG91 provider behind a port; 6-digit, hashed at rest, 5-min TTL, 3/15-min per phone, 5/hour per IP) **or** Google Sign-In (id_token verified against JWKS). Optional email add later. Minimum friction, no password store.
- **Sessions:** access JWT (15 min, HS256, kid-rotated secret) + per-device **refresh token with rotation and reuse detection** (reuse → revoke whole token family). `user_sessions` tracks device, refresh hash, last seen.
- **WS:** REST `GET /v1/realtime/ticket` → 30 s single-use ticket (Redis), used once at upgrade. No token in WS query strings beyond the ticket.
- **DPDP consent** is captured at the same signup transaction (`consent_records`: purpose-level toggles, policy versions, timestamp, IP) — consent and identity are born together.
- Account deletion: request → 30-day grace (reversible) → pseudonymization preserving game-history integrity.

## Consequences
- + Phone-OTP friction is minimal; Google covers users who avoid OTP SMS; no password reset flows ever.
- − OTP SMS cost (~₹0.15–0.25/msg) is a per-signup cost line item; per-IP throttling prevents pump attacks.
- − JWT revocation lag (≤ 15 min) is acceptable; sanctions that need instant effect use Redis session-blocklist keyed by session id (checked on every request).

## Alternatives
- Passwords: rejected (storage/reset burden, weak practice in target market).
- Magic links: rejected (email penetration low in target segment vs phone).
- Long-lived JWTs in WS URLs: rejected (leak surface).
