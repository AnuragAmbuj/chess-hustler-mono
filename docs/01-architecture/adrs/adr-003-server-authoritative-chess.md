# ADR-003 — Server-authoritative chess via `shakmaty`; deterministic replay

**Status:** Proposed · **Date:** 2026-11-17

## Context
Cheating, desyncs, and result disputes are the classic failure modes of client-authoritative chess. Rust has a mature, well-tested move-generation library; we must not write our own rules engine (a correctness trap), but we must own the *policy* layer (clocks, termination, ratings, persistence).

## Decision
- Use **`shakmaty`** for move generation/legality, FEN/UCI/SAN conversion, and position evaluation primitives. Wrap it in `hustle-chess` behind a narrow API so the engine is swappable.
- All outcome *policy* is ours: checkmate/stalemate/insufficient-material, threefold + 50-move auto-draws, timeout with insufficient-mating-material draw rule, rated eligibility, abort/abandonment.
- Every ply persists `(uci, san, fen_after, clocks, server_ts)`; replaying `moves` must reproduce the final FEN bit-for-bit — a CI property test asserts this on a corpus and on random games (deterministic replay guarantee, game-lifecycle §7).
- SAN is stored for humans/PGN; UCI is the machine truth; FEN after every ply enables cheap snapshots and server-side "load position" tooling (moderation).

## Consequences
- + Clients can be dumb renderers; a hacked client cannot produce an illegal move or fake a result.
- + Stored FEN per ply makes moderation and analytics (position caches, opening stats) cheap.
- − `shakmaty` API churn is a pinned-version risk (pin + upgrade tests).

## Alternatives
- Own engine: rejected (correctness risk disproportionate to benefit).
- Client-validated + server-sampled: rejected (anti-goal A1).
- `chess` crate (smaller community): `shakmaty` has better maintenance + perft performance track record.
