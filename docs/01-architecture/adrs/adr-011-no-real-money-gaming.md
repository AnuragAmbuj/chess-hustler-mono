# ADR-011 — Hard boundary: no real-money gaming mechanics (Online Gaming Act, 2025)

**Status:** Proposed · **Date:** 2026-11-17 · **Highest-priority ADR in this set**

## Context
India's **Promotion and Regulation of Online Gaming Act, 2025** prohibits "online money games" — games involving stakes, entry fees, or cash/cash-equivalent winnings — and prohibits their offering, advertising, and financial facilitation. Enforcement has already removed major RMG platforms from the market. Chess Hustle's Gold (non-purchasable, non-transferable, zero monetary value) exists precisely to stay **outside** that regime while still giving players a reward loop. The mandate requires any staking/pot/paid-entry mechanic to be a separate, legally-assessed feature — never silently added.

## Decision
**The following are excluded from the product by architecture and by review checklist, until a dedicated legal assessment (external counsel sign-off) explicitly re-opens them:**
1. Any wagering or staking (Gold, cosmetics, or anything else).
2. Entry fees for tournaments — even in Gold (a Gold-entry/Gold-prize tournament resembles a stake model; free-entry/Gold-prize is fine).
3. Winner-takes-pot or pot-based prize distributions.
4. Gold transfer/gift between users; marketplace/trading of cosmetics.
5. Any redemption: Gold → money, prizes, goods, vouchers, external benefits.
6. Purchases of Gold or anything with gameplay value (no IAP at all).
7. Loot boxes / gacha / randomized rewards (dark-pattern + gambling-adjacent).
8. Crypto/NFT/web3 anything.

**Structural safeguards (in code, not policy alone):**
- `gold_ledger` has no transfer reason code; no API can mutate two users' balances in one operation.
- Admin tooling cannot create "prize pots" — tournament prize config is issuance against our own budget, audited.
- A quarterly compliance review re-certifies the exclusion list; any engineer PR touching economy endpoints references this ADR in the description.

## Consequences
- + We are legally positioned as a free-to-play social game, not an online money game — the entire basis for launch without gaming licenses.
- + Brand/advertising is clean (ASCI + OGA ads prohibitions apply to money games, not us).
- − Some engagement mechanics used by competitors (stake matches) are permanently unavailable without a separate legal workstream — accepted by mandate.

## Alternatives
- "Small" Gold stakes with disclaimers: rejected — stakes with prizes are the RMG definition regardless of disclaimer.
- "Free entry, cash prizes" tournaments: rejected — cash-equivalent prizes + contest mechanics trigger the Act and consumer-law sweepstakes rules.
- Regional variant (state-law arbitrage): moot — the national Act supersedes; no state arbitrage exists.
