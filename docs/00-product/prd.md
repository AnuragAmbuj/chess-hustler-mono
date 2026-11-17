# PRD — Chess Hustle

**Status:** Proposed (v0.9) · **Owner approval required before implementation** · **Date:** 2026-11-17

---

## 1. Summary

Chess Hustle is a free-to-play, server-authoritative online chess platform built for India's record chess boom. Players get **50,000 Gold** at signup, earn more through daily rewards, missions, achievements, rated/casual play, and **optional rewarded ads**, and spend Gold only on **cosmetics** (boards, piece sets, avatars, frames). Gold has **no monetary value**, cannot be bought, transferred, or redeemed, and is strictly a progression mechanic — the "Gold Economy", never a wallet.

Monetization at launch is **rewarded ads only** (2,000 Gold per completed ad, max 5/day, server-verified via SSV). No subscriptions, no purchases, no cash mechanics.

## 2. Why now — the India context

- India crossed 85+ Grandmasters, and the 2024 World Championship win catalyzed a mass-market chess surge: school programs, YouTube chess creators, and tier-2/tier-3 uptake.
- Existing platforms are foreign-first: USD pricing signals, no Indian languages, ~120–200 ms latency from Indian metros to US/EU servers, and monetization that assumes credit cards.
- India's **Online Gaming Act, 2025** shut down real-money gaming nationwide. Free-to-play social games with non-monetary virtual currency are explicitly outside that regime — a structural advantage for a compliant, ad-funded chess product.
- Rewarded-ads monetization suits Indian F2P habits (no card payment friction; effort-not-money progression).

**Positioning:** *"Chess for the hustle generation"* — fair play, fast Indian servers, desi flavor, rewards for showing up. Anti-positioning: never a "chess betting app". Brand language and marketing must avoid gambling connotations (see `05-security/compliance-india.md` §6).

## 3. Goals, non-goals, anti-goals

**Goals (v1)**
1. G1 — Production-grade server-authoritative multiplayer chess: legal-move validation, FEN/PGN, clocks, threefold/50-move handling, reconnection, history, export.
2. G2 — Liquidity mechanics: ranked + casual pools across bullet/blitz/rapid/classical with sane wait times at 500+ CCU.
3. G3 — Retention loops: daily streaks, missions, achievements, seasons leaderboards.
4. G4 — Gold Economy with honest earning (incl. rewarded ads) and cosmetic-only sinks.
5. G5 — India-first: ap-south latency < 25 ms RTT from metros, English + Hindi UI at v1, ₹-relevant UX patterns, DPDP-compliant consent and data rights.
6. G6 — Trust & safety: reports, moderation, anti-cheat detection with human review, IT-Rules grievance channel.
7. G7 — Tournaments v1 (Swiss, free entry, Gold prizes) and friends/social graph.
8. G8 — Operational readiness: observability, SLOs, cost-aware infra, staged rollout.

**Non-goals (v1)**
- N1 — No native monetization other than rewarded ads (no subscriptions, no IAP, no battle pass purchases).
- N2 — No spectating (Phase 4), no bots (Phase 4), no Chess960/variants (Phase 4).
- N3 — No in-app chat DMs (in-game chat only), no clubs/teams (Phase 4).
- N4 — No cross-region play; single region ap-south-1 at launch.
- N5 — No Family/kids mode (Phase 4, needs counsel-reviewed DPDP design).

**Anti-goals (would make the product a failure even if it "works")**
- A1 — Any client-trusted game outcome (client sends results, client clock truth).
- A2 — Any cash-adjacent mechanic: staking, entry fees, prize pots, Gold transfer/redemption, gacha/loot boxes, pay-to-win advantages.
- A3 — Dark patterns: false urgency timers, drip pricing, obscuring ads-with-rewards economics.
- A4 — Hardcoded tunables in code (all rates/quotas/config-driven).
- A5 — A "toy clone": no graceful-degradation shortcuts that break at scale (e.g., polling instead of WS, in-memory-only state).

## 4. Personas

