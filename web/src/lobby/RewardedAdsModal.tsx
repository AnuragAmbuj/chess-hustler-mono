import { useState, useEffect, useCallback } from 'react';
import { api } from '../api/client';
import { AdStatus, AdSessionResponse } from '../types';
import { useAuth } from '../auth/useAuth';

interface RewardedAdsModalProps {
  isOpen: boolean;
  onClose: () => void;
  onRewardClaimed?: (rewardGold: number) => void;
}

export function RewardedAdsModal({ isOpen, onClose, onRewardClaimed }: RewardedAdsModalProps) {
  const { user, isAuthenticated, updateGold } = useAuth();
  const [status, setStatus] = useState<AdStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [activeSession, setActiveSession] = useState<AdSessionResponse | null>(null);
  const [adPlaying, setAdPlaying] = useState(false);
  const [adSecondsRemaining, setAdSecondsRemaining] = useState(10);
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  const fetchStatus = useCallback(async () => {
    if (!isAuthenticated) return;
    setLoading(true);
    try {
      const res = await api.getAdStatus();
      setStatus(res);
    } catch (err: unknown) {
      console.error('Failed to load ad status:', err);
    } finally {
      setLoading(false);
    }
  }, [isAuthenticated]);

  useEffect(() => {
    if (isOpen) {
      fetchStatus();
      setMessage(null);
      setAdPlaying(false);
      setActiveSession(null);
    }
  }, [isOpen, fetchStatus]);

  // Handle ad playback countdown
  useEffect(() => {
    if (!adPlaying) return;

    if (adSecondsRemaining <= 0) {
      // Ad finished!
      handleAdFinished();
      return;
    }

    const timer = setInterval(() => {
      setAdSecondsRemaining((prev) => prev - 1);
    }, 1000);

    return () => clearInterval(timer);
  }, [adPlaying, adSecondsRemaining]);

  if (!isOpen) return null;

  const handleStartWatchAd = async () => {
    if (!isAuthenticated || !status) return;
    setMessage(null);
    setLoading(true);

    try {
      const session = await api.createAdSession();
      setActiveSession(session);
      setAdSecondsRemaining(10); // 10 second presentation
      setAdPlaying(true);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Cannot start ad session';
      setMessage({ type: 'error', text: msg });
    } finally {
      setLoading(false);
    }
  };

  const handleAdFinished = async () => {
    setAdPlaying(false);
    const reward = activeSession?.reward_gold || status?.gold_per_ad || 100;

    // In production, SSV is triggered server-to-server by Google AdMob.
    // For web client test & demonstration, we credit the reward to the local balance.
    if (user) {
      updateGold(user.gold + reward);
    }
    if (onRewardClaimed) {
      onRewardClaimed(reward);
    }

    setMessage({
      type: 'success',
      text: `🎉 Ad completed! +${reward} Gold credited to your vault.`,
    });

    // Refresh ad status
    await fetchStatus();
  };

  const isEligible = status?.eligibility.eligible ?? true;
  const remainingToday = status ? Math.max(0, status.max_per_day - status.used_today) : 5;
  const isQuotaReached = status ? status.used_today >= status.max_per_day : false;

  return (
    <div className="modal-backdrop" onClick={onClose} role="presentation">
      <div
        className="modal-card modal-ads"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="ads-modal-title"
        style={{ maxWidth: '520px', width: '90%' }}
      >
        {/* Header */}
        <div className="modal-header">
          <div className="modal-title-group">
            <span className="modal-title-icon">📺</span>
            <h2 id="ads-modal-title" className="modal-title">
              Rewarded Gold Station
            </h2>
          </div>
          <button
            type="button"
            className="btn-close"
            onClick={onClose}
            aria-label="Close Rewarded Ads Dialog"
            disabled={adPlaying}
          >
            ✕
          </button>
        </div>

        {/* Notifications */}
        {message && (
          <div
            className={`banner-alert ${
              message.type === 'success' ? 'banner-alert-success' : 'banner-alert-error'
            }`}
            style={{ margin: '0.75rem 1.25rem 0' }}
          >
            <span>{message.text}</span>
          </div>
        )}

        <div className="modal-body-scroll" style={{ padding: '1.25rem' }}>
          {adPlaying ? (
            /* AD PLAYER SCREEN */
            <div
              style={{
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'center',
                textAlign: 'center',
                padding: '1.5rem',
                background: 'var(--bg-elevated)',
                border: '1px solid var(--border)',
                borderRadius: '8px',
                gap: '1rem',
              }}
            >
              <div
                style={{
                  width: '100%',
                  height: '180px',
                  background: 'linear-gradient(135deg, #1e293b 0%, #0f172a 100%)',
                  borderRadius: '6px',
                  display: 'flex',
                  flexDirection: 'column',
                  justifyContent: 'center',
                  alignItems: 'center',
                  border: '1px solid var(--border-focus)',
                  position: 'relative',
                  overflow: 'hidden',
                }}
              >
                <div style={{ fontSize: '3rem', marginBottom: '0.5rem' }}>♟️✨</div>
                <div style={{ color: 'var(--accent)', fontWeight: 700, fontSize: '1.1rem' }}>
                  CHESS HUSTLE ARENA
                </div>
                <div style={{ color: 'var(--text-muted)', fontSize: '0.85rem' }}>
                  Sponsored Fair-Play Servers & Community
                </div>

                <div
                  style={{
                    position: 'absolute',
                    top: '8px',
                    right: '10px',
                    background: 'rgba(0,0,0,0.6)',
                    padding: '0.2rem 0.5rem',
                    borderRadius: '4px',
                    fontSize: '0.75rem',
                    color: 'var(--text-main)',
                  }}
                >
                  Ad: {adSecondsRemaining}s
                </div>
              </div>

              {/* Progress bar */}
              <div
                style={{
                  width: '100%',
                  height: '6px',
                  background: 'var(--bg-surface)',
                  borderRadius: '3px',
                  overflow: 'hidden',
                }}
              >
                <div
                  style={{
                    height: '100%',
                    background: 'var(--accent)',
                    width: `${((10 - adSecondsRemaining) / 10) * 100}%`,
                    transition: 'width 0.9s linear',
                  }}
                />
              </div>

              <div style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>
                Reward unlocks when timer reaches zero. Please do not close this window.
              </div>
            </div>
          ) : (
            /* AD DETAILS & STATUS */
            <div style={{ display: 'flex', flexDirection: 'column', gap: '1rem' }}>
              {/* Daily Quota Card */}
              <div
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                  padding: '1rem',
                  background: 'var(--bg-elevated)',
                  border: '1px solid var(--border)',
                  borderRadius: '8px',
                }}
              >
                <div>
                  <div style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>Daily Allowance</div>
                  <div style={{ fontSize: '1.25rem', fontWeight: 700, color: 'var(--text-main)' }}>
                    {remainingToday} of {status?.max_per_day || 5} Available
                  </div>
                  <div style={{ fontSize: '0.75rem', color: 'var(--text-dim)' }}>
                    Resets daily at 00:00 IST (UTC+5:30)
                  </div>
                </div>

                <div style={{ textAlign: 'right' }}>
                  <div style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>Reward Per Ad</div>
                  <div style={{ fontSize: '1.25rem', fontWeight: 700, color: 'var(--accent)' }}>
                    +{status?.gold_per_ad || 100} 🪙
                  </div>
                  <div style={{ fontSize: '0.75rem', color: 'var(--text-dim)' }}>Zero real-money (ADR-011)</div>
                </div>
              </div>

              {/* Eligibility Checklist */}
              <div
                style={{
                  padding: '1rem',
                  background: 'var(--bg-elevated)',
                  border: '1px solid var(--border)',
                  borderRadius: '8px',
                  display: 'flex',
                  flexDirection: 'column',
                  gap: '0.5rem',
                }}
              >
                <div style={{ fontSize: '0.85rem', fontWeight: 600, color: 'var(--text-main)', marginBottom: '0.25rem' }}>
                  Eligibility & Fair Play Checks
                </div>

                <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', fontSize: '0.85rem' }}>
                  <span>{status?.eligibility.account_age_ok ? '✅' : '⏳'}</span>
                  <span style={{ color: status?.eligibility.account_age_ok ? 'var(--text-main)' : 'var(--text-muted)' }}>
                    Account age &gt;= 24 hours
                  </span>
                </div>

                <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', fontSize: '0.85rem' }}>
                  <span>{status?.eligibility.games_completed_ok ? '✅' : '⏳'}</span>
                  <span style={{ color: status?.eligibility.games_completed_ok ? 'var(--text-main)' : 'var(--text-muted)' }}>
                    Completed &gt;= 5 games on Chess Hustle
                  </span>
                </div>

                <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', fontSize: '0.85rem' }}>
                  <span>{status?.eligibility.no_active_game ? '✅' : '❌'}</span>
                  <span style={{ color: status?.eligibility.no_active_game ? 'var(--text-main)' : 'var(--danger)' }}>
                    Not currently inside an active game
                  </span>
                </div>

                <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', fontSize: '0.85rem' }}>
                  <span>{status?.eligibility.not_sanctioned ? '✅' : '❌'}</span>
                  <span style={{ color: status?.eligibility.not_sanctioned ? 'var(--text-main)' : 'var(--danger)' }}>
                    Clean fair play record (no sanctions)
                  </span>
                </div>
              </div>

              {/* Action Button */}
              <div style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem', marginTop: '0.5rem' }}>
                <button
                  type="button"
                  className="btn btn-primary btn-lg"
                  onClick={handleStartWatchAd}
                  disabled={loading || isQuotaReached || !isEligible}
                  style={{ width: '100%', justifyContent: 'center' }}
                >
                  {loading
                    ? 'Loading Session...'
                    : isQuotaReached
                    ? 'Daily Limit Reached (Come back tomorrow)'
                    : !isEligible
                    ? 'Ineligible for Rewarded Ads'
                    : `Watch Video Ad (+${status?.gold_per_ad || 100} Gold)`}
                </button>

                <p
                  style={{
                    fontSize: '0.75rem',
                    color: 'var(--text-dim)',
                    textAlign: 'center',
                    margin: 0,
                  }}
                >
                  Ads on Chess Hustle follow ADR-007 server-side verification and are strictly voluntary.
                  Gold is non-monetary and cannot be exchanged for cash.
                </p>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
