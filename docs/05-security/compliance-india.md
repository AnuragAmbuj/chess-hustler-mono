# India Compliance — Chess Hustle

**Status:** Proposed · **Scope:** privacy, intermediary, gaming, consumer/advertising, app-store policy, tax posture · **Standing caveat:** summaries below are engineering-facing; **external counsel sign-off is a gate before public launch and before each Phase 3+ feature** (esp. Family mode, tournaments at scale).

---

## 1. Product legal posture (the one-paragraph version)

Chess Hustle is a **free-to-play social game with a non-monetary virtual currency (Gold)**. No stakes, no entry fees, no cash or cash-equivalent prizes, no purchases, no transfers, no redemption. This places it **outside** India's Online Gaming Act, 2025 definition of prohibited "online money games" and outside gambling/contest law exposure — *provided* the exclusion list (ADR-011) is structurally enforced (it is: no transfer reason codes, no pot mechanics, no IAP). Every compliance artifact below assumes and reinforces that posture.

## 2. Online Gaming Act, 2025 (Promotion and Regulation of Online Gaming)

| Requirement | Our position |
|---|---|
| Prohibition on offering/operating online money games | N/A — no stakes/fees/cash prizes by design (ADR-011) |
| Prohibition on advertising money games / financial facilitation | N/A — we advertise a social game; our rewarded-ads economy pays Gold (non-monetary), never cash |
| Recognition of e-sports | Opportunity — chess esports events are a Phase 5 sponsorship/B2B track |
| Any NCOG/self-regulatory registration for social games | Monitor guidance; register/notify if a mechanism applies to social games |
| Quarterly re-certification | Exclusion list re-audit each quarter; any proposal touching §10 of the PRD halts for counsel |

## 3. DPDP Act 2023 (+ DPDP Rules, phased 2025–26) — privacy engineering

| Obligation | Implementation |
|---|---|
| Lawful purpose + notice | Itemized consent notice at signup (essential/functional, marketing, personalised ads, chat) — versioned, granular, auditable (`consent_records`) |
| Consent withdrawal | As easy as grant: settings toggles write new consent records; marketing stops immediately |
| **Children (< 18)** | ADR-010: 18+ self-declared gate at launch; no child-directed features; Family mode deferred to a counsel-reviewed Phase 4 design (no tracking/targeted ads for minors would be hard requirements) |
| Data minimization | Phone OR email only; hashed IPs; no contacts, no device fingerprints, no third-party trackers |
| Purpose limitation | Analytics is first-party, product-improvement scoped; ad personalization only with explicit opt-in |
| Right to access | `GET /me/export` async job (target ≤ 7 days) |
| Right to correction | `PATCH /me` |
| Right to erasure | `DELETE /me` → 30-day grace → pseudonymization preserving game-history integrity (users never hard-deleted; PII scrubbed: sessions, tokens, device entries, phone/email nulled) |
| Breach notification | Runbook: notify Data Protection Board + affected users per DPDP timelines; contact registry maintained |
| Cross-border transfer | Primary storage ap-south-1; processors (Sentry, ad network, SMS) reviewed for notified-country restrictions; documented processor list |
| Consent manager interoperability | Design consent records machine-readable (structured, versioned) for a future registered consent-manager flow |
| Significant Data Fiduciary triggers (DPO, audits) | Below thresholds at launch; growth checklist to re-assess at 2 crore users |

## 4. IT Act & IT Rules 2021 — intermediary obligations

| Obligation | Implementation |
|---|---|
| Grievance officer (published name, contact, response times) | `/grievance` page + policy; **24 h acknowledgement, 15-day disposition** targets; ticketed queue = moderation reports + dedicated channel |
| Content removal for prohibited content | Chat redaction (`chat_messages.redacted`), user suspension, evidence retention 90 days |
| Fast-track categories (self-harm, explicit) | 24-hour removal SLA in moderation runbook |
| User agreement + rules & regulations | Published ToS, Fair Play policy, Chat policy, Gold disclosure policy |
| Monthly compliance report | Required for significant intermediaries (> 50 lakh users) — we publish a voluntary quarterly transparency report from day 1 (flags, actions, SLAs) |
| Traceability / identification duties | N/A at launch (no messaging beyond in-game chat; no legal order workflow designed — counsel to review if a notice arrives) |

