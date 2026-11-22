# Launch & Operational Runbook — Chess Hustle

**Status:** Final · **Scope:** Launch operations, incident mitigation, moderation staffing, and SLA runbooks (Phase 4 Deliverable).

---

## 1. Pre-Flight Checklist

Before enabling public traffic to production in `ap-south-1`:

1. **Schema & Migration Gate:** Ensure all migrations up to `0003_session_rotation.sql` have run cleanly via the migration job.
2. **Contract-Drift Gate:** Integration test `cargo test -p hustle-api --test contract_drift` is green.
3. **SSV Ad Key Cache:** Google AdMob public key fetcher initialized with fallback keys cached locally.
4. **Virtual Economy Audit:** Double-entry ledger balance check: `SELECT SUM(amount) FROM gold_ledger` matches `SELECT SUM(gold) FROM profiles`.
5. **Anti-Cheat Analyzer:** Anomaly detection background job running on a 15-minute cron.
6. **Web Client Build:** Production assets compiled with zero asset errors (`dist/assets/`).

---

## 2. Incident Runbooks

### 2.1 Pod Disconnection Storm or Node Loss
- **Symptoms:** Sharp rise in WebSocket reconnect requests (`/v1/realtime/ticket`), temporary drop in active sockets.
- **Automated Behavior:** Clients reconnect automatically with exponential backoff and request replay via `last_ids`.
- **Operator Steps:**
  1. Inspect HPA metrics: `kubectl get hpa -n hustle-prod`.
  2. If memory utilization > 80% on socket pods, scale replicas:
     ```bash
     kubectl scale deployment/hustle-server --replicas=6 -n hustle-prod
     ```
  3. Verify NGINX ingress connection rate limits are not throttling legitimate reconnects.

### 2.2 Redis Instance Loss or Rebuild
- **Symptoms:** WebSocket pub/sub degradation, matchmaking queue stalls.
- **Recovery Procedure (RTO < 2 min):**
  1. Ephemeral stream state will reset; PostgreSQL remains the source of truth for all active game moves.
  2. Redis automatically restarts or failovers to replica.
  3. Execute state recovery worker to reconstruct match state from `moves` table:
     ```bash
     cargo run --bin hustle-worker -- --recover-active-games
     ```
  4. Active clients reconnect seamlessly using snapshot resync.

### 2.3 PostgreSQL Primary Failover (ADR-013)
- **Symptoms:** Write requests return 500/503; read replicas healthy.
- **Recovery Procedure (RTO < 5 min):**
  1. Standby replica in Availability Zone B is promoted by CloudNativePG operator.
  2. Connection poolers update DNS pointer to the new primary.
  3. Verify ledger idempotency:
     ```sql
     SELECT COUNT(*) FROM gold_ledger WHERE created_at > NOW() - INTERVAL '5 minutes';
     ```

---

## 3. Trust, Safety & Legal Compliance

### 3.1 DPDP Act Compliance (Digital Personal Data Protection)
- **Consent Logs:** User consents stored in `consent_records` with timestamp and IP hash.
- **Data Export Request:** Must be processed within 7 business days.
- **Account Deletion Request:**
  - 30-day grace period during which user may cancel via `/v1/me/deletion-cancel`.
  - After 30 days: PII (phone, email, name, avatar) pseudonymized; game moves preserved with `handle = "Deleted Player"` to protect opponent game history integrity.

### 3.2 IT Rules, 2021 Grievance Redressal
- **Grievance Officer:** Mandatory appointment visible at `/v1/grievance` and footer.
- **Acknowledgement:** Within 24 hours of ticket receipt.
- **Disposal/Resolution:** Within 15 calendar days with written reasoning.

### 3.3 Moderation Staffing Tiers
- **Tier 1 (Automated):** Chat profanity dictionary masking, auto-mute on 3 repeated violations within a match.
- **Tier 2 (Queue Review):** Human review of flagged reports in `/v1/admin/reports` (toxic chat, intentional stalling, inappropriate handle).
- **Tier 3 (Fair Play Panel):** Review of engine-correlation anomalies flagged by `hustle-moderation/src/anticheat.rs`. Sanction escalation: Warning -> 7-Day Rating Pool Ban -> Account Termination.

---

## 4. SLO Dashboards & Alert Thresholds

| Metric | Target | Warning Threshold | Page Incident Alert |
|---|---|---|---|
| Move Acknowledgment | p95 < 100ms | p95 > 150ms for 5m | p95 > 250ms for 3m |
| Matchmaking Wait | p50 < 30s | p50 > 60s | p50 > 120s |
| API 5xx Error Rate | < 0.1% | > 0.5% for 5m | > 1.0% for 3m |
| SSV Ad Verification | > 95% | < 90% for 15m | < 80% for 15m |
| Ledger Balance Drift | 0 | Drift != 0 (Immediate Page) | Drift != 0 |
