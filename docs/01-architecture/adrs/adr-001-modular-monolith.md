# ADR-001 — Modular Rust monolith (no microservices at v1)

**Status:** Proposed · **Date:** 2026-11-17

## Context
A 3–5 person team must ship chess multiplayer, economy, social, and tournaments in ~3–4 months. Distributed systems tax small teams with deployment complexity, distributed transactions, and observability overhead that the product stage doesn't justify. The mandate explicitly forbids unnecessary microservices.

## Decision
One deployable Rust binary (`hustle-server`) plus a worker binary (`hustle-worker`) from a single Cargo workspace. Domain boundaries are enforced at the **crate level**: each domain owns its tables exclusively, cross-domain access only via in-process traits (ports/DTOs), no shared mutable row types. HTTP and WebSocket live in the same process; the WS-gateway extraction is the first candidate if p95 move latency or WS counts demand it.

## Consequences
- + Single transaction boundary (Postgres) for cross-domain operations (game finalize touches chess, ratings, gold, missions in one TXN).
- + Simple local dev (docker-compose), one deploy unit, straightforward tracing.
- − Requires discipline (boundary rules in overview §3); mitigated by crate visibility + review checklist + a schema-ownership lint in CI (cross-crate SQL string scan).
- − Horizontal scaling of one concern (e.g., ads analysis) means scaling the whole process; worker binary absorbs batch load.

## Alternatives
- **Microservices from day 1:** rejected — distributed TXN for game finalize would need sagas/outbox for correctness we can get with one DB transaction.
- **Single-crate god binary:** rejected — boundaries are the mechanism that keeps the codebase AI/human-navigable and extraction-ready.
- **Trigger to split:** sustained p95 move-ack > 100 ms attributable to contention, WS > 10k concurrent per node, or a domain needing an independent release cadence.
