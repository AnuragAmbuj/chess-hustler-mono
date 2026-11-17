# Gold Economy — Chess Hustle

**Status:** Proposed · **Currency:** Gold (fictional, non-monetary) · Related: ADR-006 (ledger), ADR-011 (legal boundary), `02-data/schema.md` §7

---

## 1. Design principles

1. **Not money, provably.** No purchase, transfer, redemption, or exchange path exists in code, API, DB (no transfer reason code), or admin tooling. Every Gold surface repeats the disclosure: *"Gold is for fun and progression only — it has no monetary value and can't be bought, sold, or exchanged."*
2. **Earned, not bought.** All issuance flows from play, streaks, missions, achievements, and optional rewarded ads. Progress speed is effort-indexed.
3. **Every unit accountable.** Append-only ledger; `Σ deltas = balance`; daily source/sink dashboards; fraud queries are single-index lookups.
4. **No dark patterns.** No gacha, no bundles, no fake urgency, no paywall in disguise. Claims are honest ("earn up to 2,000 Gold per ad, max 5/day").
5. **Tunable, not hardcoded.** All rates live in `app_config` (admin API + audit); experiments via feature flags with rollout %.

## 2. Currency rules (normative)

| Rule | Value |
|---|---|
| Welcome grant | **50,000** at signup, single, idempotent (`welcome:{identity}`) |
| Balance floor | 0 (DB `CHECK`); purchases above balance fail with `gold.insufficient` |
| Transfer between users | **Impossible by design** (no API, no reason code, no endpoint) |
| Purchasable | **No.** No IAP, no web billing, no offerwalls that pay cash |
| Redeemable | **No.** Nothing external ever granted for Gold |
| Max balance | No cap needed structurally; anomaly flags trigger at > 3× the 99th percentile of daily issuance for the account class |
| Display | Always shown as "Gold" with the ⚜/◈ mark, never ₹, $, or coin imagery implying cash |

## 3. Sources (issuance)

Defaults below are `app_config` values (`economy.*`), seeded at deploy, adjustable by admin with audit.

### 3.1 Welcome
| Amount | Trigger |
|---|---|
| 50,000 | First verified signup (T1). Shown with first-launch tutorial moment + disclosure. |

### 3.2 Daily streak (RW-1)
7-day escalating loop, one claim per IST day; a missed day resets to day 1:

| Day | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
|---|---|---|---|---|---|---|---|
| Gold | 500 | 750 | 1,000 | 1,500 | 2,000 | 3,000 | 5,000 |

(7-day average ≈ 1,964/day.) Claim is `T4`: PK `(user_id, ist_date)` conflict = already claimed; streak read/advanced in same TXN; ledger idem `daily:{user}:{ist_date}`.

### 3.3 Missions (RW-2)
3 daily missions (rotate from a seeded pool, deterministic per user+day) + 1 weekly. Progress accrues server-side from game/ad lifecycle events (no client reporting):

| Example mission | Goal | Reward |
|---|---|---|
| Play 3 blitz games | 3 games | 150 |
| Win any rated game | 1 win | 200 |
| Play 10 moves in one game | 10 plies (single game) | 250 |
| Complete 1 rewarded ad | 1 ad | 100 |
| Play a friend | 1 game | 150 |
| **Weekly:** play 15 games | 15 games | 1,500 |

Typical daily mission income ≈ 400–600 Gold.

### 3.4 Achievements (RW-3) — one-time ladder
| Milestone | Gold |
|---|---|
| First win | 500 |
| First checkmate in ≤ 30 moves | 750 |
| 10 / 50 / 100 / 500 / 1,000 games | 500 / 1,000 / 2,000 / 5,000 / 10,000 |
| First tournament played / first podium | 1,000 / 5,000 |
| Reach 1400 / 1600 / 1800 / 2000 (any pool) | 2,500 / 5,000 / 10,000 / 25,000 |
| 7-day / 30-day streak | 1,000 / 7,500 |

### 3.5 Gameplay results (GL earning loop)
Per completed rated or casual game with ≥ 8 plies (farming guard), any pool:

| Outcome | Gold |
|---|---|
| Win | 25 |
| Draw | 10 |
| Loss | 5 |
| Daily cap (gameplay source) | 500 |

