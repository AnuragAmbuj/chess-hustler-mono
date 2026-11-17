# Repository Structure — Chess Hustle

**Status:** Proposed · Monorepo (ADR-012)

---

## 1. Tree

```
chess-hustler/
├─ docs/                        # this blueprint + future design notes/ADRs
│  ├─ README.md
│  ├─ 00-product/prd.md
│  ├─ 01-architecture/{overview.md, game-lifecycle.md, adrs/adr-*.md}
│  ├─ 02-data/schema.md
│  ├─ 03-api/{rest.md, websocket.md}
│  ├─ 04-economy/{gold-economy.md, rewarded-ads.md}
│  ├─ 05-security/{threat-model.md, compliance-india.md}
│  ├─ 06-testing/strategy.md
│  ├─ 07-infrastructure/plan.md
│  ├─ 08-roadmap/roadmap.md
│  └─ 09-repo-structure.md
├─ contracts/
│  ├─ openapi.yaml              # REST v1 (source of truth for codegen + CI drift gate)
│  ├─ asyncapi.yaml             # WS event catalog
│  └─ schemas/                  # shared JSON Schemas (envelope, game state, errors)
├─ server/
│  ├─ Cargo.toml                # workspace
│  ├─ crates/
│  │  ├─ hustle-kernel/         # config, errors, telemetry, ids, clock, flags reader
│  │  ├─ hustle-domain/         # PURE: chess rules glue, glicko2, economy math, swiss pairings
│  │  ├─ hustle-chess/          # games/moves service (owns tables; shakmaty inside)
│  │  ├─ hustle-identity/       # users, sessions, consent, otp/oauth
│  │  ├─ hustle-profiles/       # profiles, settings, blocks
│  │  ├─ hustle-matchmaking/    # Redis queue + sweeper + claim (Lua)
│  │  ├─ hustle-ratings/        # ratings + history
│  │  ├─ hustle-gold/           # ledger + balances (the only gold mutator)
│  │  ├─ hustle-rewards/        # daily/missions/achievements
│  │  ├─ hustle-ads/            # sessions, quota, admob SSV verifier (port impls)
│  │  ├─ hustle-cosmetics/      # catalog, inventory, loadout
│  │  ├─ hustle-tournaments/    # swiss lifecycle, pairings, prizes (issuance only)
│  │  ├─ hustle-friends/        # friendships, presence
│  │  ├─ hustle-realtime/       # ws sessions, streams, tickets, publisher
│  │  ├─ hustle-notifications/  # feed, push (FCM/APNs ports)
│  │  ├─ hustle-moderation/     # chat, reports, sanctions, anomalies
│  │  ├─ hustle-analytics/      # app_events sink, partitions
│  │  ├─ hustle-admin/          # config, flags, audit
│  │  ├─ hustle-api/            # axum routers, DTOs, validation, auth middleware
│  │  ├─ hustle-server/         # bin: composition root (REST+WS)
│  │  ├─ hustle-worker/         # bin: sweepers, analysis, exports, dispatch
│  │  └─ hustle-testkit/        # seeded fixtures, ws client harness, container helpers
│  ├─ migrations/               # sqlx (checksummed, expand→contract policy)
│  └─ deny.toml
├─ web/                         # React 18 + Vite + TS strict
│  ├─ src/{app, features/, board/, api/ (generated), ws/ (generated types), i18n/}
│  └─ e2e/                      # Playwright smoke
├─ android/                     # Gradle, Kotlin 2.x, Compose, Hilt
│  └─ app/src/{main/ (game/, lobby/, economy/, realtime/), test/}
├─ ios/                         # SwiftPM, SwiftUI
│  └─ ChessHustle/{Features/, BoardKit/, Realtime/, Ads/}
├─ infra/
│  ├─ k8s/{base/, overlays/{staging,prod}}/   # Kustomize
│  ├─ docker/{server.Dockerfile, compose.yml}
│  ├─ terraform/                # cloud resources (PG, Redis, object storage, DNS)
│  ├─ grafana/                  # dashboards-as-code
│  └─ runbooks/                 # DR, incident, rollback, redis-rebuild
├─ tools/
│  ├─ seed/                     # deterministic fixture generator
│  ├─ k6/                       # load scripts (ws, matchmaking, ads)
│  └─ pgn/                      # golden PGN corpus + import/export checks
├─ .github/workflows/           # ci.yml (path-filtered), release.yml, nightly.yml
└─ README.md
```

## 2. Crate rules (enforced)

1. **Ownership:** each domain crate is the only writer of its tables (schema.md ownership table). Cross-domain reads via ports; narrow DTOs, no shared mutable structs.
2. **Purity:** `hustle-domain` has zero IO deps (no sqlx, no redis) — it is the unit-test bed.
3. **Composition:** only `hustle-server`/`hustle-worker` wire implementations to ports (no domain crate knows axum).
4. **Visibility:** crate internals `pub(crate)` by default; cross-crate surface minimal; `cargo-deny` bans accidental cross-domain deps via a dependency rules file.
5. **Tests live with code**; shared harness in `hustle-testkit` (containers, scripted WS clients, fixture seeds).

## 3. Code conventions

- Rust: 2021 edition, `#![deny(warnings)]` on CI, clippy pedantic subset; errors via `thiserror` in kernel, `anyhow` only at bin edges; tracing spans on every service call; ids passed as typed newtypes (`UserId`, `GameId`).
- SQL: sqlx with compile-time checked queries (`cargo sqlx prepare` committed); migrations never edited post-merge (checksum gate).
- TypeScript: strict, no `any`; generated API/WS types only (openapi-typescript); board state reducer pure + unit-tested.
- Kotlin/Swift: generated contract models; WS layer behind an interface for testability; ads SDK isolated in its own module (`Ads/`) so it can be swapped without touching game code.

## 4. CI/CD pipelines

| Pipeline | Trigger | Steps |
|---|---|---|
| **ci.yml** | PR | path-filtered: server (fmt/clippy/test/L3/L6/coverage), web (lint/test/build), mobile (build/test on PRs touching them), contracts (drift gate), security (gitleaks, cargo audit, trivy) |
| **release.yml** | tag `v*` | full matrix, image build+push, deploy staging, manual-gate prod deploy + migration job |
| **nightly.yml** | cron | L5 load/chaos, dependency audit, restore drill (staging), fairness report aggregation |

- Environments: `staging` auto, `prod` manual approval with migration-first ordering.
- Images signed; SBOM attached; no `latest` tags in prod.

## 5. Documentation rules

- Every ADR has status/date/context/decision/consequences/alternatives (format per ADR-001).
- Design notes for Phase 3+ features (tournaments detail, bots, family mode) land in `docs/` before code.
- The schema doc and contract files must be updated in the same PR as schema/route changes (CI-enforced reminder via checklists, drift gates where automatable).
