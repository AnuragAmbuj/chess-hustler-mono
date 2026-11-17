# Infrastructure Plan — Chess Hustle

**Status:** Proposed · **Region:** ap-south-1 (Mumbai), multi-AZ (ADR-013) · **Cost-aware scaling**

---

## 1. Environments

| Env | Purpose | Shape |
|---|---|---|
| **local** | dev | docker-compose: PG, Redis, mail/SMS sandbox, OTel all-in-one |
| **staging** | pre-prod, load tests, ad test units | prod-shape small (1 node per AZ, PG 2 vCPU, Redis 1 GB), anonymized weekly restore |
| **prod** | live | multi-AZ per §2 |

Promotion: merge → staging auto-deploy → release tag → prod (manual approval, migration job first).

## 2. Production topology (Kubernetes, Kustomize manifests)

| Component | Spec (launch) | Scaling knob |
|---|---|---|
| `hustle-server` | 3 replicas × 2 vCPU/2 GB, spread 2 AZ; HPA on CPU 60% + WS-count | replicas (WS is memory-bound: ~1.5k sockets/GB) |
| `hustle-worker` | 2 replicas × 1 vCPU/1 GB; leader-elected loops + fan-out jobs | replicas (batch) |
| PostgreSQL | CloudNativePG or managed: primary + sync replica (AZ-B), 2 vCPU/8 GB, PITR | vCPU/replicas; read replica when analytics queries compete |
| Redis | managed or in-cluster: 2 GB, AOF, AUTH+TLS | memory (streams trimmed by MAXLEN config) |
| Ingress | NGINX ingress (WS upgrade), cert-manager | L4 upgrade path if WS head-count grows |
| Edge | Cloudflare: TLS, WAF, DDoS, bot rules, caching for static/CDN assets | plans |
| Object storage | S3-compatible: PGN/event exports, backups, data-export zips | lifecycle rules |
| Observability | OTel → self-hosted Grafana stack (Prometheus/Loki/Tempo) on the cluster's "ops" node pool; Sentry SaaS | retention |

## 3. Sizing & cost model (indicative, Mumbai pricing, Nov 2026 ballpark)

| Stage | Assumption | Monthly infra cost (₹) | Notes |
|---|---|---|---|
| Dev/staging only | 1–5 users | ~8,000 | shuts down nights/weekends |
| **Launch** (≤ 5k MAU, ≤ 1k CCU) | 3 server + 2 worker nodes, small PG/Redis | **35,000–55,000** | observability self-hosted on ops node; Sentry free tier |
| Growth (25k MAU, 5k CCU) | 6–8 nodes, PG 4 vCPU, Redis 4 GB | 1.2–1.8 L | first read replica; CDN for assets |
| Scale (100k MAU, 15k CCU) | 12–16 nodes, PG 8 vCPU + replica, Redis 8 GB | 2.5–4 L | WS-gateway extraction likely (ADR-001 trigger) |

Third-party line items: OTP SMS ~₹0.15–0.25/msg (≈ ₹5–15k/mo at launch sign-up volumes), Sentry, Grafana Cloud (if outsourced), FCM/APNs free. Ad revenue is the offsetting line — at modest Indian rewarded eCPM (₹60–150), 5 ads/day × even 2% of DAU covers launch infra. **Cost per DAU dashboard** is a KPI (PRD §7); alerts if it exceeds 1.5× budget.

## 4. Scaling playbook (measured triggers, in order)

1. **Vertical first:** server replicas are stateless — HPA handles CCU up to ~10k across 3–5 nodes.
2. **PG reads:** analytics/BI queries move to read replica (they never touch the move path).
3. **WS-gateway extraction** (first service split, ADR-001): when p95 move-ack degrades from fanout load or a single node's socket ceiling binds. Protocol is already gateway-friendly (Redis streams are the bus).
4. **Deferred move persistence** (write-behind with Redis as WAL) only if inline PG writes exceed ~8 ms p95 under load — measured threshold, pre-designed (schema §7 note).
5. **Queue partitioning by rating band** if sweep CPU binds (algorithm already O(scan); expected late).
6. **Multi-region:** demand-driven (NRI traffic), Phase 5 decision with cost per CCU analysis.

## 5. Deployment & migrations

- Images: multi-stage Rust build → distroless runtime; one image for server+worker (command differs).
- Deploy: rolling, `maxUnavailable=0`; WS connections drain gracefully (clients reconnect-resume — designed for it, lifecycle §4).
- Migrations: pre-deploy Job (`hustle_migrator` role only); expand→contract for breaking changes; **never** auto-destructive (drop guards require explicit `--allow-destructive` env).
- Config: `app_config` changes are runtime (no redeploy); infra config via Kustomize overlays; secrets via Sealed Secrets → SSM sync.
- Feature flags: rollout % changes are runtime ops, not deploys.

## 6. Observability & SLOs

| Signal | Key metrics | Alert (page) |
|---|---|---|
| API | p50/p95/p99 per route, 5xx rate | 5xx > 1% for 5 min |
| WS | connections, resumes, slow-consumer disconnects, send-queue overflow | resume failure spike; overflow > 0.1%/min |
| Game | move-ack p95 (server), flag decisions/s, resyncs | move-ack p95 > 100 ms for 10 min |
| Matchmaking | queue depth, wait p50/p95 per pool, sweep lag | wait p95 > 5 min (blitz) |
| Economy | gold issued/sunk per reason, ledger-balance drift (must be 0) | **drift ≠ 0 (page)**; issuance 3× baseline |
| Ads | session→verified ratio, SSV 4xx rate, grant latency | verification ratio < 90% for 15 min |
| PG/Redis | connections, replication lag, lock waits, stream memory | lag > 5 s; memory > 80% |
| Business | DAU, games/hour, ad funnel, crash-free sessions | DAU drop > 20% day-over-day (page) |

Traces span: WS receive → move apply → PG commit → publish (single trace per move). Dashboards-as-code in repo (`infra/grafana/`). Synthetic probe: staging bot plays one game every 5 min against prod-shape staging (post-deploy smoke).

## 7. DR & runbooks (ADR-013)

| Scenario | RTO | Runbook outline |
|---|---|---|
| Single pod/node loss | 0 (k8s) | self-healing; clients auto-resume |
| AZ loss | < 5 min | sync replica promotes; server rescheduled across remaining AZ |
| PG catastrophic (region) | ≤ 30 min | restore cluster in ap-south-2 from PITR → verify ledger invariants → repoint DNS |
| Redis loss | < 2 min | rebuild job from `moves`; queues/tickets/streams ephemeral by design |
| Bad deploy | < 5 min | rollback image tag; migrations are expand-compatible by policy |
| Bad config change | < 1 min | `app_config` version rollback via admin (audited) |
| Security incident | — | IR runbook: revoke sessions (blocklist), rotate secrets, preserve evidence, notify per DPDP if PII breach |

Quarterly: restore drill (automated staging), failover drill (manual), exclusion-list compliance re-audit (ADR-011), secret rotation.

## 8. Security posture in infra

- Network policies: only `hustle-*` pods reach PG/Redis; ad-callback route isolated to its own ingress path/rate class; admin UI behind IP allowlist + OIDC.
- WAF: managed ruleset + chess-specific bot rules (endpoint enumeration, PGN scraping).
- Image provenance: signed builds, SBOM per release, trivy gate.
- Backups encrypted (KMS); object storage lifecycle: exports auto-delete after 7 days.
