import { useState, useEffect, useCallback } from 'react';
import { api } from '../api/client';
import { MissionItem, AchievementItem } from '../types';
import { useAuth } from '../auth/useAuth';

interface MissionsModalProps {
  isOpen: boolean;
  onClose: () => void;
  onClaimableCountChange?: (count: number) => void;
}

const MISSION_TITLES: Record<string, { title: string; desc: string; icon: string }> = {
  daily_games_1: {
    title: 'Daily Warmup',
    desc: 'Play 1 game in any rated or blitz pool',
    icon: '♟️',
  },
  daily_games_3: {
    title: 'Street Hustler',
    desc: 'Play 3 games across any pool',
    icon: '🎮',
  },
  daily_win_1: {
    title: 'Taste of Victory',
    desc: 'Win at least 1 match today',
    icon: '🏆',
  },
  daily_checkmate_1: {
    title: 'King Slayer',
    desc: 'Deliver a decisive checkmate in a live game',
    icon: '👑',
  },
  weekly_games_15: {
    title: 'Weekly Marathon',
    desc: 'Complete 15 games throughout this week',
    icon: '⚡',
  },
  weekly_wins_7: {
    title: 'Grandmaster in the Making',
    desc: 'Score 7 wins this week',
    icon: '🔥',
  },
};

const ACHIEVEMENT_INFO: Record<string, { title: string; desc: string; icon: string }> = {
  achieve_first_win: {
    title: 'First Blood',
    desc: 'Win your first live chess match',
    icon: '🏆',
  },
  achieve_games_10: {
    title: 'Board Regular',
    desc: 'Play 10 games across all pools',
    icon: '⚔️',
  },
  achieve_games_50: {
    title: 'Arena Veteran',
    desc: 'Play 50 games on Chess Hustle',
    icon: '🛡️',
  },
  achieve_checkmate_5: {
    title: 'Mating Specialist',
    desc: 'Deliver 5 checkmates against human opponents',
    icon: '👑',
  },
  achieve_streak_7: {
    title: 'Seven Days of Hustle',
    desc: 'Maintain a 7-day login & claim streak',
    icon: '🔥',
  },
  achieve_rating_1500: {
    title: 'The 1500 Club',
    desc: 'Reach a peak Glicko-2 rating of 1500 in any pool',
    icon: '⭐',
  },
};

