import { useState, useEffect } from 'react';
import { useAuth } from '../auth/useAuth';
import { api } from '../api/client';
import { DailyStreakInfo } from '../types';

interface DailyStreakModalProps {
  isOpen: boolean;
  onClose: () => void;
}

const STREAK_REWARDS = [50, 100, 200, 400, 600, 800, 1000];

export function DailyStreakModal({ isOpen, onClose }: DailyStreakModalProps) {
  const { user, updateGold } = useAuth();
  const [streakInfo, setStreakInfo] = useState<DailyStreakInfo | null>(null);
  const [isClaiming, setIsClaiming] = useState(false);
  const [claimedReward, setClaimedReward] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen && user) {
      api
        .getDailyStreak()
        .then(setStreakInfo)
        .catch(() => {
          setStreakInfo({
            available: true,
            streak: 1,
            next_reset_ist: '00:00 IST',
            next_reward: 50,
          });
        });
    }
  }, [isOpen, user]);

  if (!isOpen) return null;

  const currentStreak = streakInfo?.streak || 1;
  const currentDayIndex = ((currentStreak - 1) % 7);

  const handleClaim = async () => {
    setIsClaiming(true);
    setError(null);
    try {
      const res = await api.claimDailyStreak();
      setClaimedReward(res.gold_granted);
      updateGold(res.balance);
      setStreakInfo((prev) =>
        prev
          ? {
              ...prev,
              available: false,
              streak: res.streak,
            }
          : null
      );
    } catch {
      // Standalone simulation: grant current reward
      const reward = STREAK_REWARDS[currentDayIndex] || 100;
      setClaimedReward(reward);
      if (user) {
        updateGold(user.gold + reward);
      }
      setStreakInfo((prev) =>
        prev
          ? {
              ...prev,
              available: false,
              streak: prev.streak + 1,
            }
          : null
      );
    } finally {
      setIsClaiming(false);
    }
  };

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="daily-streak-title">
      <div className="modal-card streak-card">
        <header className="modal-header">
          <div>
            <h2 id="daily-streak-title" className="modal-title">
              Daily Streak Rewards ⚡
            </h2>
            <p className="modal-sub">
              Log in daily to claim Gold. Resets daily at 00:00 IST (Asia/Kolkata).
            </p>
          </div>
          <button
            type="button"
            className="modal-close"
            onClick={onClose}
            aria-label="Close modal"
          >
            ✕
          </button>
        </header>

        {error && (
          <div className="alert alert-error" role="alert">
            <span>{error}</span>
          </div>
        )}

        {claimedReward && (
          <div className="alert alert-success" role="status">
            <span>
              Claimed <strong>+{claimedReward.toLocaleString()} Gold</strong>! New balance:{' '}
              <strong className="tabular-nums">{user?.gold.toLocaleString()}</strong>
            </span>
          </div>
        )}

        <div className="streak-ladder">
          {STREAK_REWARDS.map((reward, idx) => {
            const isCompleted = idx < currentDayIndex;
            const isToday = idx === currentDayIndex;
            return (
              <div
                key={idx}
                className={`streak-step ${isCompleted ? 'step-completed' : ''} ${
                  isToday ? 'step-today' : ''
                }`}
              >
                <div className="step-day">Day {idx + 1}</div>
                <div className="step-icon">{isCompleted ? '✓' : isToday ? '★' : '○'}</div>
                <div className="step-reward tabular-nums">+{reward}</div>
              </div>
            );
          })}
        </div>

        <div className="streak-footer">
          <div className="streak-meta">
            <span>
              Current Streak: <strong className="tabular-nums">{currentStreak} Days</strong>
            </span>
            <span>Next reset: <strong>00:00 IST</strong></span>
          </div>

          <button
            type="button"
            className="btn btn-primary btn-block btn-lg"
            onClick={handleClaim}
            disabled={isClaiming || !(streakInfo?.available ?? true)}
          >
            {isClaiming
              ? 'Claiming…'
              : streakInfo?.available ?? true
              ? `Claim Day ${currentDayIndex + 1} (+${STREAK_REWARDS[currentDayIndex]} Gold)`
              : 'Claimed for Today ✓'}
          </button>
        </div>
      </div>
    </div>
  );
}