| Persona | Who | What they need |
|---|---|---|
| **Ananya, 19, college blitz grinder** | Plays 15–30 blitz games/day between classes; 1300–1500 | Fast matchmaking, clean board on mid-range Android, streaks, cosmetics to flex, zero cost |
| **Vikram, 28, IT professional** | 3–5 rapid games on commute/evenings | 10+5 pool, resume-after-network-drop (metro tunnels), push notifications for matches |
| **Coach Iyer, 45, academy organizer** | Runs a Chennai chess academy; wants club events | Free Swiss tournaments with certificates-style standings (B2B tools later) |
| **Rohan, 21, ad-earning student** | Watches rewarded ads for Gold, saves for the "Chaturanga" board set | Honest ad economics, clear quotas, no interruptions mid-game |
| **Priya, 26, returning casual** | Played as a kid, rusty | Casual pool, post-game PGN export, gentle missions, no rating anxiety |

## 5. Functional requirements

IDs are testable and referenced by the test plan (`06-testing/strategy.md`). Domain prefixes: **ID** identity · **PRO** profile · **MM** matchmaking · **GM** game orchestration · **RT** realtime · **RA** ratings · **GL** Gold economy · **RW** rewards (daily/missions/achievements) · **AD** ads · **CS** cosmetics · **TN** tournaments · **FR** friends · **NT** notifications · **MD** moderation · **AC** anti-cheat · **AN** analytics.

### 5.1 Identity & profiles (ID, PRO)
- **ID-1** Signup via mobile OTP (India-first, +91 default) or Google Sign-In. Age gate: self-declared "I confirm I am 18 or older"; refuse account creation otherwise (ADR-010).
- **ID-2** DPDP consent capture at signup with per-purpose granular toggles (essential/functional vs marketing vs personalised ads), versioned and auditable (ADR-008, `05-security/compliance-india.md`).
- **ID-3** Session model: short-lived access JWT (15 min) + rotating refresh tokens per device; single-use WS tickets; logout-one-device and logout-all.
- **ID-4** DPDP rights: self-service data export (async, ≤ 30 days, we target ≤ 7), correction, and account deletion (30-day grace, then pseudonymization that preserves game-history integrity).
- **PRO-1** Unique handle (@, 3–20 chars), display name, avatar (from cosmetics), bio, country pinned IN default.
- **PRO-2** Public profile: per-pool ratings + RD, win/loss/draw counts, peak ratings, streak, achievements, recent rated games.
- **PRO-3** Settings: board theme defaults, sound, chat opt-out, notification quiet hours (IST-aware), personalised-ads toggle.

### 5.2 Matchmaking (MM)
- **MM-1** Pools: bullet/blitz/rapid/classical × rated/casual × standard chess. Time-control presets per pool (config-driven, e.g., 1+0, 3+0, 3+2, 5+0, 10+0, 10+5, 15+10, 30+20).
- **MM-2** Rating-window widening over wait time (start ±75, +15/s, cap ±700); provisional players (< 8 rated games in pool) preferentially paired with provisional.
- **MM-3** Never pair users who have blocked each other; avoid immediate rematch (same pair within last 2 games, same pool) when alternatives exist.
- **MM-4** Color balancing: track each player's last-10 color count; assign colors to minimize imbalance; deterministic RNG.
- **MM-5** Atomic pairing under concurrency (no double-pairing, no orphan queue entries); queue leave is immediate and safe.
- **MM-6** Server returns estimated wait and current pool size; events on match.