export function MissionsModal({ isOpen, onClose, onClaimableCountChange }: MissionsModalProps) {
  const { user, isAuthenticated, updateGold } = useAuth();
  const [activeTab, setActiveTab] = useState<'missions' | 'achievements'>('missions');
  const [missions, setMissions] = useState<MissionItem[]>([]);
  const [achievements, setAchievements] = useState<AchievementItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [claimingId, setClaimingId] = useState<string | null>(null);
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  const fetchData = useCallback(async () => {
    if (!isAuthenticated) return;
    setLoading(true);
    try {
      const [mList, aList] = await Promise.all([
        api.getMissions(),
        api.getAchievements(),
      ]);
      setMissions(mList);
      setAchievements(aList);

      const claimable = mList.filter((m) => m.progress >= m.goal && !m.claimed).length;
      if (onClaimableCountChange) {
        onClaimableCountChange(claimable);
      }
    } catch (err: unknown) {
      console.error('Failed to load missions or achievements:', err);
    } finally {
      setLoading(false);
    }
  }, [isAuthenticated, onClaimableCountChange]);

  useEffect(() => {
    if (isOpen) {
      fetchData();
      setMessage(null);
    }
  }, [isOpen, fetchData]);

  if (!isOpen) return null;

  const handleClaimMission = async (mission: MissionItem) => {
    if (claimingId) return;
    setClaimingId(mission.id);
    setMessage(null);
    try {
      const res = await api.claimMission(mission.id);
      updateGold(res.new_balance);
      setMessage({
        type: 'success',
        text: `Claimed +${res.amount.toLocaleString()} 🪙! New Balance: ${res.new_balance.toLocaleString()} 🪙`,
      });
      await fetchData();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Claim failed';
      setMessage({ type: 'error', text: msg });
    } finally {
      setClaimingId(null);
    }
  };

  const dailyMissions = missions.filter((m) => m.kind === 'daily');
  const weeklyMissions = missions.filter((m) => m.kind === 'weekly');
  const totalUnlockedAchievements = achievements.filter((a) => a.unlocked).length;

  return (
    <div className="modal-backdrop" onClick={onClose} role="dialog" aria-modal="true">
      <div className="modal-card modal-lg" onClick={(e) => e.stopPropagation()}>
        {/* Header */}
        <div className="modal-header">
          <div className="header-left">
            <h2 className="modal-title">🎯 Missions & Achievements</h2>
            <span className="modal-subtitle">Earn Gold through daily bounties, weekly marathons & milestones</span>
          </div>
          <div className="header-right-group">
            {isAuthenticated && user && (
              <div className="vault-gold-chip" title="Current balance">
                <span className="gold-icon">🪙</span>
                <span className="gold-amount tabular-nums">{user.gold.toLocaleString()}</span>
              </div>
            )}
            <button
              type="button"
              className="modal-close-btn"
              onClick={onClose}
              aria-label="Close modal"
            >
              ✕
            </button>
          </div>
        </div>

        {/* Status Message */}
        {message && (
          <div
            className={`banner-alert ${
              message.type === 'success' ? 'banner-alert-success' : 'banner-alert-error'
            }`}
          >
            <span>{message.text}</span>
          </div>
        )}

        {/* Tab Switcher */}
        <div className="cosmetics-tab-nav" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={activeTab === 'missions'}
            className={`cosmetics-tab ${activeTab === 'missions' ? 'active' : ''}`}
            onClick={() => setActiveTab('missions')}
          >
            <span>🎯</span>
            <span>Missions (Daily & Weekly)</span>
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={activeTab === 'achievements'}
            className={`cosmetics-tab ${activeTab === 'achievements' ? 'active' : ''}`}
            onClick={() => setActiveTab('achievements')}
          >
            <span>🏆</span>
            <span>Achievements ({totalUnlockedAchievements}/{achievements.length})</span>
          </button>
        </div>

        {/* Body Content */}
        <div className="modal-body-scroll">
          {loading ? (
            <div className="loading-state">
              <span className="loading-spinner">⌛</span>
              <span>Loading Missions & Milestones...</span>
            </div>
          ) : activeTab === 'missions' ? (
            <div className="missions-container">
              {/* Daily Section */}
              <div className="mission-group">
                <div className="group-header-row">
                  <span className="group-title">⚡ Daily Missions</span>
                  <span className="group-subtext">Resets daily at 00:00 IST (UTC+5:30)</span>
                </div>

                <div className="mission-list">
                  {dailyMissions.map((m) => {
                    const info = MISSION_TITLES[m.id] || {
                      title: m.id,
                      desc: `Goal: ${m.goal_type}`,
                      icon: '♟️',
                    };
                    const pct = Math.min(100, Math.round((m.progress / m.goal) * 100));
                    const canClaim = m.progress >= m.goal && !m.claimed;

                    return (
                      <div key={m.id} className={`mission-card ${m.claimed ? 'claimed' : ''}`}>
                        <div className="mission-icon-box">{info.icon}</div>
                        <div className="mission-content">
                          <div className="mission-header-line">
                            <span className="mission-title">{info.title}</span>
                            <span className="mission-reward-badge">+{m.reward_gold} 🪙</span>
                          </div>
                          <span className="mission-desc">{info.desc}</span>

                          {/* Progress bar */}
                          <div className="mission-progress-bar-wrap">
                            <div
                              className="mission-progress-bar-fill"
                              style={{ width: `${pct}%` }}
                            />
                          </div>
                          <div className="mission-progress-text-row">
                            <span className="progress-nums tabular-nums">
                              {m.progress} / {m.goal}
                            </span>
                            <span className="progress-pct tabular-nums">{pct}%</span>
                          </div>
                        </div>

                        {/* Action button */}
                        <div className="mission-action">
                          {m.claimed ? (
                            <span className="status-claimed-badge">✓ Claimed</span>
                          ) : canClaim ? (
                            <button
                              type="button"
                              className="btn btn-accent btn-sm"
                              onClick={() => handleClaimMission(m)}
                              disabled={claimingId === m.id}
                            >
                              <span>{claimingId === m.id ? 'Claiming...' : `Claim (+${m.reward_gold})`}</span>
                            </button>
                          ) : (
                            <span className="status-in-progress-badge">In Progress</span>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>

              {/* Weekly Section */}
              <div className="mission-group">
                <div className="group-header-row">
                  <span className="group-title">📅 Weekly Marathon</span>
                  <span className="group-subtext">Resets every Monday at 00:00 IST</span>
                </div>

                <div className="mission-list">
                  {weeklyMissions.map((m) => {
                    const info = MISSION_TITLES[m.id] || {
                      title: m.id,
                      desc: `Goal: ${m.goal_type}`,
                      icon: '⚡',
                    };
                    const pct = Math.min(100, Math.round((m.progress / m.goal) * 100));
                    const canClaim = m.progress >= m.goal && !m.claimed;

                    return (
                      <div key={m.id} className={`mission-card ${m.claimed ? 'claimed' : ''}`}>
                        <div className="mission-icon-box">{info.icon}</div>
                        <div className="mission-content">
                          <div className="mission-header-line">
                            <span className="mission-title">{info.title}</span>
                            <span className="mission-reward-badge">+{m.reward_gold} 🪙</span>
                          </div>
                          <span className="mission-desc">{info.desc}</span>

                          <div className="mission-progress-bar-wrap">
                            <div
                              className="mission-progress-bar-fill"
                              style={{ width: `${pct}%` }}
                            />
                          </div>
                          <div className="mission-progress-text-row">
                            <span className="progress-nums tabular-nums">
                              {m.progress} / {m.goal}
                            </span>
                            <span className="progress-pct tabular-nums">{pct}%</span>
                          </div>
                        </div>

                        <div className="mission-action">
                          {m.claimed ? (
                            <span className="status-claimed-badge">✓ Claimed</span>
                          ) : canClaim ? (
                            <button
                              type="button"
                              className="btn btn-accent btn-sm"
                              onClick={() => handleClaimMission(m)}
                              disabled={claimingId === m.id}
                            >
                              <span>{claimingId === m.id ? 'Claiming...' : `Claim (+${m.reward_gold})`}</span>
                            </button>
                          ) : (
                            <span className="status-in-progress-badge">In Progress</span>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>
          ) : (
            /* Achievements View */
            <div className="achievements-container">
              <div className="achievements-grid">
                {achievements.map((ach) => {
                  const info = ACHIEVEMENT_INFO[ach.id] || {
                    title: ach.id,
                    desc: `Goal: ${ach.goal_type}`,
                    icon: '🏆',
                  };
                  const pct = Math.min(100, Math.round((ach.progress / ach.target) * 100));

                  return (
                    <div
                      key={ach.id}
                      className={`achievement-card ${ach.unlocked ? 'unlocked' : 'locked'}`}
                    >
                      <div className="achievement-icon-circle">{info.icon}</div>
                      <div className="achievement-info">
                        <div className="achievement-header-line">
                          <span className="achievement-title">{info.title}</span>
                          <span className="achievement-gold-badge">+{ach.reward_gold} 🪙</span>
                        </div>
                        <span className="achievement-desc">{info.desc}</span>

                        <div className="mission-progress-bar-wrap">
                          <div
                            className="mission-progress-bar-fill"
                            style={{ width: `${pct}%` }}
                          />
                        </div>

                        <div className="achievement-footer-row">
                          <span className="tabular-nums">
                            {ach.progress.toLocaleString()} / {ach.target.toLocaleString()}
                          </span>
                          {ach.unlocked ? (
                            <span className="achievement-unlocked-tag">✓ Unlocked</span>
                          ) : (
                            <span className="achievement-locked-tag">{pct}%</span>
                          )}
                        </div>
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