## 5. Consumer Protection Act 2019 + CCPA Dark Patterns Guidelines 2023

| Risk pattern | Our guard |
|---|---|
| False urgency / countdown pressure | No purchase urgency exists; streak resets are honest (shown in IST, miss-day = reset, no "lose everything" cliffs) |
| drip pricing / hidden charges | Nothing is purchasable; ads economics disclosed plainly ("max 5/day") |
| Forced action | Rewarded ads are opt-in surfaces; closing = no penalty |
| Basket sneaking / confirm-shaming | No checkout flows at all |
| Misleading ad claims (ASCI) | All reward claims match server truth (single source: `app_config` rendered to client via `/meta/config`) |

## 6. Brand & advertising posture

- "Hustle" must never read as betting: marketing copy bans gambling lexicon ("stake", "bet", "pot", "cash out") — lint list in brand QA.
- Any creator/influencer collaborations later follow ASCI influencer guidelines (disclosure labels).
- Rewarded-ads surfaces labeled "Ad · earn 2,000 Gold" — reward stated before the ad starts, not after (honesty + store policy).

## 7. App-store policy (Google Play / Apple App Store)

| Requirement | Plan |
|---|---|
| Data safety / privacy nutrition labels | Generated from actual collection table (phone, email optional, gameplay, chat) — no SDK-surprise audits |
| Ads declaration + ads policy | Rewarded only, user-initiated, never mid-game; ads clearly distinguishable; no misleading claims |
| Target audience | 18+ positioning; not Families-designed; IARC rating questionnaire accordingly (violence: abstract chess) |
| In-app purchases | None declared (true: no billing anywhere) |
| Account deletion (Play requirement) | Self-service delete + web path, within-grace cancel documented |
| Content: user-generated (chat) | Report/block/moderation flows documented in listing review notes |
| Rewards/economy policies | "No real-world value" disclaimers in-app and in listing; no simulated gambling content (we have none) |
| Apple specifics | No ATT prompt (non-personalized ads only at v1); IAP rules N/A; account-deletion-in-app satisfied |

## 8. Tax & finance posture (flag for CA; not engineering-blocking)

- **No consumer payments** ⇒ no payment-aggregator/RB II obligations.
- Ad revenue from foreign ad networks: assess GST (export of service vs OIDAR reverse-charge), withholding tax on foreign receipts, invoicing entity choice (India entity assumed) at first payout.
- No GST on player-facing flows (nothing sold to players). Gold issuance/sinks are not taxable supplies.

## 9. Required artifacts before launch (checklist)

- [ ] Privacy Policy (DPDP notice: purposes, retention, rights, grievance, processors)
- [ ] Terms of Service (incl. Fair Play policy, sanctions, appeals)
- [ ] Gold & Rewards disclosure page (non-monetary statement everywhere Gold appears)
- [ ] Grievance officer page (`/grievance`) with IT-Rules timelines
- [ ] Cookie/tracker notice (web: essential-only at v1)
- [ ] Security.txt + vulnerability disclosure policy
- [ ] Data-retention schedule (from schema §15) + deletion job evidence
- [ ] Processor list (SMS, OAuth, ad network, push, error tracking, cloud) with DPAs
- [ ] Breach-notification runbook (roles, channels, board-notice templates)
- [ ] Counsel sign-off memo on: ADR-011 exclusion list, ADR-010 age posture, rewarded-ads design, ToS
- [ ] Quarterly compliance review cadence (owner + counsel) — standing calendar item

## 10. Feature-gate table (legal review required before build)

| Proposed feature | Why gated | Gate |
|---|---|---|
| Family/kids mode (13–17) | DPDP children + ads restrictions | Counsel-reviewed design (Phase 4) |
| Gold-entry tournaments | Stake-adjacent (ADR-011 §2) | External counsel opinion; default: never |
| Cosmetics marketplace (user trading) | Transfer economy | Never without ADR-011 revision |
| Cash/coupon prize tournaments | Cash-equivalent rewards | Never without ADR-011 revision |
| Paid Gold (any form) | Kills the no-money-game posture | Prohibited by mandate |
| Third-party analytics SDKs | DPDP processors + tracker policy | Privacy review each vendor |
| DMs/clubs (open social surfaces) | IT Rules content scope widens | Legal + trust review (Phase 4+) |
