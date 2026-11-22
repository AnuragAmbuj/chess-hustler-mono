export type PoolType = 'bullet' | 'blitz' | 'rapid' | 'classical';

export interface TimeControlConfig {
  initial_s: number;
  increment_s: number;
  pool: PoolType;
  label: string;
}

export interface UserRatings {
  bullet: number;
  blitz: number;
  rapid: number;
  classical: number;
}

export interface UserSettings {
  chat_enabled: boolean;
  sound_enabled: boolean;
  premove_enabled: boolean;
  quiet_hours_start: number;
  quiet_hours_end: number;
  marketing_opt_in: boolean;
}

export interface UserProfile {
  id: string;
  handle: string;
  display_name: string;
  bio?: string;
  country?: string;
  avatar_cosmetic_id?: string;
  games_count: number;
  wins: number;
  losses: number;
  draws: number;
  ratings: UserRatings;
  provisional: boolean;
  gold: number;
  settings?: UserSettings;
}

export interface DpdpConsents {
  essential: boolean;
  functional: boolean;
  marketing: boolean;
  personalised_ads: boolean;
  chat?: boolean;
}

export interface AuthTokens {
  access_token: string;
  refresh_token: string;
  expires_in: number;
  user: UserProfile;
}

export interface MoveRecord {
  ply: number;
  uci: string;
  san: string;
  ts?: string;
}

export interface GameClocks {
  white_ms: number;
  black_ms: number;
  server_ts: string;
}

export interface GamePlayerInfo {
  handle: string;
  rating: number;
  provisional?: boolean;
}

export interface RatingAdjustment {
  before: number;
  after: number;
  delta: number;
}

export interface GameResultPayload {
  result: 'white' | 'black' | 'draw';
  termination: 'checkmate' | 'resignation' | 'timeout' | 'stalemate' | 'agreement' | 'abort' | '50-move' | 'threefold' | string;
  ratings?: {
    white: RatingAdjustment;
    black: RatingAdjustment;
  };
  gold?: {
    white_earned?: number;
    black_earned?: number;
    earned?: number;
    balance?: number;
    reason?: string;
  };
}

export interface GameSnapshot {
  id: string;
  rated: boolean;
  pool: PoolType;
  tc: {
    initial_s: number;
    increment_s: number;
  };
  white: GamePlayerInfo;
  black: GamePlayerInfo;
  status: 'active' | 'finished' | 'aborted';
  fen: string;
  ply: number;
  version: number;
  clocks: GameClocks;
  moves: MoveRecord[];
  stream_id?: string;
  draw_offer?: 'white' | 'black' | null;
  result?: 'white' | 'black' | 'draw' | null;
  termination?: string | null;
}

export interface DailyStreakInfo {
  available: boolean;
  streak: number;
  next_reset_ist: string;
  next_reward: number;
}

export interface DailyClaimResult {
  claimed: boolean;
  streak: number;
  gold_granted: number;
  balance: number;
}

export interface RealtimeEnvelope<T = any> {
  id: string;
  ts: string;
  ch: string;
  type: string;
  data: T;
}

export interface MatchmakingMatchedEvent {
  game_id: string;
  color: 'white' | 'black';
  rated: boolean;
  tc: {
    initial_s: number;
    increment_s: number;
  };
  opponent: {
    handle: string;
    rating: number;
    provisional?: boolean;
  };
}

export interface FriendPresence {
  online: boolean;
  status: 'idle' | 'playing' | 'offline' | string;
}

export interface FriendInfo {
  id: number;
  user_id: string;
  handle: string;
  display_name: string;
  avatar_cosmetic_id?: string | null;
  rating: number;
  presence: FriendPresence;
  accepted_at?: string | null;
}

export interface FriendRequestItem {
  id: number;
  handle: string;
  display_name: string;
  rating: number;
  avatar_cosmetic_id?: string | null;
  created_at: string;
}

export interface FriendRequestsResponse {
  incoming: FriendRequestItem[];
  outgoing: FriendRequestItem[];
}

export interface BlockedUserItem {
  handle: string;
  display_name: string;
  blocked_at: string;
}

export interface DirectChallenge {
  id: string;
  challenger_handle: string;
  challenger_rating: number;
  target_handle: string;
  initial_s: number;
  increment_s: number;
  rated: boolean;
  created_at: string;
  expires_at: string;
}

export interface ChatMessageItem {
  id: number;
  game_id: string;
  user_id: string;
  handle: string;
  body: string;
  masked: boolean;
  created_at: string;
}

export interface CreateReportRequest {
  category: 'cheating' | 'abuse' | 'inappropriate_name' | 'other' | string;
  target_handle: string;
  game_id?: string;
  note?: string;
}

export interface ReportItem {
  id: number;
  reporter_id: string;
  category: string;
  target_handle: string;
  game_id?: string;
  note?: string;
  status: string;
  created_at: string;
}

export interface GrievancePolicyLink {
  name: string;
  url: string;
}

export interface GrievanceInfo {
  officer_name: string;
  officer_title: string;
  officer_email: string;
  address: string;
  acknowledgment_sla_hours: number;
  disposition_sla_days: number;
  governing_rule: string;
  policies: GrievancePolicyLink[];
}

