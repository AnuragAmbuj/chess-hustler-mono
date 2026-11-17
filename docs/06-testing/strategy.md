# Testing Strategy — Chess Hustle

**Status:** Proposed · **CI gates live in ADR-012** · Coverage gates: domain crates ≥ 85%

---

## 1. Test pyramid

| Layer | Runner | Scope | Speed gate |
|---|---|---|---|
| L1 Unit | `cargo test` | Pure domains: chess rules, Glicko-2, gold math, pairings, clock math | < 10 s, every commit |
| L2 Property | `proptest` | Invariants over random inputs (§2) | < 60 s, every commit |
| L3 Integration | `cargo test` + testcontainers (PG+Redis) | Services against real stores; TXN semantics; idempotency | < 5 min, every PR |
| L4 E2E (server) | Rust harness, 2+ scripted WS clients | Full game lifecycles incl. reconnect/kill/failover | < 10 min, every PR |
| L5 Load/chaos | k6 + scripts | WS concurrency, matchmaking storm, pod kills, Redis loss | Nightly + pre-release |
| L6 Contract | openapi-diff + asyncapi lint | Server ↔ spec ↔ generated clients drift | Every PR |
| L7 Client | Vitest/RTL (web), JUnit+Compose tests (Android), XCUITest (iOS) | Unit + snapshot + WS-mock flows | Per client PR |
| L8 Manual | Playtest checklist | Board feel, i18n, accessibility, ads UX | Per release |

## 2. Chess-correctness suites (the load-bearing tests)

| Suite | What it proves |
|---|---|
| **Perft** | Our shakmaty wrapper matches reference node counts (startpos d1–6; Kiwipete, position 3–6 corpus) |
| **Golden PGN roundtrip** | 5,000-game master corpus: PGN → replay → identical final FEN + SAN strings; export reproduces headers |
| **Termination rules** | Hand-built positions: mate, stalemate, insufficient material (each dead-position class), threefold, 50-move, timeout-with-insufficient-mating-material draw |
| **Clock semantics** | Property: remaining never negative before a flag decision; increment applied on completed move; `deadline` math matches sweep decision under random move timing; lag-credit caps honored |
| **Replay determinism** | Random legal games: persist ply-by-ply → rebuild from `moves` → byte-identical FEN chain (game-lifecycle §7) |
| **Concurrency** | Two simulated clients racing moves: exactly one wins each ply; no double-ply, no lost move; DB constraint violations impossible |

## 3. Economy & trust invariants (property + integration)

| Invariant | Test |
|---|---|
| `Σ ledger.delta == balance` for every account after random op streams | property test on the ledger helper |
| Overdraft impossible | concurrent purchase storm → some fail with `gold.insufficient`, no negative balance ever |
| Idempotency | every grant/spend replayed 3× → single ledger row, identical responses, `Idempotent-Replay` flag |
| Ad grant trust | forged/replayed/out-of-window SSV callbacks → rejected; valid → exactly one grant; quota race (2 callbacks for last slot) → exactly one wins |
| Quota/cooldown | 7 rapid ad sessions → 5 granted / 2 rejected with stable codes |
| Mission/achievement progress | server-event-driven only; client-reported counters structurally impossible (no such API) |
| Rating math | Glicko-2 vs reference vectors (golden values); provisional weightings; pool separation |
| Matchmaking | randomized arrival distribution → no double-pairing (Lua claim atomicity), window widening monotonic, blocked pairs never matched |

## 4. Integration & E2E scenarios (harness: scripted WS/REST clients)

1. Happy rated blitz game → result → ratings + gold + mission progress all correct in one look.
2. Resign → immediate finalization; rematch handshake → colors swapped.
3. Disconnect/resume: kill client WS mid-game (both sides), reconnect with `last_ids` → zero lost events, clock truth preserved.
4. Server pod kill mid-game → restart → `game.resync`/snapshot rebuild → game completes with correct result.
5. Redis flush mid-game → rebuild job reconstructs state from `moves` → clocks consistent (documented tolerance).
6. Flag resolution: flag at deadline → win; flag with insufficient mating material → draw.
7. Tournament: 8 players, 3 rounds, byes, forfeit, standings + prize issuance (ADR-011-compliant issuance only).
8. Signup → consent → 50,000 welcome → daily claim × 7 → streak reset → ad session → SSV grant → purchase → ledger coherence throughout.
9. Moderation: report → queue → sanction → WS blocklist instant-effect.
10. Account deletion → grace cancel → final pseudonymization; game history still renders with "Deleted Player".

## 5. Load & chaos (nightly, staging with prod-shape data)

| Scenario | Target |
|---|---|
| 10k concurrent WS (k6), mixed idle/moving | move-ack p95 ≤ 100 ms server-side; zero dropped events |
| Matchmaking storm: 5k joins in 60 s (blitz) | p50 match < 30 s; no double-pairing; no orphan queue rows |
| Ads callback burst (2k/min) | SSV handler p95 ≤ 50 ms; zero double-grants |
| Pod kill during 500 active games | ≤ 1 move visibility blip; all games resolvable; no terminal-state corruption |
| PG failover drill (manual monthly) | recovery ≤ 30 s; games continue via Redis→PG flush |
| Redis restart | rebuild job ≤ 30 s for 1k active games; clients resume |

## 6. Security tests (CI + scheduled)

- Authz matrix: every endpoint × {other-user, moderator, admin, anonymous} → expected 401/403/200 (generated from the route table, not hand-listed).
- Rate-limit conformance per class (threat-model §4).
- OTP brute force simulation → lockouts fire; no SMS after caps.
- Idempotency replay attacks, WS ticket reuse, channel-subscription forgery attempts.
- Secret scanning (gitleaks), dependency audit gates, container scan gates.

## 7. Client tests

- **Web:** board interaction unit tests (legal-hint rendering vs snapshot), WS reducer (event application idempotency by `id`), offline-reconnect flows with mocked streams, i18n snapshot tests (en/hi).
- **Android:** Compose UI tests for lobby/game/rewards; AdMob flow with test ad units + mocked SSV callback; rotation/process-death state restore.
- **iOS (P3):** same shape via SwiftUI testing + XCUITest smoke.
- **Shared:** a cross-client conformance pack — the same scripted event sequences (from `contracts/asyncapi.yaml` fixtures) must render identical board states on all three clients.

## 8. Release gates per phase (roadmap tie-in)

| Phase | Must pass before exit |
|---|---|
| P1 (web MVP) | L1–L4 green; 100-game soak (2 bots, random openings) with zero desync; reconnect suite green |
| P2 (social/economy) | Economy invariants suite green; moderation flows e2e; load 2k CCU |
| P3 (mobile + ads + tournaments) | Ad trust suite green (forgery/replay/quota); tournament e2e; 10k CCU load; store pre-review checklist |
| P4+ | Chaos suite green; cost-per-DAU dashboard live; fairness report pipeline |

## 9. Test data & environments

- `tools/seed`: deterministic fixture generator (users, games incl. edge positions, economy events) — same generator feeds local, staging, and property tests.
- Staging: prod-shaped Postgres volume restore (anonymized) weekly; ad unit IDs = test units; SMS provider = sandbox; Sentry = separate project.
- No production PII in any test environment; fixtures are synthetic handles/phones (+9190000000xx range reserved).
