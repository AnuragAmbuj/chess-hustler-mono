# Chess Hustle — Product & Engineering Blueprint

**Product:** Chess Hustle — a production-grade, India-first online multiplayer chess platform.
**Repo dir:** `chess-hustler` (kept as-is; product name is "Chess Hustle").
**Status of this document set:** **APPROVED (owner sign-off received).** Phase 0 (foundation) implementation is underway — see [`README.md`](../README.md) for what's built and machine-verified so far. Documents remain the source of truth; normative changes go through ADR updates.

## Decision record (owner-approved inputs)

| Decision | Value |
|---|---|
| Audience at launch | **18+ only** (self-declared age gate). "Family mode" (13+, parental consent, no rewarded ads) deferred to Phase 4. |
| Client order | **Web (React PWA) → Android (Kotlin/Compose) → iOS (Swift/SwiftUI)** |
| Team & runway | 3–5 engineers, ~3–4 months to full v1 across platforms |
| Monetization v1 | Optional rewarded ads only (2,000 Gold / completed ad, max 5/day, server-verified). No subscriptions, no purchases, no cash mechanics — ever. |
| Economy | Fictional **Gold**, granted 50,000 at signup, non-purchasable, non-transferable, zero monetary value. Named the "Gold Economy", never a wallet. |
| Hard boundary | Any staking, winner-takes-pot, paid entry, or cash-equivalent reward is **excluded** pending dedicated legal assessment (ADR-011). |

## Document map

| # | Document | What it covers |
|---|---|---|
| 00 | [`00-product/prd.md`](00-product/prd.md) | Vision, personas, goals/anti-goals, functional & non-functional requirements, KPIs, risks, glossary |
| 01 | [`01-architecture/overview.md`](01-architecture/overview.md) | System architecture, modular Rust monolith, domain boundaries, runtime data flows, deployment topology |
| 01 | [`01-architecture/game-lifecycle.md`](01-architecture/game-lifecycle.md) | Game/matchmaking/rematch state machines, clock model, disconnect/reconnect/resume, crash recovery |
| 01 | [`01-architecture/adrs/`](01-architecture/adrs/) | 14 Architecture Decision Records (ADR-001 … ADR-014) |
| 02 | [`02-data/schema.md`](02-data/schema.md) | ER diagram, full PostgreSQL schema (constraints, indexes), transaction boundaries, retention |
| 03 | [`03-api/rest.md`](03-api/rest.md) | REST API v1: conventions, endpoint catalog, request/response examples |
| 03 | [`03-api/websocket.md`](03-api/websocket.md) | Realtime protocol: envelope, channels, event catalog, resume protocol, worked examples |
| 04 | [`04-economy/gold-economy.md`](04-economy/gold-economy.md) | Gold Economy spec: sources, sinks, rates, ledger mechanics, anti-abuse, tuning |
| 04 | [`04-economy/rewarded-ads.md`](04-economy/rewarded-ads.md) | Ad integration spec: rewarded session flow, SSV verification, idempotency, quotas, experiment plan |
| 05 | [`05-security/threat-model.md`](05-security/threat-model.md) | Assets, adversaries, STRIDE analysis, anti-cheat strategy, rate limits, controls |
| 05 | [`05-security/compliance-india.md`](05-security/compliance-india.md) | DPDP Act 2023, IT Rules 2021, Online Gaming Act 2025, consumer/advertising law, app-store policy |
| 06 | [`06-testing/strategy.md`](06-testing/strategy.md) | Test pyramid, chess-correctness suites, property tests, load/chaos, acceptance gates |
| 07 | [`07-infrastructure/plan.md`](07-infrastructure/plan.md) | Environments, Kubernetes layout, sizing, cost model (INR), scaling knobs, DR, runbooks |
| 08 | [`08-roadmap/roadmap.md`](08-roadmap/roadmap.md) | Phased roadmap P0–P4 with milestones, exit criteria, staffing |
| 09 | [`09-repo-structure.md`](09-repo-structure.md) | Monorepo layout, crate boundaries, conventions, CI pipelines |

## Reading order

1. PRD → 2. Architecture overview + ADRs → 3. Schema → 4. REST + WebSocket contracts → 5. Economy & ads → 6. Security/compliance → 7. Testing → 8. Infra → 9. Roadmap → 10. Repo structure.

## What this blueprint deliberately does NOT contain

- No real-money gaming, wagering, entry fees, cash prizes, or redemption of any kind — excluded by mandate and by ADR-011 (India's Online Gaming Act, 2025 makes this an existential legal boundary, not a style choice).
- No microservices: one modular Rust monolith with enforced domain crates (ADR-001).
- No hardcoded rule values: every tunable (Gold rates, ad quotas, rating windows, time controls) lives in server-side config with an admin API and audit trail.
- No code: implementation begins after blueprint approval, in the phase order of [`08-roadmap/roadmap.md`](08-roadmap/roadmap.md).
