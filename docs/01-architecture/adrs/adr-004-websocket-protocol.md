# ADR-004 — WebSocket protocol: channel-scoped Redis Streams with resume cursors

**Status:** Proposed · **Date:** 2026-11-17

## Context
Mobile networks in India are lossy (metro tunnels, tower handoff). Users must reconnect into a live game with zero lost events and no bespoke "sync" logic per feature. We also need server-side fanout without a message bus.

## Decision
- Single WS endpoint, ticket-authenticated (30 s single-use ticket from REST; avoids long-lived JWTs in URLs).
- Channels: `u:{user_id}` (personal stream: invites, economy, notifications, presence), `g:{game_id}`, `t:{tournament_id}`.
- Each channel is a **Redis Stream** (`XADD` per event; stream ID is the public event `id`). Connection-level fanout via Redis pub/sub; per-channel history retained (games: finish+24 h; u: 24 h) enabling **resume by `last_ids`** — client reconnects, sends `resume {last_ids}`, server `XRANGE`s the gap then goes live. Gap too large → server sends `resume.stale` → client falls back to snapshot + replay.
- Envelope: `{id, ts, ch, type, data}`; every event idempotent to re-receive; `game.*` events carry `version` (game-lifecycle I-1).
- Heartbeat ping/pong 15 s; dead-socket reaping at 30 s.

## Consequences
- + One uniform delivery mechanism for every realtime feature (game, matchmaking, tournaments, notifications) — resume logic written once.
- + Server restarts preserve resume semantics (streams outlive connections).
- − Redis Streams memory cost needs per-stream trimming (MAXLEN policy, config-driven).

## Alternatives
- Event-ID counters + in-memory ring buffers: rejected (not restart-safe).
- MQTT broker: rejected (extra infra for no additional capability at this scale).
- Socket.io-style rooms: rejected (opaque semantics, JS-centric).
