# ADR-002 — PostgreSQL as system of record; Redis for hot state, pub/sub, queues

**Status:** Proposed · **Date:** 2026-11-17

## Context
Moves are latency-critical (≤ 100 ms p95) but games must survive any component crash with zero lost accepted moves. Matchmaking and WS fanout need sub-ms shared state across connections. We need crash-consistency without a distributed-coordination zoo.

## Decision
**PostgreSQL 16 is the only durable store.** Every accepted game mutation is committed inline (moves row + games row in one TXN). **Redis 7** carries: per-game hot state hash, per-channel event streams (fanout + resume cursors), matchmaking queues, WS tickets, presence, rate-limit counters, config cache. Redis is *always rebuildable* from PG for game correctness (rebuild job on boot + hourly reconcile); anything whose loss is unacceptable lives only in PG.

## Consequences
- + One source of truth; recovery semantics are trivial to reason about (game-lifecycle §7).
- + Move TXN gives cross-node correctness via row locks + version column (no distributed lock service).
- − Inline PG write adds ~2–5 ms/move; acceptable at ≤ 3k games/hour. Documented escape hatch (write-behind with Redis WAL) is a measured threshold change, not an architecture change.
- − Redis loss mid-game is survivable (clients get snapshots) but degrades UX briefly; runbook exists.

## Alternatives
- **Everything in Redis, PG as archive:** rejected — violates "no toy shortcuts"; recovery story becomes event-sourced reprocessing.
- **CockroachDB/SpANNER-style:** rejected — unnecessary for single-region.
- **Only PG, no Redis:** rejected — WS fanout via PG LISTEN/NOTIFY cannot do per-channel cursors/resume cleanly and matchmaking sweeps would hammer PG.
