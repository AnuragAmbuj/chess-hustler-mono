# ADR-013 — Single region ap-south-1 (Mumbai), multi-AZ; DR posture

**Status:** Proposed · **Date:** 2026-11-17

## Context
India-first product: latency promise (≤ 25 ms from metros) and DPDP data-residency posture both favor India-region hosting. Launch scale (≤ 10k CCU) does not justify active-active multi-region.

## Decision
- **ap-south-1 (Mumbai) as the only region** at launch (Cloudflare edge in front for TLS/WAF/DDoS; Indian PoPs absorb edge traffic).
- **Multi-AZ** for everything stateful: PG sync-replica across AZs (CloudNativePG or managed), Redis with replica, 2+ server replicas per AZ.
- **DR:** nightly full + continuous WAL PITR (RPO ≤ 5 min) to object storage in a *second* region (e.g., ap-south-2 Hyderabad) for restore-only; RTO ≤ 30 min via documented runbook (provision restore cluster, PITR, repoint DNS). Redis is not backed up (rebuildable by design).
- Quarterly restore drill (automated in staging) — a backup that hasn't been restored is not a backup.
- Multi-region active-active is a Phase 5+ decision gated on measurable cross-region demand (e.g., NRI users) and cost per CCU.

## Consequences
- + Simplest consistent model: one PG primary, no cross-region conflict resolution ever.
- + Meets residency expectations by default (data at rest in India; cross-border transfers limited to support tooling/SaaS, documented in compliance doc).
- − Single-region outage = full outage (≤ 43 min/mo SLO budget); mitigated by AZ redundancy + restore runbook.

## Alternatives
- ap-south-1 + Singapore standby: rejected for v1 (2× stateful cost, cross-region latency for writes pointless at this scale).
- Self-hosted bare metal (cheaper ₹/vCPU): deferred — ops overhead not justified pre-PMF; revisit at 50k+ MAU (infra doc §7).
