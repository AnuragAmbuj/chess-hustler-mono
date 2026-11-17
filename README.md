# Chess Hustle

India-first, server-authoritative online chess. **Status: Phase 0 (foundation) — blueprint approved, implementation started.**

Product & engineering blueprint: **[docs/README.md](docs/README.md)** (PRD, architecture, ADRs, schema, API/WS contracts, economy, threat model, compliance, testing, infra, roadmap).

## Monorepo layout

| Path | What |
|---|---|
| `docs/` | Approved blueprint (read `docs/README.md` first) |
| `contracts/` | OpenAPI + AsyncAPI (source of truth; CI drift gate) |
| `server/` | Rust workspace: `hustle-server` (REST+WS), domain crates per ADR-001 |
| `web/` | React + TypeScript PWA shell |
| `infra/` | Docker, Kustomize manifests, runbooks |
| `.github/workflows/` | CI (fmt/clippy/tests/migrations/contracts/web/docker) |

## Quickstart (local)

```bash
# Option 1 — full stack in Docker (Postgres + migrations + Redis + server + web)
docker compose -f infra/docker/compose.yml up --build
# → server http://localhost:8080 · web http://localhost:5173

# Option 2 — server only, against running Postgres
psql "postgresql://postgres:hustle@localhost:5432/hustle" -f server/migrations/0001_init.sql
cd server && cargo run -p hustle-server
curl localhost:8080/healthz && curl localhost:8080/v1/meta/config

# Option 3 — web dev server (proxies /v1 to :8080)
cd web && npm install && npm run dev
```

## Phase 0 scope (this commit)

- Rust workspace: `hustle-kernel` (config/telemetry), `hustle-domain` (pure chess vocab + **Glicko-2 golden-tested against the paper's reference example** + economy math), `hustle-api` (health/ready/meta routes), `hustle-server` binary (graceful shutdown).
- **Contract drift gate in both directions** — a route not in the contract, or a contract path not served, fails `cargo test`.
- Schema v1 migration (`server/migrations/0001_init.sql`) — full approved data model with seeds.
- CI: rustfmt/clippy/tests, Postgres migration job, Redocly contract lint, web build, Docker image build.
- Local env: `infra/docker/compose.yml` (PG, auto-migrate, Redis, server, web).

## What's next (Phase 1)

Identity + consent, matchmaking, game orchestration on `shakmaty` with server-authoritative clocks, WebSocket channels with resume, ratings, game history/PGN, daily-streak + Gold ledger. See `docs/08-roadmap/roadmap.md`.
