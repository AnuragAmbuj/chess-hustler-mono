-- 0001_init.sql — Schema v1 (docs/02-data/schema.md)
-- Policy: forward-only; never edit after merge (checksum gate, ADR-012).
-- Ordering matters: FK dependencies are resolved top-down in this file.

-- =============================================================================
-- Identity & consent (hustle-identity)
-- =============================================================================

CREATE TABLE users (
  id                    bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  status                text NOT NULL DEFAULT 'active'
                        CHECK (status IN ('active','deletion_requested','anonymized','suspended')),
  role                  text NOT NULL DEFAULT 'user' CHECK (role IN ('user','moderator','admin')),
  phone_e164            text UNIQUE,
  email                 text UNIQUE,
  age_confirmed_at      timestamptz,               -- 18+ gate moment (ADR-010)
  deletion_requested_at timestamptz,
  anonymized_at         timestamptz,
  created_at            timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT users_min_identity CHECK (phone_e164 IS NOT NULL OR email IS NOT NULL)
);
-- Users are never DELETEd; anonymization nulls PII (DPDP erasure, schema §3).

CREATE TABLE auth_identities (
  id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  user_id     bigint NOT NULL REFERENCES users(id),
  kind        text NOT NULL CHECK (kind IN ('phone','google')),
  identifier  text NOT NULL,                       -- e.164 phone or Google `sub`
  verified_at timestamptz,
  created_at  timestamptz NOT NULL DEFAULT now(),
  UNIQUE (kind, identifier)
);
CREATE INDEX auth_identities_user_idx ON auth_identities (user_id);

CREATE TABLE user_sessions (
  id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  user_id        bigint NOT NULL REFERENCES users(id),
  family_id      uuid NOT NULL,                    -- rotation family (reuse detection)
  refresh_hash   text NOT NULL UNIQUE,             -- sha256(refresh_token)
  device_label   text,
  ip_hash        text,                             -- hashed; DPDP minimization
  created_at     timestamptz NOT NULL DEFAULT now(),
  last_used_at   timestamptz NOT NULL DEFAULT now(),
  revoked_at     timestamptz,
  revoked_reason text CHECK (revoked_reason IN ('logout','logout_all','rotation_reuse','admin'))
);
CREATE INDEX user_sessions_active_user_idx ON user_sessions (user_id) WHERE revoked_at IS NULL;

