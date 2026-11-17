# Contracts

Machine-readable API contracts (ADR-009). These are the source of truth for
client codegen and the CI drift gate.

| File | Covers | Enforced by |
|---|---|---|
| `openapi.yaml` | REST v1 | `server/crates/hustle-api/tests/contract_drift.rs` (both directions: route→contract and contract→route) + Redocly lint in CI |
| `asyncapi.yaml` | WS realtime | event catalog additions linted from Phase 1; envelope is normative already |

Rules:
- `/v1` is additive-only; breaking changes ⇒ `/v2`.
- Update contracts in the **same PR** as route changes — the drift test fails otherwise.
- Client types are generated from these files; never hand-write client models.
