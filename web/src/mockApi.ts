import type { Plugin } from 'vite';

export function mockApiPlugin(): Plugin {
  return {
    name: 'chess-hustle-mock-api',
    configureServer(server) {
      server.middlewares.use((req: any, res: any, next: any) => {
        const url: string = req.url || '';
        const method: string = req.method || 'GET';
        if (!url.startsWith('/v1')) {
          return next();
        }

        res.setHeader('Content-Type', 'application/json');

        // Helper to send json
        const sendJson = (status: number, data: unknown) => {
          res.statusCode = status;
          res.end(JSON.stringify(data));
        };

        // 1. Auth endpoints
        if (url === '/v1/auth/otp/send' && method === 'POST') {
          return sendJson(200, { status: 'ok', message: 'OTP sent to mobile' });
        }
        if (url === '/v1/auth/otp/verify' && method === 'POST') {
          return sendJson(200, {
            access_token: 'mock_jwt_token_' + Date.now(),
            refresh_token: 'mock_refresh_' + Date.now(),
            expires_in: 86400,
            user: {
              id: 'usr_hustler_1',
              handle: 'hustler_pro',
              display_name: 'Hustler Pro',
              country: 'IN',
              games_count: 14,
              wins: 9,
              losses: 4,
              draws: 1,
              ratings: { bullet: 1320, blitz: 1410, rapid: 1385, classical: 1200 },
              provisional: false,
              gold: 50000,
              settings: {
                chat_enabled: true,
                sound_enabled: true,
                premove_enabled: true,
                quiet_hours_start: 22,
                quiet_hours_end: 7,
                marketing_opt_in: true,
              },
            },
          });
        }
        if (url === '/v1/me' && method === 'GET') {
          return sendJson(200, {
            id: 'usr_hustler_1',
            handle: 'hustler_pro',
            display_name: 'Hustler Pro',
            country: 'IN',
            games_count: 14,
            wins: 9,
            losses: 4,
            draws: 1,
            ratings: { bullet: 1320, blitz: 1410, rapid: 1385, classical: 1200 },
            provisional: false,
            gold: 50000,
            settings: {
              chat_enabled: true,
              sound_enabled: true,
              premove_enabled: true,
              quiet_hours_start: 22,
              quiet_hours_end: 7,
              marketing_opt_in: true,
            },
          });
        }

        // 2. Daily Streak
        if (url === '/v1/me/daily-streak') {
          return sendJson(200, {
            streak_days: 4,
            today_claimed: false,
            reward_gold: 150,
            next_reset_ist: new Date(Date.now() + 18 * 3600 * 1000).toISOString(),
            ladder: [
              { day: 1, gold: 50, claimed: true },
              { day: 2, gold: 75, claimed: true },
              { day: 3, gold: 100, claimed: true },
              { day: 4, gold: 150, claimed: false },
              { day: 5, gold: 200, claimed: false },
              { day: 6, gold: 300, claimed: false },
              { day: 7, gold: 500, claimed: false },
            ],
          });
        }
        if (url === '/v1/me/daily-streak/claim' && method === 'POST') {
          return sendJson(200, {
            claimed: true,
            reward_gold: 150,
            new_balance: 50150,
            streak_days: 4,
          });
        }

        // 3. Tournaments v1
        if (url === '/v1/tournaments' || url.startsWith('/v1/tournaments?')) {
          return sendJson(200, [
            {
              id: 1,
              public_id: 'tn_mumbai_blitz_open',
              name: 'Mumbai Blitz Championship',
              tc: { initial_s: 180, increment_s: 2, pool: 'blitz', label: '3+2' },
              rounds: 5,
              round_minutes: 15,
              status: 'registration',
              starts_at: new Date(Date.now() + 25 * 60 * 1000).toISOString(),
              registration_closes_at: new Date(Date.now() + 20 * 60 * 1000).toISOString(),
              capacity: 32,
              prize_config: { pool_gold: 2500, distribution: { '1': 1250, '2': 750, '3': 500 } },
              created_by: 1,
            },
            {
              id: 2,
              public_id: 'tn_delhi_masters',
              name: 'Delhi Rapid Masters',
              tc: { initial_s: 600, increment_s: 0, pool: 'rapid', label: '10+0' },
              rounds: 4,
              round_minutes: 25,
              status: 'active',
              starts_at: new Date(Date.now() - 30 * 60 * 1000).toISOString(),
              registration_closes_at: new Date(Date.now() - 35 * 60 * 1000).toISOString(),
              capacity: 16,
              prize_config: { pool_gold: 1500, distribution: { '1': 750, '2': 450, '3': 300 } },
              created_by: 1,
            },
            {
              id: 3,
              public_id: 'tn_bullet_frenzy',
              name: 'Midnight Bullet Arena',
              tc: { initial_s: 60, increment_s: 0, pool: 'bullet', label: '1+0' },
              rounds: 7,
              round_minutes: 10,
              status: 'completed',
              starts_at: new Date(Date.now() - 120 * 60 * 1000).toISOString(),
              registration_closes_at: new Date(Date.now() - 125 * 60 * 1000).toISOString(),
              capacity: 64,
              prize_config: { pool_gold: 5000, distribution: { '1': 2500, '2': 1500, '3': 1000 } },
              created_by: 1,
            },
          ]);
        }
        if (url.startsWith('/v1/tournaments/') && !url.includes('/join') && !url.includes('/withdraw')) {
          const publicId = url.replace('/v1/tournaments/', '').split('?')[0];
          return sendJson(200, {
            tournament: {
              id: 1,
              public_id: publicId,
              name: publicId === 'tn_mumbai_blitz_open' ? 'Mumbai Blitz Championship' : 'Swiss Chess Open',
              tc: { initial_s: 180, increment_s: 2, pool: 'blitz', label: '3+2' },
              rounds: 5,
              round_minutes: 15,
              status: 'registration',
              starts_at: new Date(Date.now() + 25 * 60 * 1000).toISOString(),
              registration_closes_at: new Date(Date.now() + 20 * 60 * 1000).toISOString(),
              capacity: 32,
              prize_config: { pool_gold: 2500, distribution: { '1': 1250, '2': 750, '3': 500 } },
              created_by: 1,
            },
            entries_count: 18,
            my_entry: null,
            standings: [
              { rank: 1, user_id: 101, handle: 'grandmaster_vikram', points: 3.0, buchholz: 6.5, sonneborn_berger: 6.0, games_played: 3, wins: 3, draws: 0, losses: 0 },
              { rank: 2, user_id: 102, handle: 'queen_gambit_shreya', points: 2.5, buchholz: 5.5, sonneborn_berger: 4.5, games_played: 3, wins: 2, draws: 1, losses: 0 },
              { rank: 3, user_id: 103, handle: 'blitz_shark_rohit', points: 2.0, buchholz: 6.0, sonneborn_berger: 3.5, games_played: 3, wins: 2, draws: 0, losses: 1 },
              { rank: 4, user_id: 104, handle: 'hustler_pro', points: 2.0, buchholz: 5.0, sonneborn_berger: 3.0, games_played: 3, wins: 2, draws: 0, losses: 1 },
              { rank: 5, user_id: 105, handle: 'pawn_storm_arjun', points: 1.5, buchholz: 4.5, sonneborn_berger: 2.0, games_played: 3, wins: 1, draws: 1, losses: 1 },
            ],
            pairings: [
              { id: 11, tournament_id: 1, round: 1, board: 1, white_id: 101, black_id: 104, game_id: 1001, result: '1 - 0' },
              { id: 12, tournament_id: 1, round: 1, board: 2, white_id: 102, black_id: 103, game_id: 1002, result: '1 - 0' },
              { id: 13, tournament_id: 1, round: 2, board: 1, white_id: 101, black_id: 102, game_id: 1003, result: '½ - ½' },
              { id: 14, tournament_id: 1, round: 2, board: 2, white_id: 104, black_id: 105, game_id: 1004, result: '1 - 0' },
              { id: 15, tournament_id: 1, round: 3, board: 1, white_id: 103, black_id: 101, game_id: 1005, result: '0 - 1' },
              { id: 16, tournament_id: 1, round: 3, board: 2, white_id: 102, black_id: 104, game_id: 1006, result: '1 - 0' },
            ],
          });
        }
        if (url.includes('/join') && method === 'POST') {
          return sendJson(200, {
            tournament_id: 1,
            user_id: 100,
            joined_at: new Date().toISOString(),
            withdrawn: false,
          });
        }
        if (url.includes('/withdraw') && method === 'POST') {
          res.statusCode = 204;
          return res.end();
        }

        // 4. Rewarded Ads (ADR-007 / ADR-011)
        if (url === '/v1/ads/status') {
          return sendJson(200, {
            enabled: true,
            gold_per_ad: 100,
            max_per_day: 5,
            used_today: 2,
            cooldown_until: null,
            eligibility: {
              eligible: true,
              reason: null,
              account_age_ok: true,
              games_completed_ok: true,
              no_active_game: true,
              not_sanctioned: true,
              age_ok: true,
            },
          });
        }
        if (url === '/v1/ads/session' && method === 'POST') {
          return sendJson(200, {
            session_id: 'as_01HX_DEV_MOCK',
            provider: 'admob',
            reward_gold: 100,
            remaining_today: 3,
            cooldown_s: 0,
          });
        }

        // 5. Cosmetics & Loadout
        if (url === '/v1/cosmetics' || url.startsWith('/v1/cosmetics?')) {
          return sendJson(200, [
            { id: 'board_wood_classic', category: 'board', name_key: 'Staunton Classical Wood', price_gold: 0, rarity: 'common', preview: {}, active: true, owned: true },
            { id: 'board_obsidian', category: 'board', name_key: 'Dark Obsidian Arena', price_gold: 500, rarity: 'rare', preview: {}, active: true, owned: true },
            { id: 'board_emerald', category: 'board', name_key: 'Emerald Vinyl Club', price_gold: 800, rarity: 'epic', preview: {}, active: true, owned: false },
            { id: 'board_royal_gold', category: 'board', name_key: 'Amber Royal Gold', price_gold: 2500, rarity: 'legendary', preview: {}, active: true, owned: false },
            { id: 'pieces_merida', category: 'pieces', name_key: 'Merida Classic Pieces', price_gold: 0, rarity: 'common', preview: {}, active: true, owned: true },
            { id: 'pieces_staunty', category: 'pieces', name_key: 'Staunty Modern Set', price_gold: 400, rarity: 'rare', preview: {}, active: true, owned: true },
            { id: 'pieces_cburnett', category: 'pieces', name_key: 'CBurnett Standard Set', price_gold: 600, rarity: 'epic', preview: {}, active: true, owned: true },
          ]);
        }
        if (url === '/v1/me/loadout') {
          return sendJson(200, {
            board_id: 'board_wood_classic',
            pieces_id: 'pieces_merida',
            avatar_id: null,
            frame_id: null,
          });
        }

        // 6. Missions & Achievements
        if (url === '/v1/me/missions') {
          return sendJson(200, [
            { id: 'daily_games_1', kind: 'daily', title_key: 'daily_games_1', goal_type: 'play_games', progress: 1, goal: 1, claimed: false, reward_gold: 50, period_key: 'daily' },
            { id: 'daily_games_3', kind: 'daily', title_key: 'daily_games_3', goal_type: 'play_games', progress: 2, goal: 3, claimed: false, reward_gold: 100, period_key: 'daily' },
            { id: 'daily_win_1', kind: 'daily', title_key: 'daily_win_1', goal_type: 'win_games', progress: 1, goal: 1, claimed: false, reward_gold: 75, period_key: 'daily' },
            { id: 'daily_checkmate_1', kind: 'daily', title_key: 'daily_checkmate_1', goal_type: 'deliver_checkmate', progress: 1, goal: 1, claimed: true, reward_gold: 100, period_key: 'daily' },
            { id: 'weekly_games_15', kind: 'weekly', title_key: 'weekly_games_15', goal_type: 'play_games', progress: 8, goal: 15, claimed: false, reward_gold: 250, period_key: 'weekly' },
            { id: 'weekly_wins_7', kind: 'weekly', title_key: 'weekly_wins_7', goal_type: 'win_games', progress: 5, goal: 7, claimed: false, reward_gold: 300, period_key: 'weekly' },
          ]);
        }
        if (url.includes('/claim') && url.includes('/missions/')) {
          return sendJson(200, {
            claimed: true,
            amount: 75,
            new_balance: 50075,
          });
        }
        if (url === '/v1/me/achievements') {
          return sendJson(200, [
            { id: 'achieve_first_win', title_key: 'achieve_first_win', goal_type: 'first_win', progress: 1, target: 1, reward_gold: 100, unlocked: true },
            { id: 'achieve_games_10', title_key: 'achieve_games_10', goal_type: 'games_count', progress: 14, target: 10, reward_gold: 150, unlocked: true },
            { id: 'achieve_games_50', title_key: 'achieve_games_50', goal_type: 'games_count', progress: 14, target: 50, reward_gold: 500, unlocked: false },
            { id: 'achieve_checkmate_5', title_key: 'achieve_checkmate_5', goal_type: 'checkmates', progress: 3, target: 5, reward_gold: 250, unlocked: false },
          ]);
        }

        // 7. Friends & Social
        if (url === '/v1/friends') {
          return sendJson(200, [
            { user_id: 201, handle: 'grandmaster_vikram', display_name: 'GM Vikram', online: true, in_game: false, rating: 1450 },
            { user_id: 202, handle: 'chess_queen_anjali', display_name: 'Anjali R', online: true, in_game: true, rating: 1380 },
            { user_id: 203, handle: 'blitz_king_arjun', display_name: 'Arjun K', online: false, in_game: false, rating: 1290 },
          ]);
        }
        if (url === '/v1/friends/requests') {
          return sendJson(200, { incoming: [], outgoing: [] });
        }
        if (url === '/v1/blocks') {
          return sendJson(200, []);
        }

        // 8. Matchmaking & Realtime ticket
        if (url === '/v1/matchmaking/queue' && method === 'POST') {
          return sendJson(200, { queued: true, est_wait_s: 3 });
        }
        if (url === '/v1/realtime/ticket') {
          return sendJson(200, {
            ticket: 'dev_mock_ticket',
            ws_url: '/ws',
            expires_in: 3600,
          });
        }

        // Fallback for any unhandled /v1 route
        return sendJson(404, { error: { code: 'not_found', message: `Route ${url} not found` } });
      });
    },
  };
}
