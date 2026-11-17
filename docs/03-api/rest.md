# REST API v1 — Chess Hustle

**Status:** Proposed · **Base URL:** `https://api.<domain>/v1` · **OpenAPI 3.1:** `contracts/openapi.yaml` (generated from this spec at Phase 0)

---

## 1. Conventions

| Concern | Rule |
|---|---|
| Auth | `Authorization: Bearer <access_jwt>` (15 min TTL). Public endpoints (health, ad SSV callback, auth) excepted. |
| WS auth | `GET /v1/realtime/ticket` → 30 s single-use ticket (ADR-008) |
| Content | `application/json; charset=utf-8` (PGN endpoint: `application/x-chess-pgn`) |
| Time | ISO-8601 UTC. IST-day fields are plain `date` (server computes in `Asia/Kolkata`) |
| IDs | Public ULIDs; internal bigints never serialized |
| Errors | Always `{ "error": { "code", "message", "details"? } }`; codes are stable strings (§4) |
| Pagination | Cursor-based: `?limit=25&cursor=<opaque>` → `{"items":[…], "next_cursor": "…"}` (null = end). Max limit 100 |
| Idempotency | `Idempotency-Key` header **required** on safety-critical POSTs (queue join, purchase, challenge, rematch, ad session). Replays return the original result with `Idempotent-Replay: true` |
| Tracing | Honor inbound `traceparent`; every response carries `x-request-id` |
| Versioning | `/v1` additive-only; deprecations announced via `Sunset` header ≥ 90 days |
| Rate limits | Headers `x-ratelimit-limit/remaining/reset`; 429 with `Retry-After`. Class table in threat-model §7 |

## 2. Endpoint catalog

Auth is required unless marked **[public]**. Roles: `user` (default), `moderator`, `admin` (admin endpoints §2.11).

### 2.1 Auth & sessions
| Method | Path | Notes |
|---|---|---|
| POST | `/auth/otp/send` | [public] `{phone}` → 204. Rate-limited hard (5/hr/IP, 3/15min/phone) |
| POST | `/auth/otp/verify` | [public] `{phone, code, consents{…}, age_confirmed:true, profile{handle,display_name}}` → tokens + user |
| POST | `/auth/google` | [public] `{id_token, consents…, age_confirmed, profile…}` |
| POST | `/auth/refresh` | [public] `{refresh_token}` → rotated pair (reuse ⇒ family revoked) |
| POST | `/auth/logout` | revoke current session |
| POST | `/auth/logout-all` | revoke all sessions |
| GET | `/realtime/ticket` | → `{ticket, ws_url, expires_in:30}` |

### 2.2 Me & profile
| Method | Path | Notes |
|---|---|---|
| GET | `/me` | user + profile + balances + per-pool ratings |
| PATCH | `/me` | display_name, bio, settings (handle change: 1 per 90 days) |
| DELETE | `/me` | DPDP deletion request → 30-day grace |
| POST | `/me/deletion-cancel` | within grace window |
| GET | `/me/export` | → 202 `{job_id}`; async zip (DPDP right of access) |
| GET | `/me/gold` | balance + `?cursor` ledger page |
| GET | `/me/players/{handle}` | public profile (also `/players/{handle}`) |
| GET | `/players/{handle}/games` | rated games feed |

### 2.3 Matchmaking & games
| Method | Path | Notes |
|---|---|---|
| POST | `/matchmaking/queue` | `{tc:{initial_s,increment_s}, rated}` → 202 `{pool, est_wait_s}`; idempotent per user |
| DELETE | `/matchmaking/queue` | leave (also safe if not queued) |
| GET | `/matchmaking/status` | queued? pool? waited_s |
| GET | `/games/{id}` | full snapshot (fen, clocks from server truth, players, status, result, stream_id) |
| GET | `/games/{id}/moves` | ply list (cursor by ply) |
| GET | `/games/{id}/pgn` | [public-if-shared] PGN export with full headers |
| POST | `/games/{id}/resign` | instant; idempotent |
| POST | `/games/{id}/draw-offer` | one outstanding |
| POST | `/games/{id}/draw-accept` | |
| POST | `/games/{id}/draw-decline` | |
| POST | `/games/{id}/abort` | valid ≤ 4 plies |
| POST | `/games/{id}/claim-timeout` | opponent-flag claim fallback (server verifies against clock truth) |
| POST | `/games/{id}/rematch` | handshake (WS events) |
| POST | `/challenge` | `{handle, tc, rated}` friend challenge → invite via WS |
| POST | `/challenge/{id}/accept` · `/decline` | |

> **Moves are WS-only** (`websocket.md` §5) — they are the hot path and carry realtime semantics; REST moves are deliberately not supported to prevent a two-truths state.

