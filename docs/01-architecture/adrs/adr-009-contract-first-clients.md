# ADR-009 — Contract-first multi-client strategy (web → Android → iOS)

**Status:** Proposed · **Date:** 2026-11-17

## Context
Three client stacks (React/TS, Kotlin/Compose, Swift/SwiftUI) against one backend. Owner-approved platform order: web first, then Android (where rewarded ads live), then iOS. Contracts must prevent drift and let each client start before backend features finish.

## Decision
- **Contracts are artifacts in-repo** (`contracts/`): OpenAPI 3.1 for REST, AsyncAPI 2.6 for WS events, JSON Schema for shared payloads. CI fails on uncommitted contract drift (openapi-diff gate).
- Clients generate types from contracts (openapi-typescript; Kotlin/TS codegen; Swift OpenAPI Generator). **No hand-written client models.**
- One WS event catalog is normative (websocket.md §4); client code maps enum names to generated types; unknown event types are ignored (forward-compat rule).
- Board UI: web uses `react-chessground` (MIT); Android/iOS implement minimal board components consuming the same event types (board engine logic stays server-side; clients only render legal-move hints via snapshot data).
- Web is a PWA (installable, offline shell for browsing history), because v1 liquidity comes fastest from shareable links.

## Consequences
- + Backend can move without waiting on clients; a single contract change ripples through codegen in CI.
- − Contract discipline is a real tax (spec-first PRs); mitigated by the drift gate.
- − Three board implementations eventually (accept: platforms differ in UI toolkit norms).

## Alternatives
- React Native / Flutter for both mobiles: rejected — owner mandate specifies native stacks; rewarded-ad SDK integration is also cleanest native.
- Hand-rolled clients: rejected — drift is the #1 source of multiplayer desync bugs in practice.
