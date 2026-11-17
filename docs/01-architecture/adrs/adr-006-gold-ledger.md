# ADR-006 — Gold economy as an append-only, idempotent ledger

**Status:** Proposed · **Date:** 2026-11-17

## Context
Gold must be *economy-grade* even though it's fictional: no double-grants on retry, no negative balances, no "free money" paths, and full auditability for support and fraud analysis. The mandate also requires Gold to be structurally incapable of becoming a wallet.

## Decision
- **Append-only `gold_ledger`** is the truth of every change: `(user_id, delta, balance_after, reason, ref_type, ref_id, idem_key UNIQUE NULL, meta)`. `gold_balances` is a locked cache updated in the same TXN with `CHECK (balance >= 0)`.
- Every grant/spend takes an **idempotency key** derived from its causal fact (`ads:{provider}:{txn}`, `daily:{user}:{ist_date}`, `game:{id}:gold`, `purchase:{user}:{cosmetic}:{attempt}`). Unique index → retries are no-ops; concurrent attempts serialize on the balance row lock.
- **No code path exists** for purchase/transfer/gift/redemption of Gold. Admin grants are audited, reason-coded, capped, and visible to the user in their ledger ("support grant").
- Reason codes are a closed enum + `meta` JSONB for details; analytics computes source/sink ratios from the ledger alone.
- Currency math lives in `hustle-domain::economy` (pure, unit-tested); DB layer applies.

## Consequences
- + Fraud forensics: every rupee-equivalent question ("how did this account get 200k Gold?") is one indexed query.
- + Retries (client flaps, callback re-delivery) are structurally safe.
- − Ledger grows forever → partition by month (schema §8); balance column stays hot.

## Alternatives
- Balance-only (no ledger): rejected — unauditable, unreplayable, fraud-prone.
- Event sourcing with projections: overkill; the ledger IS the projection with a materialized balance.
- User-to-user gifting with limits: rejected — creates a transfer economy, invites RMT farms; the mandate forbids transfers outright.