### 2.4 Gold & rewards
| Method | Path | Notes |
|---|---|---|
| GET | `/me/gold/summary` | balance + 30-day source/sink breakdown (transparency view) |
| GET | `/me/daily` | `{available, streak, next_reset_ist}` |
| POST | `/me/daily/claim` | idempotent per IST day |
| GET | `/me/missions` | active daily/weekly + progress |
| POST | `/me/missions/{id}/claim` | idempotent |
| GET | `/me/achievements` | |

### 2.5 Ads
| Method | Path | Notes |
|---|---|---|
| POST | `/ads/session` | → `{session_id, provider, reward_gold, remaining_today, cooldown_s}`; rejected if in active game / quota exhausted / ineligible |
| GET | `/ads/status` | `{enabled, gold_per_ad, max_per_day, used_today, cooldown_until, eligibility{…}}` |
| GET | `/ads/ssv/{provider}/callback` | **[public, provider-signed]** AdMob SSV verified callback (ads doc §3) |

### 2.6 Cosmetics
| Method | Path | Notes |
|---|---|---|
| GET | `/cosmetics` | catalog (active, available window) |
| GET | `/cosmetics/{id}` | detail + owned? |
| POST | `/cosmetics/{id}/purchase` | idempotent; tx: ledger+inventory |
| GET | `/me/loadout` | equipped set |
| PUT | `/me/loadout` | equip owned items |

### 2.7 Friends & blocks
| Method | Path | Notes |
|---|---|---|
| GET | `/friends` | accepted list + presence |
| GET | `/friends/requests` | incoming/outgoing |
| POST | `/friends/requests` | `{handle}` |
| POST | `/friends/requests/{id}/accept` · `/decline` | |
| DELETE | `/friends/{id}` | |
| GET | `/blocks` | |
| POST | `/blocks` | `{handle}` (blocks pairing + chat) |
| DELETE | `/blocks/{handle}` | |

### 2.8 Tournaments
| Method | Path | Notes |
|---|---|---|
| GET | `/tournaments` | `?status=scheduled\|registering\|running\|finished` |
| GET | `/tournaments/{id}` | detail, standings, rounds, my games |
| POST | `/tournaments/{id}/join` | free entry (ADR-011) |
| POST | `/tournaments/{id}/withdraw` | until registration closes |

### 2.9 Moderation & notifications
| Method | Path | Notes |
|---|---|---|
| POST | `/reports` | `{category, target_user_id, game_id?, note?}` |
| GET | `/notifications` | feed |
| POST | `/notifications/read` | `{ids:[…]}` or `{all:true}` |
| GET | `/grievance` | [public] IT-Rules grievance officer contact + policy links |

### 2.10 Meta
| Method | Path | Notes |
|---|---|---|
| GET | `/meta/config` | [public] client boot config: TC presets, economy public values (ads/rewards display rates), feature flags, `server_time` |
| GET | `/healthz`, `/readyz` | [public, internal port] |

### 2.11 Admin (role-guarded; audit-logged)
| Method | Path | Notes |
|---|---|---|
| GET | `/admin/players/{id}` | full view incl. sanctions/flags |
| POST | `/admin/players/{id}/sanctions` | `{kind, reason, expires_at?}` (T9) |
| POST | `/admin/gold/grants` | `{user_id, amount, reason}` — support grants only; hard cap/day; audited |
| GET | `/admin/reports?status` / POST `/admin/reports/{id}/action` | moderation queue |
| GET | `/admin/flags?status` / POST `/admin/flags/{id}/resolve` | anomaly review (ADR-014) |
| GET/PUT | `/admin/config/{key}` | economy/ads knobs (optimistic-lock on version) |
| GET/PUT | `/admin/flags-config/{key}` | feature flags + rollout |
| POST | `/admin/tournaments` | create tournament |
| GET | `/admin/audit?cursor` | audit log |

## 3. Worked examples

### 3.1 Signup (OTP verify)
```http
POST /v1/auth/otp/verify
{ "phone": "+919876543210", "code": "482913",
  "age_confirmed": true,
  "consents": { "essential": true, "functional": true, "marketing": false, "personalised_ads": false },
  "profile": { "handle": "ananya_plays", "display_name": "Ananya" } }
```
```json
{ "access_token": "eyJ…", "expires_in": 900,
  "refresh_token": "rt_9f3…", "user": {
    "id": "01JDMK…", "handle": "ananya_plays", "display_name": "Ananya",
    "gold": 50000,
    "ratings": { "bullet": 1200, "blitz": 1200, "rapid": 1200, "classical": 1200 },
    "provisional": true } }
```
Welcome grant of 50,000 Gold is included atomically (T1) and appears as the first ledger entry `reason=welcome`.

