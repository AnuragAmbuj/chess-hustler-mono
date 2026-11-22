import {
  AchievementItem,
  AdSessionResponse,
  AdStatus,
  AuthTokens,
  BlockedUserItem,
  ChatMessageItem,
  ClaimMissionResult,
  CosmeticItem,
  CreateReportRequest,
  CreateTournamentRequest,
  DailyClaimResult,
  DailyStreakInfo,
  DirectChallenge,
  DpdpConsents,
  FriendInfo,
  FriendRequestItem,
  FriendRequestsResponse,
  GameResultPayload,
  GameSnapshot,
  GrievanceInfo,
  MissionItem,
  PurchaseResult,
  ReportItem,
  Tournament,
  TournamentDetail,
  TournamentEntry,
  UserLoadout,
  UserProfile,
} from '../types';

const API_BASE = '/v1';

class ApiClient {
  private token: string | null = null;

  constructor() {
    this.token = localStorage.getItem('hustle_access_token');
  }

  setToken(token: string | null) {
    this.token = token;
    if (token) {
      localStorage.setItem('hustle_access_token', token);
    } else {
      localStorage.removeItem('hustle_access_token');
    }
  }

  getToken(): string | null {
    return this.token;
  }

  private headers(extra: Record<string, string> = {}): Record<string, string> {
    const h: Record<string, string> = {
      'Content-Type': 'application/json',
      ...extra,
    };
    if (this.token) {
      h['Authorization'] = `Bearer ${this.token}`;
    }
    return h;
  }

  // --- Auth ---
  async sendOtp(phone: string): Promise<boolean> {
    const res = await fetch(`${API_BASE}/auth/otp/send`, {
      method: 'POST',
      headers: this.headers(),
      body: JSON.stringify({ phone }),
    });
    if (!res.ok && res.status !== 204) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to send OTP' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return true;
  }

