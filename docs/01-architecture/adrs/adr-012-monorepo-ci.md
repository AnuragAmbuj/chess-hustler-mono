# ADR-012 — Monorepo with Cargo workspace + contract dir; GitHub Actions CI

**Status:** Proposed · **Date:** 2026-11-17

## Context
One backend team + web + mobile clients shipping against evolving contracts. Split repos would create contract-sync hell and version skew across 4 codebases.

## Decision
- **Monorepo** (`09-repo-structure.md`): `server/` (Cargo workspace, domain crates), `web/`, `android/`, `ios/`, `contracts/` (OpenAPI/AsyncAPI/JSON Schema), `infra/`, `docs/`.
- **CI (GitHub Actions):** on PR — rustfmt/clippy/-D warnings, unit + integration tests (testcontainers), contract drift gate, schema-migration checksum check, mobile/web builds on change paths, `cargo audit` + image scan (trivy), coverage gate (domain crates ≥ 85%). Path-filtered pipelines keep mobile builds off backend PRs.
- **Versioning:** server is trunk-based; clients tag releases; contract files carry `v1` namespace (additive-only within v1).
- Migration discipline: sqlx migrations run as pre-deploy job; **expand→contract** two-phase for any breaking schema change; migration checksum gate prevents post-hoc edits.

## Consequences
- + Atomic cross-stack changes (server + contract + web in one PR).
- + One place for docs and ADRs next to code.
- − Larger CI matrix; mitigated by path filters and caching (sccache, Gradle/Kaniko caches).

## Alternatives
- Polyrepo: rejected — contract drift between 4 repos is the dominant failure mode for small teams.
- Bazel: rejected — toolchain cost unjustified at this scale; cargo/pnpm/gradle caches suffice.
