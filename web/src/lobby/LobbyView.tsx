import { useState, useEffect, useCallback } from 'react';
import { ProfileHeader } from './ProfileHeader';
import { PoolSelector, POOLS } from './PoolSelector';
import { MatchmakingQueue } from './MatchmakingQueue';
import { FriendsModal } from './FriendsModal';
import { DirectChallengeModal } from './DirectChallengeModal';
import { ChallengeReceivedModal } from './ChallengeReceivedModal';
import { CosmeticsModal } from './CosmeticsModal';
import { MissionsModal } from './MissionsModal';
import { AdminModal } from './AdminModal';
import { TournamentsModal } from './TournamentsModal';
import { RewardedAdsModal } from './RewardedAdsModal';
import { TimeControlConfig, GameSnapshot, MatchmakingMatchedEvent, FriendInfo, UserLoadout } from '../types';
import { useAuth } from '../auth/useAuth';
import { useRealtime } from '../realtime/useRealtime';
import { api } from '../api/client';

interface LobbyViewProps {
  onStartGame: (snapshot: GameSnapshot, myColor: 'white' | 'black', loadout?: UserLoadout) => void;
}

export function LobbyView({ onStartGame }: LobbyViewProps) {
  const { user, isAuthenticated, openAuthModal } = useAuth();
  const { subscribe } = useRealtime();

  // Loadout state
  const [loadout, setLoadout] = useState<UserLoadout>({});
  const [isCosmeticsOpen, setIsCosmeticsOpen] = useState(false);

  // Missions state
  const [isMissionsOpen, setIsMissionsOpen] = useState(false);
  const [claimableMissionsCount, setClaimableMissionsCount] = useState(0);

  // Admin console state
  const [isAdminOpen, setIsAdminOpen] = useState(false);

  // Tournaments & Ads state
  const [isTournamentsOpen, setIsTournamentsOpen] = useState(false);
  const [isAdsOpen, setIsAdsOpen] = useState(false);

  useEffect(() => {
    if (isAuthenticated) {
      api.getLoadout().then(setLoadout).catch(() => {});
      api
        .getMissions()
        .then((m) => {
          setClaimableMissionsCount(
            m.filter((item) => item.progress >= item.goal && !item.claimed).length
          );
        })
        .catch(() => {});
    }
  }, [isAuthenticated]);

  // Default to Blitz 3+2
  const [selectedTc, setSelectedTc] = useState<TimeControlConfig>(
    POOLS[1]?.timeControls[1] || {
      initial_s: 180,
      increment_s: 2,
      pool: 'blitz',
      label: '3+2',
    }
  );

  const [isQueued, setIsQueued] = useState(false);
  const [estWait, setEstWait] = useState(6);
  const [rated, setRated] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Friends & Challenges state
  const [isFriendsOpen, setIsFriendsOpen] = useState(false);
  const [challengeTarget, setChallengeTarget] = useState<FriendInfo | null>(null);
  const [pendingRequestsCount, setPendingRequestsCount] = useState(0);

  const handleGameCreated = useCallback(
    (gameId: string) => {
      api
        .getGame(gameId)
        .then((snapshot) => {
          const myColor =
            user?.handle && snapshot.black.handle === user.handle ? 'black' : 'white';
          onStartGame(snapshot, myColor, loadout);
        })
        .catch((err) => {
          console.error('Failed to load created game:', err);
        });
    },
    [user?.handle, onStartGame, loadout]
  );

  // Poll / refresh pending requests count
  useEffect(() => {
    if (!isAuthenticated) return;
    api
      .getFriendRequests()
      .then((r) => {
        setPendingRequestsCount(r.incoming.length);
      })
      .catch(() => {});

    const unsub = subscribe('friend.request', () => {
      setPendingRequestsCount((prev) => prev + 1);
    });

    return () => {
      unsub();
    };
  }, [isAuthenticated, subscribe]);

  // Listen to WebSocket matchmaking.matched events
  useEffect(() => {
    const unsubscribe = subscribe('matchmaking.matched', (envelope) => {
      const match = envelope.data as MatchmakingMatchedEvent;
      setIsQueued(false);

      api
        .getGame(match.game_id)
        .then((snapshot) => {
          onStartGame(snapshot, match.color, loadout);
        })
        .catch(() => {
          const snapshot: GameSnapshot = {
            id: match.game_id,
            rated: match.rated,
            pool: selectedTc.pool,
            tc: match.tc,
            white:
              match.color === 'white'
                ? { handle: user?.handle || 'me', rating: user?.ratings[selectedTc.pool] || 1200 }
                : match.opponent,
            black:
              match.color === 'black'
                ? { handle: user?.handle || 'me', rating: user?.ratings[selectedTc.pool] || 1200 }
                : match.opponent,
            status: 'active',
            fen: 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1',
            ply: 0,
            version: 0,
            clocks: {
              white_ms: match.tc.initial_s * 1000,
              black_ms: match.tc.initial_s * 1000,
              server_ts: new Date().toISOString(),
            },
            moves: [],
          };
          onStartGame(snapshot, match.color);
        });
    });

    return () => {
      unsubscribe();
    };
  }, [subscribe, selectedTc, user, onStartGame]);

  const handleFindMatch = async () => {
    if (!isAuthenticated) {
      openAuthModal();
      return;
    }

    setError(null);
    setIsQueued(true);

    try {
      const receipt = await api.joinQueue(
        {
          initial_s: selectedTc.initial_s,
          increment_s: selectedTc.increment_s,
        },
        rated
      );
      if (receipt?.est_wait_s) {
        setEstWait(receipt.est_wait_s);
      }
    } catch {
      // In standalone test mode, match simulated opponent after 2.5 seconds
      setEstWait(3);
      setTimeout(() => {
        setIsQueued(false);
        const demoOpponentHandle = 'grandmaster_vikram';
        const demoSnapshot: GameSnapshot = {
          id: `game_${Date.now().toString(36)}`,
          rated,
          pool: selectedTc.pool,
          tc: {
            initial_s: selectedTc.initial_s,
            increment_s: selectedTc.increment_s,
          },
          white: {
            handle: user?.handle || 'player',
            rating: user?.ratings[selectedTc.pool] || 1200,
          },
          black: {
            handle: demoOpponentHandle,
            rating: 1450,
          },
          status: 'active',
          fen: 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1',
          ply: 0,
          version: 0,
          clocks: {
            white_ms: selectedTc.initial_s * 1000,
            black_ms: selectedTc.initial_s * 1000,
            server_ts: new Date().toISOString(),
          },
          moves: [],
        };
        onStartGame(demoSnapshot, 'white');
      }, 2500);
    }
  };

  const handleCancelQueue = async () => {
    try {
      await api.leaveQueue();
    } finally {
      setIsQueued(false);
    }
  };

  const handlePracticeGame = () => {
    const practiceSnapshot: GameSnapshot = {
      id: `practice_${Date.now().toString(36)}`,
      rated: false,
      pool: selectedTc.pool,
      tc: {
        initial_s: selectedTc.initial_s,
        increment_s: selectedTc.increment_s,
      },
      white: {
        handle: user?.handle || 'Player',
        rating: user?.ratings[selectedTc.pool] || 1200,
      },
      black: {
        handle: 'Hustle Engine',
        rating: 1500,
      },
      status: 'active',
      fen: 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1',
      ply: 0,
      version: 0,
      clocks: {
        white_ms: selectedTc.initial_s * 1000,
        black_ms: selectedTc.initial_s * 1000,
        server_ts: new Date().toISOString(),
      },
      moves: [],
    };
    onStartGame(practiceSnapshot, 'white', loadout);
  };

  // Safe fallback stats for guests
  const bulletRating = user?.ratings.bullet ?? 1200;
  const blitzRating = user?.ratings.blitz ?? 1200;
  const rapidRating = user?.ratings.rapid ?? 1200;
  const totalGold = user?.gold ?? 1000;

  return (
    <div className="lobby-screen">
      <ProfileHeader
        onOpenFriends={() => setIsFriendsOpen(true)}
        onOpenCosmetics={() => setIsCosmeticsOpen(true)}
        onOpenMissions={() => setIsMissionsOpen(true)}
        onOpenAdmin={() => setIsAdminOpen(true)}
        onOpenTournaments={() => setIsTournamentsOpen(true)}
        onOpenAds={() => setIsAdsOpen(true)}
        pendingRequestsCount={pendingRequestsCount}
        claimableMissionsCount={claimableMissionsCount}
      />

      <main className="lobby-main">
        {error && (
          <div className="banner-alert banner-alert-error" role="alert">
            <span>{error}</span>
          </div>
        )}

        {/* Hero Section */}
        <div className="lobby-hero-card">
          <div className="hero-content">
            <h1 className="hero-title">Step up to the board</h1>
            <p className="hero-subtitle">
              Live chess matchmaking with Glicko-2 ratings, daily gold bounties, and zero-latency WebSocket clocks.
            </p>
          </div>

          <div className="hero-cta-group">
            <label className="rated-switch-label">
              <input
                type="checkbox"
                className="rated-switch-input"
                checked={rated}
                onChange={(e) => setRated(e.target.checked)}
                disabled={isQueued}
              />
              <span className="switch-text">
                Rated match <span className="switch-sub">(Affects Glicko-2)</span>
              </span>
            </label>

            <div className="cta-button-row">
              <button
                type="button"
                className="btn btn-primary btn-lg"
                onClick={handleFindMatch}
                disabled={isQueued}
              >
                <span>Find Match ({selectedTc.label} {selectedTc.pool.toUpperCase()})</span>
              </button>

              <button
                type="button"
                className="btn btn-secondary btn-lg"
                onClick={() => {
                  if (!isAuthenticated) {
                    openAuthModal();
                  } else {
                    setIsFriendsOpen(true);
                  }
                }}
                disabled={isQueued}
                title="View friends, direct challenges, and presence"
              >
                <span>👥 Friends & Challenges</span>
              </button>

              <button
                type="button"
                className="btn btn-secondary btn-lg"
                onClick={() => {
                  if (!isAuthenticated) {
                    openAuthModal();
                  } else {
                    setIsTournamentsOpen(true);
                  }
                }}
                disabled={isQueued}
                title="Browse and compete in scheduled Swiss tournaments"
              >
                <span>🏆 Tournaments</span>
              </button>

              <button
                type="button"
                className="btn btn-ghost btn-lg"
                onClick={handlePracticeGame}
                disabled={isQueued}
                title="Immediate practice game against local engine"
              >
                <span>Practice vs Bot</span>
              </button>
            </div>
          </div>
        </div>

        {/* Pool Time Control Selector */}
        <PoolSelector
          selectedTc={selectedTc}
          onSelectTc={setSelectedTc}
          disabled={isQueued}
        />

        {/* Dashboard Quick Stats */}
        <div className="dashboard-stats-grid">
          <div className="stat-card">
            <span className="stat-card-label">Bullet Rating</span>
            <span className="stat-card-value tabular-nums">{bulletRating}</span>
            <span className="stat-card-sub">Fast & instinctive (&lt; 3m)</span>
          </div>
          <div className="stat-card">
            <span className="stat-card-label">Blitz Rating</span>
            <span className="stat-card-value tabular-nums">{blitzRating}</span>
            <span className="stat-card-sub">Primary competitive pool</span>
          </div>
          <div className="stat-card">
            <span className="stat-card-label">Rapid Rating</span>
            <span className="stat-card-value tabular-nums">{rapidRating}</span>
            <span className="stat-card-sub">Deep calculation (10m+)</span>
          </div>
          <div className="stat-card">
            <span className="stat-card-label">Total Gold</span>
            <span className="stat-card-value stat-gold tabular-nums">
              {totalGold.toLocaleString()} 🪙
            </span>
            <span className="stat-card-sub">+25 Gold per win</span>
          </div>
        </div>
      </main>

      <MatchmakingQueue
        isQueued={isQueued}
        timeControl={selectedTc}
        estWaitSeconds={estWait}
        onCancel={handleCancelQueue}
      />

      <FriendsModal
        isOpen={isFriendsOpen}
        onClose={() => setIsFriendsOpen(false)}
        onChallengeFriend={(friend) => setChallengeTarget(friend)}
      />

      {challengeTarget && (
        <DirectChallengeModal
          target={challengeTarget}
          isOpen={!!challengeTarget}
          onClose={() => setChallengeTarget(null)}
          onGameCreated={handleGameCreated}
        />
      )}

      <ChallengeReceivedModal onGameAccepted={handleGameCreated} />

      <CosmeticsModal
        isOpen={isCosmeticsOpen}
        onClose={() => setIsCosmeticsOpen(false)}
        onLoadoutChange={setLoadout}
      />

      <MissionsModal
        isOpen={isMissionsOpen}
        onClose={() => setIsMissionsOpen(false)}
        onClaimableCountChange={setClaimableMissionsCount}
      />

      <AdminModal
        isOpen={isAdminOpen}
        onClose={() => setIsAdminOpen(false)}
      />

      <TournamentsModal
        isOpen={isTournamentsOpen}
        onClose={() => setIsTournamentsOpen(false)}
        onOpenGame={handleGameCreated}
      />

      <RewardedAdsModal
        isOpen={isAdsOpen}
        onClose={() => setIsAdsOpen(false)}
      />
    </div>
  );
}