### 3.2 Join matchmaking
```http
POST /v1/matchmaking/queue
Idempotency-Key: 7c9e6679-7425-40de-944b-e07fc1f90ae7
{ "tc": {"initial_s": 180, "increment_s": 2}, "rated": true }
```
```json
{ "pool": "blitz", "est_wait_s": 12,
  "ratings": { "blitz": 1421, "rd": 87 } }
```
Match arrives via WS `matchmaking.matched` (websocket.md §5.2) — the 202 response never blocks.

### 3.3 Game snapshot (mid-game, post-reconnect)
```json
{ "id": "01JDQ7…", "rated": true, "pool": "blitz",
  "tc": {"initial_s": 180, "increment_s": 2},
  "white": {"handle": "ananya_plays", "rating": 1424},
  "black": {"handle": "vikram64", "rating": 1439},
  "status": "active",
  "fen": "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
  "ply": 1, "version": 1,
  "clocks": { "white_ms": 181934, "black_ms": 180000, "server_ts": "2026-11-17T09:12:44.812Z" },
  "moves": [{"ply":1,"uci":"e2e4","san":"e4"}],
  "stream_id": "1731837164-0",
  "draw_offer": null }
```
`clocks.server_ts` is the anchor: remaining time for the side to move is `clocks[c] − (now − server_ts)`; the opponent's value is frozen until they move. Clients render from this; only the server's own clock decisions can end the game.

### 3.4 Resign
```http
POST /v1/games/01JDQ7…/resign
Idempotency-Key: 42…
```
```json
{ "status": "finished", "result": "black", "termination": "resignation",
  "ratings": { "white": {"before": 1424, "after": 1408, "delta": -16},
               "black": {"before": 1439, "after": 1455, "delta": 16} },
  "gold": { "earned": 5, "reason": "gameplay", "balance": 67310 } }
```

### 3.5 Daily reward claim
```http
POST /v1/me/daily/claim
Idempotency-Key: …
```
```json
{ "streak": 4, "gold_granted": 1500, "balance": 92310,
  "ledger": {"id": "gl_01JDS…", "reason": "daily_streak", "delta": 1500 } }
```

### 3.6 Ad session request (ineligible case shown)
```http
POST /v1/ads/session
```
```json
{ "error": { "code": "ads.quota_exceeded",
  "message": "Daily rewarded-ad limit reached",
  "details": { "used_today": 5, "max_per_day": 5, "resets_at": "2026-11-18T00:00:00+05:30" } } }
```

### 3.7 Purchase cosmetic
```http
POST /v1/cosmetics/board_chaturanga/purchase
Idempotency-Key: …
```
```json
{ "owned": true, "balance": 24210,
  "ledger": {"reason": "cosmetic_purchase", "delta": -30000} }
```
Replay with the same key → same response, `Idempotent-Replay: true`, single ledger row.

### 3.8 Error shape (auth)
```json
{ "error": { "code": "auth.session_revoked",
  "message": "Session revoked; sign in again",
  "details": { "reason": "rotation_reuse" } } }
```

## 4. Error code registry (stable, additive)

| Code | HTTP | Meaning |
|---|---|---|
| `auth.otp_invalid` / `auth.otp_rate_limited` | 401/429 | OTP verification |
| `auth.age_gate` | 403 | 18+ declaration missing/refused (ADR-010) |
| `auth.token_expired` / `auth.session_revoked` | 401 | |
| `auth.ticket_used` / `auth.ticket_expired` | 401 (WS) | |
| `validation.failed` | 400 | field errors in `details.fields` |
| `not_found` / `forbidden` | 404/403 | |
| `conflict` | 409 | generic state conflict (e.g., second draw offer) |
| `mm.already_matched` | 409 | queue join after match already made |
| `game.wrong_turn` / `game.illegal_move` / `game.finished` / `game.not_participant` | 400/403/409 | move/action rules |
| `gold.insufficient` | 409 | purchase above balance |
| `ads.quota_exceeded` / `ads.cooldown` / `ads.ineligible` / `ads.in_active_game` | 409/403 | AD-3 |
| `tn.full` / `tn.closed` | 409 | tournament capacity/window |
| `md.blocked` | 403 | block prevents action |
| `rate_limited` | 429 | |
| `internal` | 500 | never leaks internals |

## 5. Contract governance

- `contracts/openapi.yaml` is the artifact of record; CI gate: any server route change without a contract diff fails the build (ADR-009).
- WS events live in `contracts/asyncapi.yaml`; the event registry in `websocket.md` §4 is normative prose.
- Breaking changes ⇒ `/v2` namespace; `/v1` stays additive (add fields, add endpoints, never re-shape).