### 5.3 Game orchestration (GM)
- **GM-1** Server is the sole authority for legality (all rules incl. en passant, castling, promotion, pinned pieces), checkmate/stalemate, insufficient material, threefold auto-draw, 50-move auto-draw (100 halfmoves), and timeout outcomes (incl. insufficient-mating-material draw rule).
- **GM-2** Clocks are server-side; per-move server timestamps; increment applied on completed move; sweep-based flag detection.
- **GM-3** Full move record persisted per ply (UCI, SAN, resulting FEN, both remaining clocks, server timestamp) → deterministic replay + PGN export with headers (Event, Site, Date, Round, White/Black, Result, ECO via small opening book, TimeControl, ply count).
- **GM-4** Rated vs casual; rated affects Glicko-2 only for both-players-present rated games with ≥ 2 completed moves per side... *(normative: rated results require ≥ 4 plies or the game auto-aborts, MM/GM abort rules)*.
- **GM-5** Actions: resign (instant, irrevocable), draw offer (one outstanding; auto-declined by any move), draw accept, abort (before 4 plies, no rating/gold effect; auto-abort if opponent hasn't moved within 20 s of start), timeout claim fallback, rematch handshake (colors swap).
- **GM-6** Anti-flagging fairness: a player disconnected ≥ grace (bullet 10 s, others 20 s) who reconnects resumes with their clock running from server truth; the opponent cannot "steal" the win except via the clock itself.
- **GM-7** Every accepted mutation bumps a monotonic per-game `version`; all events carry it (order/consistency invariant).

### 5.4 Realtime (RT)
- **RT-1** Single WS endpoint; auth via 30 s single-use ticket; channels: `u:{user_id}` (personal), `g:{game_id}` (game), `t:{tournament_id}`.
- **RT-2** Envelope: `{id, ts, ch, type, data}`; `id` is a Redis-stream ID usable as resume cursor per channel.
- **RT-3** Resume: client reconnects with `last_ids` and receives replay of missed events (buffered ≥ 24 h for games, then PG history covers the rest); snapshot+resume is race-free via the game lock.
- **RT-4** Heartbeat ping/pong (15 s), server-side dead-connection reaping, multi-device support (moves accepted from any session of the right player).

### 5.5 Ratings (RA)
- **RA-1** Glicko-2 per pool; provisional RD 350; rating change visible post-game with new rating + delta.
- **RA-2** Provisional players' results affect opponents with reduced weight (Glicko-2 native).
- **RA-3** Leaderboards: weekly top-50 per pool (snapshot) + personal peak; no rating resets.

### 5.6 Gold economy (GL) — summary; full spec in `04-economy/gold-economy.md`
- **GL-1** Welcome grant 50,000 Gold at signup (single, idempotent).
- **GL-2** Earn: daily-streak rewards, missions, achievements, gameplay results (win/draw/loss with daily cap), rewarded ads (see AD).
- **GL-3** Spend: cosmetics only (boards, piece sets, avatars, frames). No gacha, no bundles, no timers that imply scarcity theater.
- **GL-4** Append-only ledger; every balance change has a reason code, reference, and idempotency key; balances can never go negative (DB-enforced).
- **GL-5** No purchase, transfer, gifting, redemption, or exchange path exists in code, API, or admin tooling (admin grants are audited and reason-coded, for support only).
- **GL-6** In-game disclosure copy: "Gold is for fun and progression only. It has no monetary value and can't be bought, sold, or exchanged." Shown at first grant, in the Gold screen, and in the store.

### 5.7 Rewards (RW)
- **RW-1** Daily reward: 7-day escalating streak (500 → 5,000); missed day resets streak; claim is per-IST-day, idempotent.
- **RW-2** Missions: 3 rotating daily missions + 1 weekly; progress server-tracked from game events; manual claim with idempotency.
- **RW-3** Achievements: one-time ladder (first win → 1,000 games → rating milestones → tournament medals), auto-granted server-side.

### 5.8 Rewarded ads (AD) — summary; full spec in `04-economy/rewarded-ads.md`
- **AD-1** Mobile-only (Android first; iOS when it ships). Web PWA has no rewarded ads in v1 (no viable rewarded-web provider); server flag-gates availability.
- **AD-2** Experiment defaults: 2,000 Gold per completed ad, max 5/day/user, 60 s cooldown between sessions; all values server-config, admin-adjustable.
- **AD-3** Eligibility: account age ≥ 24 h, ≥ 5 completed games, no active sanctions; **no ad sessions while an active game exists on any device**; 18+ only.
- **AD-4** Grant happens exclusively on **server-verified SSV callback** with signature verification, TTL checks, and transaction-id dedupe (replay-safe). Clients can never report their own reward.
- **AD-5** Provider-agnostic port (`AdVerifier` trait) with AdMob SSV implementation first.

### 5.9 Cosmetics (CS)
- **CS-1** Catalog: boards (incl. Indian-motif sets), piece sets, avatars, profile frames; prices 2,000–75,000 Gold.
- **CS-2** Ownership table + loadout equipping; previews in store; no randomized purchases.
- **CS-3** Purchase is transactional (balance lock + ledger + inventory in one DB transaction), idempotent.

### 5.10 Tournaments (TN)
- **TN-1** Swiss system, free entry, Gold prizes to top N (prize splits are config; **no entry fee** — paid entry is excluded pending legal review, ADR-011).
- **TN-2** Registration window, capacity cap, round scheduling, auto-pairing by score-then-rating with rematch avoidance, byes per simplified FIDE rules, standings with Buchholz tiebreak.
- **TN-3** Tournament games are rated and use the tournament time control; all game lifecycle rules apply.

### 5.11 Friends & social (FR)
- **FR-1** Friend requests (search by handle), accept/decline, remove; block list (blocks also prevent pairing and chat).
- **FR-2** Online presence for friends; challenge friends to casual or rated games with any time control (challenge = pending game via WS invite flow).

### 5.12 Notifications (NT)
- **NT-1** Push (FCM Android/web-later, APNs iOS phase 3) + in-app notification feed: match found, game invite, tournament start, daily reward available, friend events.
- **NT-2** Quiet hours (default 22:00–07:00 IST, user-configurable); transactional vs marketing consent separation; marketing requires explicit opt-in.

### 5.13 Moderation & anti-cheat (MD, AC)
- **MD-1** In-game chat: rate-limited, profanity-masked (incl. Hinglish/transliteration list), quick-chat presets, report + block; stored 90 days for moderation evidence.
- **MD-2** Report categories (cheating/abuse/other) with evidence snapshot; moderation queue with actions (warning, mute, matchmaking suspension, ad-earning suspension, ban) — all audited.
- **MD-3** Grievance officer channel per IT Rules 2021 (published contact, 24 h acknowledgement, 15-day disposition target).
- **AC-1** Anti-cheat: server legality is absolute; offline statistical analysis (engine-match %, move-time anomalies, pairwise collusion patterns, mult accounting heuristics) produces *flags*, never auto-bans; human review required for sanctions (ADR-014).
- **AC-2** Ad-farming defenses: per-account gates (AD-3), per-IP ad-session caps, device-anomaly analytics, sanction path (ad-earning suspension without game ban).

### 5.14 Analytics (AN)
- **AN-1** First-party event taxonomy (signup, game lifecycle, economy, ads, social) → partitioned `app_events` (90-day retention) with nightly export to object storage.
- **AN-2** Dashboards: DAU/retention, games/day, matchmaking wait p50/p95, ad funnel (shown → completed → credited), gold sources/sinks ratio, cheat-flag rate, infra cost per DAU.
- **AN-3** No third-party trackers in v1 (DPDP posture); crash reporting (Sentry self-hosted or SaaS) with PII scrubbing.

## 6. Non-functional requirements

| NFR | Target |
|---|---|
| Move-accepted latency (server processing, p95) | ≤ 100 ms (p50 ≤ 30 ms) |
| WS connection establishment p95 | ≤ 300 ms |
| Match found (p50, 500+ CCU seeded pool) | ≤ 30 s blitz; ≤ 15 s at 2,000+ CCU |
| Uptime (monthly) | 99.9% API/WS (≈ 43 min/mo budget) |
| Data durability | PG sync-replicated within AZ; zero accepted data loss for completed games under single-AZ failure; RPO ≤ 5 min (PITR), RTO ≤ 30 min |
| Reconnect correctness | Zero lost moves; resume works across server restarts |
| Scale target v1 | 10k concurrent WS, 50k MAU, 3k games/hour sustained without architectural change |
| Latency from Indian metros | ≤ 25 ms RTT to ap-south-1 |
| Accessibility | Board meets contrast AA; all flows keyboard-navigable on web |
| i18n | English + Hindi at v1; copy externalized; IST-first time handling |

## 7. KPIs

- **Engagement:** D1/D7/D30 retention; games per DAU; streak length distribution.
- **Matchmaking:** queue→match p50/p95 by pool; abort rate; rematch rate.
- **Fairness:** cheat-flag rate per 1k games; report uphold rate; smurf concentration.
- **Economy health:** daily gold issued vs sunk (target sink ratio 0.7–1.1 of non-ad issuance); cosmetic conversion (gold spenders/MAU); average first purchase day.
- **Monetization:** ad impressions/DAU (capped 5), ad completion rate, verified-reward credit rate, effective eCPM, ad revenue / DAU.
- **Trust:** grievance SLA compliance 100%; false-ban rate ~0 (human review).
- **Ops:** SLO error budgets; cost per DAU (₹).

## 8. Risks & mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Cold-start liquidity (empty pools) | High | Bot opponents experiment (Phase 4 flag), cohort-based invites, academy partnerships, casual+rated shared visibility |
| Ad fraud / gold farming | Economy inflation | AD-3 gates, IP caps, anomaly flags, suspend ad-earning (not account) first |
| Cheating (engine use) | Trust erosion | AC-1 statistical flags + human review + sanctions ladder; publish fairness report |
| "Hustle" brand misread as gambling | Legal/PR | Compliance-guarded brand copy; explicit no-cash disclaimers everywhere Gold appears |
| Play Store ads policy friction | Launch delay | Rewarded-only, user-initiated, never mid-game; data-safety form prepared early |
| DPDP enforcement maturing | Rework | Consent architecture from day 1 (ADR-008); counsel review before Phase 3 features |
| Scope creep (variants, spectate, DMs) | Timeline | Non-goals N1–N5 enforced by roadmap exit criteria |
| Single-region outage | Availability | Multi-AZ, runbooks, RTO 30 min; multi-region is a Phase 5 cost decision |

## 9. Competitive landscape & differentiation

| | Chess.com | Lichess | **Chess Hustle** |
|---|---|---|---|
| Servers for India | US/EU (150 ms+) | EU (150 ms+) | **Mumbai (≤ 25 ms)** |
| Languages | Many | Many | English + Hindi → regional roadmap |
| Free tier | Ad-limited, paywalled features | Fully free, donation-funded | **Fully free + 50k Gold + rewarded ads for progression** |
| Monetization | Subscription + IAP | Donations | **Rewarded ads only** |
| India-native mechanics | — | — | Desi cosmetic themes, IST-native daily loops, academy/club tools (B2B later) |

Differentiation is *regional product fit + compliance-clean economy*, not engine features.

## 10. Out of scope / excluded by hard boundary (ADR-011)

Entry fees (even in Gold), wagering, winner-takes-pot, cash or cash-equivalent prizes, Gold transfer/gifting between users, redemption/exchange to anything of value, crypto/NFTs, loot boxes/gacha, pay-to-win. Any proposal touching these stops at a dedicated legal assessment — it is never "silently added".

## 11. Glossary

| Term | Meaning |
|---|---|
| **Gold** | Fictional, non-monetary in-game progression currency. Never purchasable/transferable/redeemable. |
| **Pool** | A matchmaking bucket: {bullet, blitz, rapid, classical} × {rated, casual}. |
| **Provisional** | Fewer than 8 rated games in a pool (Glicko-2 RD ≥ 350 → high uncertainty). |
| **Flag / flagging** | Losing on time when the clock reaches zero. |
| **Grace** | Disconnect window in which an opponent cannot *additional*-claim (the clock itself still runs). |
| **SSV** | Server-side verification of ad rewards via provider-signed callbacks. |
| **Ad session** | Server-minted attempt at one rewarded ad; the unit of quota, cooldown, and idempotency. |
| **Sink / source** | Gold destruction vs issuance; ledger taxonomy with reason codes. |
| **Modular monolith** | One deployable Rust binary; domain crates enforce boundaries; no cross-domain SQL. |
| **BYO-TC** | Challenge-time custom time control (friends only; pool games use presets). |

## 12. Open decisions for the owner (non-blocking defaults chosen)

1. Domain: `chesshustle.in` placeholder until owned (infra doc).
2. Brand name legal check ("Chess Hustle" trademark/Class 9/41 search) before store submission — flagged, not blocking docs.
3. Ad partner: AdMob SSV chosen as v1 default; port trait allows Unity/ironSource later.
4. Bot opponents remain Phase 4 (anti-goal: fake liquidity with bad bots).