export type CosmeticCategory = 'board' | 'pieces' | 'avatar' | 'frame';
export type CosmeticRarity = 'common' | 'rare' | 'epic' | 'legendary';

export interface CosmeticItem {
  id: string;
  category: CosmeticCategory;
  name_key: string;
  price_gold: number;
  rarity: CosmeticRarity;
  preview: Record<string, unknown>;
  available_from?: string;
  available_to?: string;
  active: boolean;
  owned: boolean;
}

export interface UserLoadout {
  board_id?: string | null;
  pieces_id?: string | null;
  avatar_id?: string | null;
  frame_id?: string | null;
}

export interface PurchaseResult {
  owned: boolean;
  balance: number;
  ledger: {
    reason: string;
    delta: number;
  };
  replayed?: boolean;
}

export interface MissionItem {
  id: string;
  kind: 'daily' | 'weekly';
  title_key: string;
  goal_type: string;
  progress: number;
  goal: number;
  claimed: boolean;
  reward_gold: number;
  period_key: string;
}

export interface ClaimMissionResult {
  claimed: boolean;
  amount: number;
  new_balance: number;
}

export interface AchievementItem {
  id: string;
  title_key: string;
  goal_type: string;
  progress: number;
  target: number;
  reward_gold: number;
  unlocked: boolean;
  granted_at?: string;
}

// ==========================================
// Tournaments v1 (Swiss System Engine)
// ==========================================

export interface Tournament {
  id: number;
  public_id: string;
  name: string;
  tc: {
    initial_s: number;
    increment_s: number;
    pool?: string;
    label?: string;
  };
  rounds: number;
  round_minutes: number;
  status: string;
  starts_at: string;
  registration_closes_at: string;
  capacity: number;
  prize_config: {
    pool_gold?: number;
    distribution?: Record<string, number>;
  };
  created_by: number;
}

export interface TournamentEntry {
  tournament_id: number;
  user_id: number;
  joined_at: string;
  withdrawn: boolean;
  final_rank?: number | null;
  prize_gold?: number | null;
}

export interface TournamentPairing {
  id: number;
  tournament_id: number;
  round: number;
  board: number;
  white_id?: number | null;
  black_id?: number | null;
  game_id?: number | null;
  result?: string | null;
}

export interface StandingItem {
  rank: number;
  user_id: number;
  handle: string;
  points: number;
  buchholz: number;
  sonneborn_berger: number;
  games_played: number;
  wins: number;
  draws: number;
  losses: number;
}

export interface TournamentDetail {
  tournament: Tournament;
  entries_count: number;
  my_entry?: TournamentEntry | null;
  standings: StandingItem[];
  pairings: TournamentPairing[];
}

export interface CreateTournamentRequest {
  name: string;
  tc: {
    initial_s: number;
    increment_s: number;
  };
  rounds: number;
  round_minutes: number;
  starts_at: string;
  registration_closes_at: string;
  capacity: number;
  prize_config?: {
    pool_gold?: number;
    distribution?: Record<string, number>;
  };
}

// ==========================================
// Rewarded Ads (ADR-007 / ADR-011)
// ==========================================

export interface AdEligibility {
  eligible: boolean;
  reason?: string | null;
  account_age_ok: boolean;
  games_completed_ok: boolean;
  no_active_game: boolean;
  not_sanctioned: boolean;
  age_ok: boolean;
}

export interface AdStatus {
  enabled: boolean;
  gold_per_ad: number;
  max_per_day: number;
  used_today: number;
  cooldown_until?: string | null;
  eligibility: AdEligibility;
}

export interface AdSessionResponse {
  session_id: string;
  provider: string;
  reward_gold: number;
  remaining_today: number;
  cooldown_s: number;
}

// ==========================================
// Leaderboards (RA-3 / PRD G3)
// ==========================================

export interface LeaderboardEntry {
  rank: number;
  user_id: string;
  handle: string;
  display_name: string;
  rating: number;
  rd: number;
  peak_rating: number;
  games_count: number;
  win_count: number;
  loss_count: number;
  draw_count: number;
}

export interface LeaderboardResponse {
  pool: 'bullet' | 'blitz' | 'rapid' | 'classical';
  updated_at: string;
  entries: LeaderboardEntry[];
}

// ==========================================
// Daily Tactical Puzzles (Phase 5)
// ==========================================

export interface TacticalPuzzle {
  id: string;
  title: string;
  theme: string;
  difficulty: 'Novice' | 'Intermediate' | 'Master';
  fen: string;
  playerColor: 'white' | 'black';
  moves: string[]; // SAN moves e.g. ["Qxf7#"] or ["Nf7+", "Kg8", "Nh6#"]
  hint: string;
  rewardGold: number;
  solved?: boolean;
}

// ==========================================
// Live Spectate (Phase 5)
// ==========================================

export interface LiveGameSummary {
  id: string;
  white: { handle: string; rating: number };
  black: { handle: string; rating: number };
  pool: 'bullet' | 'blitz' | 'rapid' | 'classical';
  tc: { initial_s: number; increment_s: number };
  ply: number;
  started_at: string;
  spectators: number;
}