  async verifyOtp(params: {
    phone: string;
    code: string;
    age_confirmed: boolean;
    consents: DpdpConsents;
    profile: { handle: string; display_name: string };
  }): Promise<AuthTokens> {
    const res = await fetch(`${API_BASE}/auth/otp/verify`, {
      method: 'POST',
      headers: this.headers(),
      body: JSON.stringify(params),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Verification failed' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    const data = (await res.json()) as AuthTokens;
    if (data.access_token) {
      this.setToken(data.access_token);
    }
    return data;
  }

  async logout(): Promise<void> {
    try {
      await fetch(`${API_BASE}/auth/logout`, {
        method: 'POST',
        headers: this.headers(),
      });
    } finally {
      this.setToken(null);
    }
  }

  // --- Me & Profile ---
  async getMe(): Promise<UserProfile> {
    const res = await fetch(`${API_BASE}/me`, {
      headers: this.headers(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to fetch profile' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json() as Promise<UserProfile>;
  }

  // --- Daily Streak & Economy ---
  async getDailyStreak(): Promise<DailyStreakInfo> {
    const res = await fetch(`${API_BASE}/me/daily`, {
      headers: this.headers(),
    });
    if (!res.ok) {
      // Default fallback
      return {
        available: true,
        streak: 1,
        next_reset_ist: '00:00 IST',
        next_reward: 100,
      };
    }
    return res.json() as Promise<DailyStreakInfo>;
  }

  async claimDailyStreak(): Promise<DailyClaimResult> {
    const res = await fetch(`${API_BASE}/me/daily/claim`, {
      method: 'POST',
      headers: this.headers({
        'Idempotency-Key': `daily_${Date.now()}`,
      }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Claim failed' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    const data = await res.json();
    return {
      claimed: true,
      streak: data.streak || 1,
      gold_granted: data.gold_granted || data.amount || 100,
      balance: data.balance || data.new_balance || 50100,
    };
  }

  // --- Matchmaking ---
  async joinQueue(tc: { initial_s: number; increment_s: number }, rated: boolean = true) {
    const res = await fetch(`${API_BASE}/matchmaking/queue`, {
      method: 'POST',
      headers: this.headers({
        'Idempotency-Key': `queue_${Date.now()}`,
      }),
      body: JSON.stringify({ tc, rated }),
    });
    if (!res.ok && res.status !== 202) {
      const err = await res.json().catch(() => ({ error: { message: 'Queue join failed' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json().catch(() => ({ pool: 'blitz', est_wait_s: 5 }));
  }

  async leaveQueue() {
    const res = await fetch(`${API_BASE}/matchmaking/queue`, {
      method: 'DELETE',
      headers: this.headers(),
    });
    return res.ok || res.status === 204;
  }

  async getQueueStatus(): Promise<{ queued: boolean; pool?: string; waited_s?: number }> {
    const res = await fetch(`${API_BASE}/matchmaking/status`, {
      headers: this.headers(),
    });
    if (!res.ok) return { queued: false };
    return res.json();
  }

  // --- Realtime Ticket ---
  async getRealtimeTicket(): Promise<{ ticket: string; ws_url: string; expires_in: number }> {
    const res = await fetch(`${API_BASE}/realtime/ticket`, {
      headers: this.headers(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Ticket request failed' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json();
  }

  // --- Games ---
  async getGame(id: string): Promise<GameSnapshot> {
    const res = await fetch(`${API_BASE}/games/${id}`, {
      headers: this.headers(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Game snapshot failed' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json() as Promise<GameSnapshot>;
  }

  async resignGame(id: string): Promise<GameResultPayload> {
    const res = await fetch(`${API_BASE}/games/${id}/resign`, {
      method: 'POST',
      headers: this.headers({
        'Idempotency-Key': `resign_${id}_${Date.now()}`,
      }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Resign failed' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json() as Promise<GameResultPayload>;
  }

  async abortGame(id: string): Promise<boolean> {
    const res = await fetch(`${API_BASE}/games/${id}/abort`, {
      method: 'POST',
      headers: this.headers({
        'Idempotency-Key': `abort_${id}_${Date.now()}`,
      }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Abort failed' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return true;
  }

  async offerDraw(id: string): Promise<boolean> {
    const res = await fetch(`${API_BASE}/games/${id}/draw-offer`, {
      method: 'POST',
      headers: this.headers(),
    });
    return res.ok;
  }

  async claimTimeout(id: string): Promise<GameResultPayload> {
    const res = await fetch(`${API_BASE}/games/${id}/claim-timeout`, {
      method: 'POST',
      headers: this.headers({
        'Idempotency-Key': `claim_timeout_${id}_${Date.now()}`,
      }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Claim timeout failed' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json() as Promise<GameResultPayload>;
  }

  // --- Friends & Challenges ---
  async getFriends(): Promise<FriendInfo[]> {
    const res = await fetch(`${API_BASE}/friends`, {
      headers: this.headers(),
    });
    if (!res.ok) return [];
    return res.json();
  }

  async getFriendRequests(): Promise<FriendRequestsResponse> {
    const res = await fetch(`${API_BASE}/friends/requests`, {
      headers: this.headers(),
    });
    if (!res.ok) return { incoming: [], outgoing: [] };
    return res.json();
  }

  async sendFriendRequest(handle: string): Promise<FriendRequestItem> {
    const res = await fetch(`${API_BASE}/friends/requests`, {
      method: 'POST',
      headers: this.headers(),
      body: JSON.stringify({ handle }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Friend request failed' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json();
  }

  async acceptFriendRequest(id: number): Promise<void> {
    const res = await fetch(`${API_BASE}/friends/requests/${id}/accept`, {
      method: 'POST',
      headers: this.headers(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to accept' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
  }

  async declineFriendRequest(id: number): Promise<void> {
    const res = await fetch(`${API_BASE}/friends/requests/${id}/decline`, {
      method: 'POST',
      headers: this.headers(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to decline' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
  }

  async removeFriend(id: number): Promise<void> {
    const res = await fetch(`${API_BASE}/friends/${id}`, {
      method: 'DELETE',
      headers: this.headers(),
    });
    if (!res.ok && res.status !== 204) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to unfriend' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
  }

  async getBlocks(): Promise<BlockedUserItem[]> {
    const res = await fetch(`${API_BASE}/blocks`, {
      headers: this.headers(),
    });
    if (!res.ok) return [];
    return res.json();
  }

  async blockUser(handle: string): Promise<BlockedUserItem> {
    const res = await fetch(`${API_BASE}/blocks`, {
      method: 'POST',
      headers: this.headers(),
      body: JSON.stringify({ handle }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to block' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json();
  }

  async unblockUser(handle: string): Promise<void> {
    const res = await fetch(`${API_BASE}/blocks/${encodeURIComponent(handle)}`, {
      method: 'DELETE',
      headers: this.headers(),
    });
    if (!res.ok && res.status !== 204) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to unblock' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
  }

  async createChallenge(
    handle: string,
    tc: { initial_s: number; increment_s: number },
    rated: boolean = false
  ): Promise<DirectChallenge> {
    const res = await fetch(`${API_BASE}/challenge`, {
      method: 'POST',
      headers: this.headers(),
      body: JSON.stringify({ handle, tc, rated }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Challenge failed' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json();
  }

  async acceptChallenge(id: string): Promise<{ game_id: string }> {
    const res = await fetch(`${API_BASE}/challenge/${encodeURIComponent(id)}/accept`, {
      method: 'POST',
      headers: this.headers(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to accept challenge' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json();
  }

  async declineChallenge(id: string): Promise<void> {
    const res = await fetch(`${API_BASE}/challenge/${encodeURIComponent(id)}/decline`, {
      method: 'POST',
      headers: this.headers(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to decline challenge' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
  }

  // --- Moderation & In-Game Chat ---
  async getGameChat(gameId: string): Promise<ChatMessageItem[]> {
    const res = await fetch(`${API_BASE}/games/${encodeURIComponent(gameId)}/chat`, {
      headers: this.headers(),
    });
    if (!res.ok) return [];
    return res.json();
  }

  async sendGameChat(gameId: string, text: string): Promise<ChatMessageItem> {
    const res = await fetch(`${API_BASE}/games/${encodeURIComponent(gameId)}/chat`, {
      method: 'POST',
      headers: this.headers(),
      body: JSON.stringify({ text }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to send chat' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json();
  }

  async submitReport(report: CreateReportRequest): Promise<ReportItem> {
    const res = await fetch(`${API_BASE}/reports`, {
      method: 'POST',
      headers: this.headers(),
      body: JSON.stringify(report),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Report submission failed' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json();
  }

  async getGrievanceInfo(): Promise<GrievanceInfo> {
    const res = await fetch(`${API_BASE}/grievance`);
    if (!res.ok) {
      throw new Error(`Failed to fetch grievance info: HTTP ${res.status}`);
    }
    return res.json();
  }

  // --- Cosmetics & Customization ---
  async getCosmetics(category?: string): Promise<CosmeticItem[]> {
    const url = category ? `${API_BASE}/cosmetics?category=${encodeURIComponent(category)}` : `${API_BASE}/cosmetics`;
    const res = await fetch(url, { headers: this.headers() });
    if (!res.ok) return [];
    return res.json();
  }

  async getCosmeticItem(id: string): Promise<CosmeticItem> {
    const res = await fetch(`${API_BASE}/cosmetics/${encodeURIComponent(id)}`, {
      headers: this.headers(),
    });
    if (!res.ok) {
      throw new Error(`Failed to fetch cosmetic item: HTTP ${res.status}`);
    }
    return res.json();
  }

  async purchaseCosmetic(id: string, idemKey?: string): Promise<PurchaseResult> {
    const extraHeaders: Record<string, string> = {};
    if (idemKey) {
      extraHeaders['Idempotency-Key'] = idemKey;
    }
    const res = await fetch(`${API_BASE}/cosmetics/${encodeURIComponent(id)}/purchase`, {
      method: 'POST',
      headers: this.headers(extraHeaders),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Purchase failed' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json();
  }

  async getLoadout(): Promise<UserLoadout> {
    const res = await fetch(`${API_BASE}/me/loadout`, {
      headers: this.headers(),
    });
    if (!res.ok) return {};
    return res.json();
  }

  async updateLoadout(req: {
    board_id?: string | null;
    pieces_id?: string | null;
    avatar_id?: string | null;
    frame_id?: string | null;
  }): Promise<UserLoadout> {
    const res = await fetch(`${API_BASE}/me/loadout`, {
      method: 'PUT',
      headers: this.headers(),
      body: JSON.stringify(req),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to update loadout' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json();
  }

  // --- Missions & Achievements ---
  async getMissions(): Promise<MissionItem[]> {
    const res = await fetch(`${API_BASE}/me/missions`, {
      headers: this.headers(),
    });
    if (!res.ok) return [];
    return res.json();
  }

  async claimMission(id: string): Promise<ClaimMissionResult> {
    const res = await fetch(`${API_BASE}/me/missions/${encodeURIComponent(id)}/claim`, {
      method: 'POST',
      headers: this.headers(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to claim mission' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json();
  }

  async getAchievements(): Promise<AchievementItem[]> {
    const res = await fetch(`${API_BASE}/me/achievements`, {
      headers: this.headers(),
    });
    if (!res.ok) return [];
    return res.json();
  }

  // ==========================================
  // Tournaments v1 (Swiss System Engine)
  // ==========================================

  async getTournaments(params?: {
    status?: string;
    limit?: number;
    offset?: number;
  }): Promise<Tournament[]> {
    const query = new URLSearchParams();
    if (params?.status) query.set('status', params.status);
    if (params?.limit) query.set('limit', String(params.limit));
    if (params?.offset) query.set('offset', String(params.offset));

    const qs = query.toString();
    const url = `${API_BASE}/tournaments${qs ? `?${qs}` : ''}`;
    const res = await fetch(url, { headers: this.headers() });
    if (!res.ok) return [];
    return res.json();
  }

  async getTournament(id: string): Promise<TournamentDetail> {
    const res = await fetch(`${API_BASE}/tournaments/${encodeURIComponent(id)}`, {
      headers: this.headers(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Tournament not found' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json();
  }

  async joinTournament(id: string): Promise<TournamentEntry> {
    const res = await fetch(`${API_BASE}/tournaments/${encodeURIComponent(id)}/join`, {
      method: 'POST',
      headers: this.headers(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to join tournament' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json();
  }

  async withdrawTournament(id: string): Promise<void> {
    const res = await fetch(`${API_BASE}/tournaments/${encodeURIComponent(id)}/withdraw`, {
      method: 'POST',
      headers: this.headers(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to withdraw from tournament' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
  }

  async createTournament(req: CreateTournamentRequest): Promise<Tournament> {
    const res = await fetch(`${API_BASE}/admin/tournaments`, {
      method: 'POST',
      headers: this.headers(),
      body: JSON.stringify(req),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to create tournament' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json();
  }

  // ==========================================
  // Rewarded Ads (ADR-007 / ADR-011)
  // ==========================================

  async getAdStatus(): Promise<AdStatus> {
    const res = await fetch(`${API_BASE}/ads/status`, {
      headers: this.headers(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to get ad status' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json();
  }

  async createAdSession(): Promise<AdSessionResponse> {
    const res = await fetch(`${API_BASE}/ads/session`, {
      method: 'POST',
      headers: this.headers(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: { message: 'Failed to initiate ad session' } }));
      throw new Error(err?.error?.message || `HTTP ${res.status}`);
    }
    return res.json();
  }
}

export const api = new ApiClient();