CREATE TABLE consent_records (
  id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  user_id        bigint NOT NULL REFERENCES users(id),
  policy_version text NOT NULL,
  purpose        text NOT NULL CHECK (purpose IN ('essential','functional','marketing','personalised_ads','chat')),
  granted        boolean NOT NULL,
  ip_hash        text,
  created_at     timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX consent_records_user_purpose_idx ON consent_records (user_id, purpose, created_at DESC);

CREATE TABLE device_tokens (
  id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  user_id    bigint NOT NULL REFERENCES users(id),
  platform   text NOT NULL CHECK (platform IN ('android','ios','web')),
  token      text NOT NULL,
  updated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (user_id, platform, token)
);

-- =============================================================================
-- Cosmetics first (profiles & loadouts hold FKs to it)
-- =============================================================================

CREATE TABLE cosmetics (
  id             text PRIMARY KEY,
  category       text NOT NULL CHECK (category IN ('board','pieces','avatar','frame')),
  name_key       text NOT NULL,
  price_gold     bigint NOT NULL CHECK (price_gold > 0),
  rarity         text NOT NULL CHECK (rarity IN ('common','rare','epic','legendary')),
  preview        jsonb NOT NULL,                   -- CDN asset refs
  available_from timestamptz,
  available_to   timestamptz,                      -- seasonal windows; no gacha ever
  active         boolean NOT NULL DEFAULT true
);
CREATE INDEX cosmetics_active_category_idx ON cosmetics (category, price_gold) WHERE active;

-- =============================================================================
-- Profiles & social (hustle-profiles, hustle-friends)
-- =============================================================================

CREATE TABLE profiles (
  user_id            bigint PRIMARY KEY REFERENCES users(id),
  handle             text NOT NULL CHECK (handle ~ '^[a-z0-9_]{3,20}$'),
  display_name       text NOT NULL CHECK (char_length(display_name) BETWEEN 2 AND 30),
  bio                text CHECK (char_length(bio) <= 200),
  country            text NOT NULL DEFAULT 'IN',
  avatar_cosmetic_id text REFERENCES cosmetics(id),
  games_count        int NOT NULL DEFAULT 0,       -- denormalized counters,
  wins               int NOT NULL DEFAULT 0,       -- maintained in the finalize TXN
  losses             int NOT NULL DEFAULT 0,
  draws              int NOT NULL DEFAULT 0,
  created_at         timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX profiles_handle_lower_uidx ON profiles (lower(handle));

CREATE TABLE user_settings (
  user_id           bigint PRIMARY KEY REFERENCES users(id),
  chat_enabled      boolean NOT NULL DEFAULT true,
  sound_enabled     boolean NOT NULL DEFAULT true,
  premove_enabled   boolean NOT NULL DEFAULT true,
  quiet_hours_start int NOT NULL DEFAULT 22,        -- IST hour
  quiet_hours_end   int NOT NULL DEFAULT 7,
  marketing_opt_in  boolean NOT NULL DEFAULT false, -- mirrored in consent_records
  updated_at        timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT user_settings_quiet_hours CHECK (quiet_hours_start BETWEEN 0 AND 23
                                          AND quiet_hours_end BETWEEN 0 AND 23)
);

CREATE TABLE friendships (
  id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  requester_id bigint NOT NULL REFERENCES users(id),
  addressee_id bigint NOT NULL REFERENCES users(id),
  status       text NOT NULL DEFAULT 'pending'
               CHECK (status IN ('pending','accepted','declined','removed')),
  created_at   timestamptz NOT NULL DEFAULT now(),
  responded_at timestamptz,
  CONSTRAINT fr_not_self CHECK (requester_id <> addressee_id),
  UNIQUE (requester_id, addressee_id)
);
CREATE INDEX friendships_incoming_pending_idx ON friendships (addressee_id) WHERE status = 'pending';
CREATE INDEX friendships_outgoing_pending_idx ON friendships (requester_id) WHERE status = 'pending';

CREATE TABLE blocks (
  blocker_id bigint NOT NULL REFERENCES users(id),
  blocked_id bigint NOT NULL REFERENCES users(id),
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (blocker_id, blocked_id),
  CONSTRAINT b_not_self CHECK (blocker_id <> blocked_id)
);

-- =============================================================================
-- Tournaments (before games — games.tournament_id FK)
-- =============================================================================

CREATE TABLE tournaments (
  id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  public_id   text NOT NULL UNIQUE,                -- ULID
  name        text NOT NULL,
  tc          jsonb NOT NULL,                      -- one time control for all rounds
  rounds      int NOT NULL CHECK (rounds BETWEEN 3 AND 11),
  round_minutes int NOT NULL,
  status      text NOT NULL DEFAULT 'scheduled'
              CHECK (status IN ('scheduled','registering','running','finished','cancelled')),
  starts_at   timestamptz NOT NULL,
  registration_closes_at timestamptz NOT NULL,
  capacity    int NOT NULL CHECK (capacity <= 512),
  prize_config jsonb NOT NULL,                    -- issuance-only splits (ADR-011)
  created_by  bigint NOT NULL REFERENCES users(id),
  meta        jsonb NOT NULL DEFAULT '{}',
  CONSTRAINT tn_no_fees CHECK (meta->>'entry_fee_gold' IS NULL)  -- ADR-011 guard
);

-- =============================================================================
-- Chess core (hustle-chess)
-- =============================================================================

CREATE TABLE games (
  id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  public_id      text NOT NULL UNIQUE,             -- ULID
  white_user_id  bigint NOT NULL REFERENCES users(id),
  black_user_id  bigint NOT NULL REFERENCES users(id),
  pool           text NOT NULL CHECK (pool IN ('bullet','blitz','rapid','classical')),
  time_control   jsonb NOT NULL,                   -- {"initial_ms":180000,"increment_ms":2000}
  rated          boolean NOT NULL,
  status         text NOT NULL DEFAULT 'starting'
                 CHECK (status IN ('starting','active','finished','aborted')),
  result         text CHECK (result IN ('white','black','draw')),  -- NULL unless finished
  termination    text CHECK (termination IN ('checkmate','stalemate','resignation',
                    'timeout','insufficient_material','agreement','threefold',
                    'fifty_move','aborted','abandoned','forfeit')),
  fen_start      text NOT NULL,
  fen_current    text NOT NULL,
  move_count     int NOT NULL DEFAULT 0,           -- plies played
  w_remaining_ms bigint,
  b_remaining_ms bigint,
  last_tick_ts   timestamptz,                      -- server ts of last mutation
  version        int NOT NULL DEFAULT 0,           -- monotonic per game (lifecycle I-1)
  tournament_id  bigint REFERENCES tournaments(id),
  rematch_of     bigint REFERENCES games(id),
  created_at     timestamptz NOT NULL DEFAULT now(),
  started_at     timestamptz,
  ended_at       timestamptz,
  meta           jsonb NOT NULL DEFAULT '{}',      -- lag credits, flags, notes
  -- Write-once terminal state (lifecycle I-5): second finalization rejected by DB.
  CONSTRAINT games_result_iff_finished CHECK (
    (status = 'finished' AND result IS NOT NULL AND termination IS NOT NULL)
    OR (status IN ('starting','active') AND result IS NULL)
    OR (status = 'aborted' AND result IS NULL)
  ),
  CONSTRAINT games_not_self CHECK (white_user_id <> black_user_id)
);
CREATE INDEX games_white_created_idx ON games (white_user_id, created_at DESC);
CREATE INDEX games_black_created_idx ON games (black_user_id, created_at DESC);
CREATE INDEX games_active_status_idx ON games (status) WHERE status IN ('starting','active');
CREATE INDEX games_tournament_idx ON games (tournament_id) WHERE tournament_id IS NOT NULL;
CREATE INDEX games_created_idx ON games (created_at DESC);

CREATE TABLE moves (
  game_id        bigint NOT NULL REFERENCES games(id),
  ply            int  NOT NULL,                    -- 1-based, server-numbered
  uci            text NOT NULL,
  san            text NOT NULL,
  fen_after      text NOT NULL,
  w_remaining_ms bigint NOT NULL,
  b_remaining_ms bigint NOT NULL,
  ts             timestamptz NOT NULL,
  meta           jsonb NOT NULL DEFAULT '{}',
  PRIMARY KEY (game_id, ply),                      -- I-2: replay order + no duplicate plies
  CONSTRAINT mv_gt_zero CHECK (ply >= 1)
);
CREATE INDEX moves_ts_idx ON moves (ts);

-- =============================================================================
-- Ratings (hustle-ratings)
-- =============================================================================

CREATE TABLE ratings (
  user_id      bigint NOT NULL REFERENCES users(id),
  pool         text NOT NULL CHECK (pool IN ('bullet','blitz','rapid','classical')),
  rating       real NOT NULL DEFAULT 1200,
  rd           real NOT NULL DEFAULT 350,          -- Glicko-2 deviation
  volatility   real NOT NULL DEFAULT 0.06,
  games_count  int  NOT NULL DEFAULT 0,
  peak         real NOT NULL DEFAULT 1200,
  last_game_at timestamptz,
  PRIMARY KEY (user_id, pool),
  CONSTRAINT rd_bounds CHECK (rd >= 30 AND rd <= 350)
);
CREATE INDEX ratings_pool_rating_idx ON ratings (pool, rating DESC);

CREATE TABLE rating_history (
  id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  user_id       bigint NOT NULL REFERENCES users(id),
  pool          text NOT NULL,
  game_id       bigint NOT NULL REFERENCES games(id),
  rating_before real NOT NULL,
  rating_after  real NOT NULL,
  rd_after      real NOT NULL,
  delta         real NOT NULL,
  created_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX rating_history_user_pool_idx ON rating_history (user_id, pool, created_at DESC);

-- =============================================================================
-- Gold economy (hustle-gold) — append-only ledger (ADR-006)
-- =============================================================================

CREATE TABLE gold_balances (
  user_id    bigint PRIMARY KEY REFERENCES users(id),
  balance    bigint NOT NULL CHECK (balance >= 0),  -- DB-enforced non-negativity
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE gold_ledger (
  id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  user_id       bigint NOT NULL REFERENCES users(id),
  delta         bigint NOT NULL CHECK (delta <> 0),
  balance_after bigint NOT NULL,
  reason        text NOT NULL CHECK (reason IN (
                  'welcome','daily_streak','mission','achievement','gameplay',
                  'ad_reward','cosmetic_purchase','support_grant','adjustment')),
  ref_type      text,
  ref_id        text,
  idem_key      text UNIQUE,                       -- NULL for audited admin ops
  meta          jsonb NOT NULL DEFAULT '{}',
  created_at    timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT gl_delta_sign CHECK (
    (delta > 0 AND reason IN ('welcome','daily_streak','mission','achievement',
                              'gameplay','ad_reward','support_grant','adjustment'))
    OR (delta < 0 AND reason IN ('cosmetic_purchase','adjustment'))
  )
);
CREATE INDEX gold_ledger_user_created_idx ON gold_ledger (user_id, created_at DESC);
CREATE INDEX gold_ledger_reason_created_idx ON gold_ledger (reason, created_at);
-- Monthly range partitions on (created_at) from month 1 of live traffic (schema §15).

-- =============================================================================
-- Rewards (hustle-rewards)
-- =============================================================================

CREATE TABLE daily_claims (
  user_id    bigint NOT NULL REFERENCES users(id),
  ist_date   date  NOT NULL,
  streak     int   NOT NULL,                      -- streak position after this claim
  claimed_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (user_id, ist_date)                  -- one claim per IST day (T4)
);

CREATE TABLE mission_defs (
  id          text PRIMARY KEY,                    -- 'daily_win_1', 'weekly_play_15'
  kind        text NOT NULL CHECK (kind IN ('daily','weekly')),
  title_key   text NOT NULL,                       -- i18n key
  goal_type   text NOT NULL CHECK (goal_type IN ('games_played','wins','draws','checkmates',
                       'ads_completed','tournaments_played','minutes_played')),
  goal_params jsonb NOT NULL DEFAULT '{}',
  reward_gold bigint NOT NULL CHECK (reward_gold > 0),
  active      boolean NOT NULL DEFAULT true
);

CREATE TABLE user_missions (
  user_id    bigint NOT NULL REFERENCES users(id),
  mission_id text   NOT NULL REFERENCES mission_defs(id),
  period_key text   NOT NULL,                     -- '2026-11-17' | ISO '2026-W47'
  progress   int    NOT NULL DEFAULT 0,
  goal       int    NOT NULL,
  claimed    boolean NOT NULL DEFAULT false,
  claimed_at timestamptz,
  PRIMARY KEY (user_id, mission_id, period_key)
);

-- Achievement definitions (mirrors mission_defs; achievement ids are stable).
CREATE TABLE achievement_defs (
  id          text PRIMARY KEY,
  title_key   text NOT NULL,
  goal_type   text NOT NULL CHECK (goal_type IN ('games_played','wins','checkmates',
                       'streak_days','rating_peak','tournaments_played','tournament_podium')),
  goal_params jsonb NOT NULL DEFAULT '{}',
  reward_gold bigint NOT NULL CHECK (reward_gold > 0),
  active      boolean NOT NULL DEFAULT true
);

CREATE TABLE user_achievements (
  user_id        bigint NOT NULL REFERENCES users(id),
  achievement_id text   NOT NULL REFERENCES achievement_defs(id),
  granted_at     timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (user_id, achievement_id)
);

-- =============================================================================
-- Ads (hustle-ads) — SSV verification & quota (ADR-007)
-- =============================================================================

CREATE TABLE ad_sessions (
  id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  public_id    text NOT NULL UNIQUE,               -- SSV custom data carrier
  user_id      bigint NOT NULL REFERENCES users(id),
  provider     text NOT NULL CHECK (provider IN ('admob')),
  status       text NOT NULL DEFAULT 'pending'
               CHECK (status IN ('pending','completed','expired','rejected')),
  created_at   timestamptz NOT NULL DEFAULT now(),
  expires_at   timestamptz NOT NULL,               -- created + 30 min
  completed_at timestamptz
);
CREATE INDEX ad_sessions_user_created_idx ON ad_sessions (user_id, created_at DESC);

CREATE TABLE ad_reward_events (
  id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  provider       text NOT NULL,
  transaction_id text NOT NULL,                    -- provider txn id (replay anchor)
  user_id        bigint NOT NULL REFERENCES users(id),
  session_id     bigint NOT NULL REFERENCES ad_sessions(id),
  gold_amount    bigint NOT NULL,
  verified_at    timestamptz NOT NULL DEFAULT now(),
  meta           jsonb NOT NULL DEFAULT '{}',      -- signed-params digest for forensics
  UNIQUE (provider, transaction_id)               -- replay wall (T-05)
);

CREATE TABLE ad_quota_daily (
  user_id  bigint NOT NULL REFERENCES users(id),
  ist_date date   NOT NULL,
  used     int    NOT NULL DEFAULT 0 CHECK (used >= 0),
  PRIMARY KEY (user_id, ist_date)
);

-- =============================================================================
-- Cosmetics inventory (hustle-cosmetics)
-- =============================================================================

CREATE TABLE user_cosmetics (
  user_id     bigint NOT NULL REFERENCES users(id),
  cosmetic_id text   NOT NULL REFERENCES cosmetics(id),
  acquired_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (user_id, cosmetic_id)
);

CREATE TABLE user_loadout (
  user_id   bigint PRIMARY KEY REFERENCES users(id),
  board_id  text REFERENCES cosmetics(id),
  pieces_id text REFERENCES cosmetics(id),
  avatar_id text REFERENCES cosmetics(id),
  frame_id  text REFERENCES cosmetics(id)
);  -- ownership validated in the service (owned cosmetics only)

-- =============================================================================
-- Tournament entries & pairings
-- =============================================================================

CREATE TABLE tournament_entries (
  tournament_id bigint NOT NULL REFERENCES tournaments(id),
  user_id       bigint NOT NULL REFERENCES users(id),
  joined_at     timestamptz NOT NULL DEFAULT now(),
  withdrawn     boolean NOT NULL DEFAULT false,
  final_rank    int,
  prize_gold    bigint,
  PRIMARY KEY (tournament_id, user_id)
);

CREATE TABLE tournament_pairings (
  id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  tournament_id bigint NOT NULL REFERENCES tournaments(id),
  round         int NOT NULL,
  board         int NOT NULL,
  white_id      bigint REFERENCES users(id),       -- NULL = bye pairing
  black_id      bigint REFERENCES users(id),
  game_id       bigint REFERENCES games(id),
  result        text CHECK (result IN ('white','black','draw','bye','forfeit_white','forfeit_black')),
  UNIQUE (tournament_id, round, board)
);
CREATE INDEX tournament_pairings_game_idx ON tournament_pairings (game_id) WHERE game_id IS NOT NULL;

-- =============================================================================
-- Chat, moderation, trust (hustle-moderation)
-- =============================================================================

CREATE TABLE chat_messages (
  id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  game_id    bigint NOT NULL REFERENCES games(id),
  user_id    bigint NOT NULL REFERENCES users(id),
  body       text NOT NULL CHECK (char_length(body) <= 200),
  masked     boolean NOT NULL DEFAULT false,
  redacted   boolean NOT NULL DEFAULT false,       -- moderation takedown (IT Rules)
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX chat_messages_game_idx ON chat_messages (game_id, created_at);
-- 90-day retention job deletes older rows (compliance §5).

CREATE TABLE reports (
  id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  reporter_id    bigint NOT NULL REFERENCES users(id),
  category       text NOT NULL CHECK (category IN ('cheating','abuse','inappropriate_name','other')),
  target_user_id bigint NOT NULL REFERENCES users(id),
  game_id        bigint REFERENCES games(id),
  note           text CHECK (char_length(note) <= 500),
  status         text NOT NULL DEFAULT 'open'
                 CHECK (status IN ('open','reviewing','actioned','dismissed')),
  resolution     jsonb,
  created_at     timestamptz NOT NULL DEFAULT now(),
  resolved_at    timestamptz
);
CREATE INDEX reports_open_idx ON reports (status, created_at) WHERE status IN ('open','reviewing');

CREATE TABLE sanctions (
  id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  user_id    bigint NOT NULL REFERENCES users(id),
  kind       text NOT NULL CHECK (kind IN ('warning','mute','mm_suspension','ad_suspension','suspension')),
  reason     text NOT NULL,
  issued_by  text NOT NULL,
  evidence   jsonb,
  created_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz,                          -- NULL = permanent
  lifted_at  timestamptz
);
CREATE INDEX sanctions_active_user_idx ON sanctions (user_id) WHERE lifted_at IS NULL;

CREATE TABLE anomaly_flags (
  id              bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  kind            text NOT NULL CHECK (kind IN ('engine_match','move_time','pair_farming',
                        'draw_farming','ad_farming','multi_account','rating_anomaly')),
  subject_user_id bigint NOT NULL REFERENCES users(id),
  related_user_id bigint REFERENCES users(id),
  evidence        jsonb NOT NULL,                  -- game ids, scores, windows
  score           real NOT NULL,
  status          text NOT NULL DEFAULT 'open'
                  CHECK (status IN ('open','reviewing','cleared','confirmed')),
  reviewed_by     text,
  reviewed_at     timestamptz,
  created_at      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX anomaly_flags_open_idx ON anomaly_flags (status, created_at) WHERE status IN ('open','reviewing');
CREATE INDEX anomaly_flags_subject_idx ON anomaly_flags (subject_user_id, created_at DESC);

-- =============================================================================
-- Platform (hustle-admin, hustle-analytics, hustle-notifications)
-- =============================================================================

CREATE TABLE app_config (
  key        text PRIMARY KEY,
  value      jsonb NOT NULL,
  updated_by text NOT NULL,
  updated_at timestamptz NOT NULL DEFAULT now(),
  version    int NOT NULL DEFAULT 1                -- optimistic locking on admin writes
);

CREATE TABLE feature_flags (
  key         text PRIMARY KEY,
  enabled     boolean NOT NULL DEFAULT false,
  rollout_pct int  NOT NULL DEFAULT 0 CHECK (rollout_pct BETWEEN 0 AND 100),
  meta        jsonb NOT NULL DEFAULT '{}'
);

CREATE TABLE admin_audit_log (
  id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  actor       text NOT NULL,
  action      text NOT NULL,
  target_type text,
  target_id   text,
  payload     jsonb NOT NULL DEFAULT '{}',
  created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX admin_audit_created_idx ON admin_audit_log (created_at DESC);
CREATE INDEX admin_audit_action_idx ON admin_audit_log (action, created_at DESC);

CREATE TABLE notifications (
  id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  user_id    bigint NOT NULL REFERENCES users(id),
  kind       text NOT NULL,
  payload    jsonb NOT NULL DEFAULT '{}',
  read_at    timestamptz,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX notifications_unread_user_idx ON notifications (user_id, created_at DESC) WHERE read_at IS NULL;

CREATE TABLE data_export_jobs (
  id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  user_id      bigint NOT NULL REFERENCES users(id),
  status       text NOT NULL DEFAULT 'queued' CHECK (status IN ('queued','running','done','failed')),
  object_key   text,                               -- S3-compatible key (7-day signed URL)
  requested_at timestamptz NOT NULL DEFAULT now(),
  completed_at timestamptz
);

CREATE TABLE app_events (                           -- partitioned by day; 90-day retention
  ts      timestamptz NOT NULL,
  user_id bigint,
  name    text NOT NULL,
  payload jsonb NOT NULL DEFAULT '{}'
) PARTITION BY RANGE (ts);
CREATE TABLE app_events_default PARTITION OF app_events DEFAULT;
CREATE INDEX app_events_user_ts_idx ON app_events (user_id, ts);
CREATE INDEX app_events_name_ts_idx ON app_events (name, ts);

-- =============================================================================
-- Seeds (defaults mirror hustle-kernel env defaults & the economy doc)
-- =============================================================================

INSERT INTO app_config (key, value, updated_by) VALUES
  ('economy.welcome_gold',       '50000', 'seed'),
  ('economy.gameplay.win',       '25',    'seed'),
  ('economy.gameplay.draw',      '10',    'seed'),
  ('economy.gameplay.loss',      '5',     'seed'),
  ('economy.gameplay.daily_cap', '500',   'seed'),
  ('economy.daily_streak',       '[500,750,1000,1500,2000,3000,5000]', 'seed'),
  ('ads.enabled',                'false', 'seed'),
  ('ads.gold_per_ad',            '2000',  'seed'),
  ('ads.max_per_day',            '5',     'seed'),
  ('ads.cooldown_s',             '60',    'seed'),
  ('ads.min_account_age_h',      '24',    'seed'),
  ('ads.min_games_lifetime',     '5',     'seed')
ON CONFLICT (key) DO NOTHING;

INSERT INTO feature_flags (key, enabled, rollout_pct) VALUES
  ('ads_experiment',    false, 0),
  ('lag_compensation',  false, 0),
  ('bot_opponents',     false, 0)
ON CONFLICT (key) DO NOTHING;

INSERT INTO cosmetics (id, category, name_key, price_gold, rarity, preview) VALUES
  ('board_classic_green', 'board',  'cosmetic.board_classic_green.name', 5000,  'common', '{}'),
  ('pieces_classic_cream','pieces', 'cosmetic.pieces_classic_cream.name', 5000, 'common', '{}'),
  ('board_metro_map',     'board',  'cosmetic.board_metro_map.name',     8000,  'rare',   '{}'),
  ('board_chaturanga',    'board',  'cosmetic.board_chaturanga.name',    30000, 'epic',   '{}')
ON CONFLICT (id) DO NOTHING;