Rated and casual both count (rating is rated play's own reward; Gold keeps casual meaningful). Tournament games count, plus podium bonuses from `prize_config` (issuance, not pot — ADR-011).

### 3.6 Rewarded ads (AD) — full spec in `04-economy/rewarded-ads.md`
| Amount | Trigger |
|---|---|
| 2,000 / completed ad (experiment A) | Server-verified SSV callback; max 5/day/user; 60 s cooldown; eligibility gates (AD-3) |

### 3.7 Daily maximums (transparency)
| Source | Max/day |
|---|---|
| Streak | 5,000 (day 7) |
| Gameplay | 500 |
| Missions | ~600 daily + 1,500 weekly amortized |
| Ads | 10,000 (5 × 2,000) |
| **Theoretical max** | ≈ 16,700/day (ads-dominant by experiment design) |

Ads dominating the daily max is intentional for the experiment window (measures ad-driven retention); if inflation tracks badly (sink ratio §6), first lever is `ads.gold_per_ad` / `max_per_day`, not gameplay rates.

## 4. Sinks (destruction) — cosmetics only

| Category | Price band | Examples |
|---|---|---|
| Boards | 5,000–75,000 | `board_chaturanga` (30,000), `board_ipl_night` (15,000), `board_metro_map` (8,000) |
| Piece sets | 5,000–75,000 | `pieces_classic_cream`, `pieces_wah_taj` (50,000, legendary) |
| Avatars | 2,000–20,000 | knight mascots, regional motifs |
| Profile frames | 10,000–50,000 | season frames, tournament podium frames |

Rules: prices are whole Gold, always visible before purchase, no randomized anything, no expiry on owned items, seasonal items return (no FOMO pressure theater). A mid-tier earner (≈ 4–6k/day incl. streak amortization) buys one mid cosmetic every 1–2 weeks.

## 5. Accounting & integrity (normative)

- Every mutation = one `gold_ledger` row + balance update in the **same transaction**, using the write pattern in `02-data/schema.md` §7 (row lock + `CHECK (balance >= 0)` + idem key unique index).
- Reason codes (closed enum): `welcome, daily_streak, mission, achievement, gameplay, ad_reward, cosmetic_purchase, support_grant, adjustment`.
- **Support grants** (`support_grant`): admin-only, ≤ 5,000/day per admin, reason mandatory, visible in the user's own ledger ("we added Gold for you — support"), audit-logged. There is no other backdoor.
- `Σ ledger.delta per user == gold_balances.balance` — nightly reconcile job alerts on any drift (none is possible through app paths; the check catches operator error).
- Invariant tests: overdraft impossible, idem-key retry no-op, concurrent purchases serialize, ledger/balance equality.

## 6. Economy health dashboard (analytics)

| Metric | Target |
|---|---|
| Daily issuance vs sink ratio | 0.7–1.1 (sinks absorb 70–110% of non-ad issuance over a week) |
| Median account balance curve | Rising slowly; no "poor forever" cliff (buyback feel) |
| % of issuance from ads | Tracked; > 60% sustained ⇒ review ad rates |
| Cosmetic conversion (MAU) | 8–15% healthy for cosmetics-only sinks |
| First-purchase latency | Median < day 10 |
| Anomaly flags per 1k accounts | < 1 with ≥ 60% upheld |

Config changes that move these (ads rates, cosmetic prices, mission pools) are experiment-flagged where possible and reviewed weekly in P1–P2.

## 7. Abuse model & defenses (summary; threat-model has the full matrix)

| Attack | Defense |
|---|---|
| Self-play farming (two accounts, one player) | ≥ 8-ply guard, draw-farming & pair-farming flags, gameplay daily cap, same-IP pairing analysis (ADR-014) |
| Ad farming (device farm, many accounts) | AD-3 gates (account age, games played), per-IP ad-session caps, ad_suspension sanction (economy-only) |
| Refund/churn-farming missions | Server-side progress only; claims idempotent; no client-reported counters |
| Negative balance via race | `CHECK (balance >= 0)` + row lock — structurally impossible |
| Admin abuse | Dual control above cap, immutable audit log, grant reasons surfaced to user |
