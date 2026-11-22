# Chess Hustle

Chess Hustle is a server-authoritative online chess platform built with a high-performance Rust backend and a React/TypeScript web client. It is designed around fair play, reliable clock synchronization, strict contract enforcement, and a non-monetary virtual currency economy.

## Overview

The platform provides live multiplayer chess with competitive matchmaking, automated Swiss-system tournaments, and a progression economy. All game states and clock calculations are validated on the server to prevent client-side manipulation.

Key engineering principles:

- Server-Authoritative Engine: Game state transitions, legal move generation, and time deductions are evaluated server-side.
- Zero Contract Drift: The OpenAPI contract serves as the source of truth. Integration tests enforce bidirectional parity with server routes.
- Monetary Compliance: Operating under a strict virtual-only economy (ADR-011). Virtual Gold has zero real-money cashout value and is earned solely through gameplay, streaks, missions, and optional rewarded ads.
- Cryptographic Verification: Rewarded ad callbacks use server-side verification (SSV) via NIST P-256 ECDSA signatures to prevent reward replay attacks.

## Architecture

The project is organized as a monorepo containing the Rust server workspace, OpenAPI and AsyncAPI specifications, and the web client.

```
├── contracts/               # OpenAPI and AsyncAPI specifications
├── docs/                    # Architecture decision records (ADRs) and product specs
├── infra/                   # Docker Compose configurations and deployment manifests
├── server/                  # Rust workspace
│   ├── crates/
│   │   ├── hustle-ads       # Rewarded ads with P-256 ECDSA SSV verification
│   │   ├── hustle-api       # Axum HTTP routes and WebSocket handlers
│   │   ├── hustle-chess     # Chess rules engine wrapper and clock arithmetic
│   │   ├── hustle-cosmetics # Virtual items catalog and loadout management
│   │   ├── hustle-domain    # Core domain models, rating math, and economy rules
│   │   ├── hustle-friends   # Social graph, presence, and direct challenges
│   │   ├── hustle-gold      # Transactional double-entry gold ledger and streaks
│   │   ├── hustle-identity  # Authentication, OTP verification, and JWT sessions
│   │   ├── hustle-kernel    # Configuration management and telemetry
│   │   ├── hustle-matchmaking # Rating-bracket queue sweeper and match allocation
│   │   ├── hustle-moderation # In-game chat moderation, profanity masking, and anti-cheat
│   │   ├── hustle-ratings   # Golden-tested Glicko-2 implementation
│   │   ├── hustle-realtime  # WebSocket framing, envelope codec, and channel routing
│   │   ├── hustle-rewards   # Daily and weekly missions with milestone claims
│   │   ├── hustle-server    # Main executable binary and process composition root
│   │   ├── hustle-testkit   # Shared integration test harness and database isolated runners
│   │   └── hustle-tournaments # FIDE Swiss-system pairing engine and tournament manager
│   └── migrations/          # PostgreSQL schema migrations
└── web/                     # React 19 and TypeScript web client
    ├── public/              # Static piece sets and icons
    └── src/
        ├── api/             # API client methods
        ├── auth/            # Authentication context and session state
        ├── game/            # Board rendering, move validation, clocks, and audio
        ├── lobby/           # Dashboard, tournaments browser, vault, and modals
        ├── realtime/        # WebSocket client with automatic reconnection and gap resume
        └── types/           # Shared TypeScript interfaces
```

## Core Systems

### Chess Engine and Clocks
- Moves are validated server-side using pure rules logic.
- Clocks operate with anchor timestamps and remaining millisecond counters to account for network latency without desynchronization.
- Stalemate, threefold repetition, fifty-move rule, and insufficient material draws are automatically detected.
- Client board supports drag-and-drop, click-to-move, pawn promotion selection, premoves, and multiple piece styles (Merida, Staunty, CBurnett).
- Zero-asset Web Audio synthesis produces distinct cues for moves, captures, checks, and game termination.

### Swiss Tournament Engine
- Implements standard FIDE Swiss-system rules.
- Prevents player rematches across rounds while grouping by score brackets.
- Handles color balancing (alternating white and black) and assigns byes when odd player counts occur.
- Computes Buchholz and Sonneborn-Berger tie-break metrics for standings.
- Automatically advances tournament rounds and disburses Gold prize pools to top finishers.

### Economy and Rewarded Ads
- Double-entry virtual Gold ledger tracks every debit and credit with audit logs and idempotency keys.
- Daily login streak ladder with escalating reward tiers based on Indian Standard Time (IST) resets.
- Rewarded video ad integration supports Google AdMob SSV callbacks with public key caching, timestamp freshness checks (less than 7 days), and daily claim quotas.

### Real-Time Networking
- WebSockets deliver game moves, clock synchronizations, direct challenges, and game state updates.
- Clients store sequence numbers per channel in session storage to enable seamless gap-recovery upon reconnecting.
- Exponential backoff with jitter prevents reconnection stampedes during transient outages.

### Fair Play and Moderation
- In-game chat features real-time dictionary-based profanity masking and mute enforcement.
- Offline anomaly analyzer flags rapid resignation farming, collusion patterns, and engine-like move agreement ratios.
- Administrative console provides user lookup, manual sanctioning (warn, mute, suspend, ban), and grievance reporting.

## Getting Started

### Prerequisites

- Rust 1.79+ (`rustup toolchain install stable`)
- Node.js 20+ and npm
- PostgreSQL 16+ (optional for local standalone web mode; required for server)
- Redis 7+ (optional for standalone mode)

### Running the Web Client

The web client includes a built-in development API middleware that allows all user interfaces, bot matches, tournament views, and economy features to be tested locally without needing external databases.

```bash
cd web
npm install
npm run dev
```

Open `http://localhost:5173` in your browser. From here, you can:
- Play immediately against the local engine bot.
- Simulate matchmaking queues against computer opponents.
- Browse Swiss tournament brackets, round pairings, and standings.
- Test rewarded ad claiming and daily streak collections.
- Switch between dark and light themes and choose piece sets.

### Running the Full Server Stack

To run the backend server against PostgreSQL and Redis:

```bash
# 1. Start database dependencies
docker compose -f infra/docker/compose.yml up -d db redis

# 2. Run migrations
psql "postgresql://postgres:hustle@localhost:5432/hustle" -f server/migrations/0001_init.sql

# 3. Start the server
cargo run --manifest-path server/Cargo.toml -p hustle-server
```

The server listens on `http://127.0.0.1:8080`. The web client will automatically proxy `/v1` REST calls and `/ws` WebSocket connections to this address.

## Testing and Verification

The repository enforces strict quality standards through automated tests and static analysis.

### Workspace Unit Tests
Runs unit tests across all 15 crates in the workspace:
```bash
cargo test --manifest-path server/Cargo.toml --lib --workspace
```

### Contract Drift Verification
Ensures zero drift between Axum server routes and `contracts/openapi.yaml`:
```bash
cargo test --manifest-path server/Cargo.toml --test contract_drift
```

### Integration Test Suite
Executes integration tests against isolated PostgreSQL instances:
```bash
cargo test --manifest-path server/Cargo.toml -p hustle-api --test api_smoke --test ads_pg --test tournaments_pg
```

### Static Analysis and Linters
Verifies clean compilation with all warnings treated as errors:
```bash
cargo clippy --manifest-path server/Cargo.toml --workspace --all-targets -- -D warnings
```

### Web Frontend Build
Verifies TypeScript compilation and production bundling:
```bash
cd web && npm run build
```

## License

Proprietary. All rights reserved.
