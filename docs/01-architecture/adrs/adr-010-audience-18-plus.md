# ADR-010 — Audience: 18+ only at launch; Family mode deferred

**Status:** Proposed (owner-approved) · **Date:** 2026-11-17

## Context
DPDP Act 2023 treats under-18s as *children*: verifiable parental consent for processing, and a prohibition on tracking, behavioural monitoring, and **targeted advertising directed at children**. Rewarded ads + behavioural analytics are core to v1 monetization. Serving minors would require a consent-verification stack, an ad-restricted mode, heavier moderation, and store Families-policy obligations — all before product-market fit.

## Decision
- Accounts require self-declared 18+ at signup (age gate + policy text). Under-18 declaration blocks account creation (no partial accounts).
- No features are designed for children at launch; store content rating targeted accordingly (IARC; Play data-safety declares no child-directed content).
- **Deferred to Phase 4 (requires counsel review):** "Family mode" — 13–17 accounts with verified parental consent, no rewarded ads, no personalised ads, no behavioural tracking, moderated chat only. This is a *separate legal design task*, not a toggle.
- Age verification beyond self-declaration (DigiLocker-based) is a Phase 5+ option if policy demands.

## Consequences
- + Rewarded ads, analytics, and moderation scope stay lawful and simple at v1.
- + Smaller moderation surface at launch.
- − Excludes school-age players (a large chess cohort) until Family mode; mitigated by B2B academy tooling track (schools run their own tournaments on organizer accounts).
- − Self-declaration is not verification: policy risk if a minor lies; documented posture + no child-targeted features keeps good-faith compliance.

## Alternatives
- 13+ with parental consent now: rejected for v1 (compliance surface > value at this stage; revisit with counsel in Phase 4).
- No age gate: rejected — indefensible under DPDP and store policy.
