import { useState, useEffect } from 'react';
import { api } from '../api/client';
import { useRealtime } from '../realtime/useRealtime';
import { FriendInfo } from '../types';

interface DirectChallengeModalProps {
  target: FriendInfo | { handle: string; rating?: number };
  isOpen: boolean;
  onClose: () => void;
  onGameCreated: (gameId: string) => void;
}

interface ChallengeTcOption {
  label: string;
  initial_s: number;
  increment_s: number;
}

const CHALLENGE_TCS: ChallengeTcOption[] = [
  { label: '1+0 Bullet', initial_s: 60, increment_s: 0 },
  { label: '3+0 Blitz', initial_s: 180, increment_s: 0 },
  { label: '3+2 Blitz', initial_s: 180, increment_s: 2 },
  { label: '5+0 Blitz', initial_s: 300, increment_s: 0 },
  { label: '10+0 Rapid', initial_s: 600, increment_s: 0 },
];

export function DirectChallengeModal({
  target,
  isOpen,
  onClose,
  onGameCreated,
}: DirectChallengeModalProps) {
  const { subscribe } = useRealtime();
  const [selectedTc, setSelectedTc] = useState<ChallengeTcOption>(
    CHALLENGE_TCS[2] ?? { label: '3+2 Blitz', initial_s: 180, increment_s: 2 }
  );
  const [rated, setRated] = useState(true);
  const [isPending, setIsPending] = useState(false);
  const [challengeId, setChallengeId] = useState<string | null>(null);
  const [timeLeft, setTimeLeft] = useState(120);
  const [error, setError] = useState<string | null>(null);

  // Countdown timer while pending
  useEffect(() => {
    if (!isPending) return;
    const interval = setInterval(() => {
      setTimeLeft((prev) => {
        if (prev <= 1) {
          setIsPending(false);
          setError('Challenge expired (no response).');
          return 0;
        }
        return prev - 1;
      });
    }, 1000);

    return () => clearInterval(interval);
  }, [isPending]);

  // Listen to WebSocket invite result
  useEffect(() => {
    if (!challengeId) return;

    const unsubscribe = subscribe('game.invite_result', (envelope) => {
      const data = envelope.data;
      if (data.challenge_id === challengeId) {
        if (data.status === 'accepted' && data.game_id) {
          setIsPending(false);
          onGameCreated(data.game_id);
          onClose();
        } else if (data.status === 'declined') {
          setIsPending(false);
          setError(`@${target.handle} declined your challenge.`);
        }
      }
    });

    return () => {
      unsubscribe();
    };
  }, [challengeId, target.handle, subscribe, onGameCreated, onClose]);

  if (!isOpen) return null;

  const handleSendChallenge = async () => {
    setError(null);
    try {
      const challenge = await api.createChallenge(
        target.handle,
        {
          initial_s: selectedTc.initial_s,
          increment_s: selectedTc.increment_s,
        },
        rated
      );
      setChallengeId(challenge.id);
      setIsPending(true);
      setTimeLeft(120);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to send challenge';
      setError(msg);
    }
  };

  const handleCancelChallenge = async () => {
    if (challengeId) {
      try {
        await api.declineChallenge(challengeId);
      } catch {
        // ignore
      }
    }
    setIsPending(false);
    setChallengeId(null);
    onClose();
  };

  return (
    <div className="modal-backdrop" onClick={onClose} role="dialog" aria-modal="true">
      <div className="modal-card modal-challenge" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <div className="modal-title-group">
            <h2 className="modal-title">Direct Challenge</h2>
            <span className="modal-subtitle">
              Challenge @{target.handle} {target.rating ? `(${target.rating})` : ''}
            </span>
          </div>
          <button
            type="button"
            className="modal-close-btn"
            onClick={onClose}
            aria-label="Close modal"
          >
            ✕
          </button>
        </div>

        <div className="modal-body challenge-modal-body">
          {error && <div className="banner-alert banner-alert-error">{error}</div>}

          {!isPending ? (
            <>
              {/* Time Control Options */}
              <div className="challenge-section">
                <label className="section-label">Select Time Control</label>
                <div className="challenge-tc-grid">
                  {CHALLENGE_TCS.map((tc) => (
                    <button
                      key={tc.label}
                      type="button"
                      className={`tc-select-btn ${selectedTc.label === tc.label ? 'active' : ''}`}
                      onClick={() => setSelectedTc(tc)}
                    >
                      <span className="tc-title">{tc.label}</span>
                      <span className="tc-desc">
                        {tc.initial_s / 60}m {tc.increment_s > 0 ? `+ ${tc.increment_s}s` : ''}
                      </span>
                    </button>
                  ))}
                </div>
              </div>

              {/* Rated vs Casual Toggle */}
              <div className="challenge-section">
                <label className="rated-switch-label">
                  <input
                    type="checkbox"
                    className="rated-switch-input"
                    checked={rated}
                    onChange={(e) => setRated(e.target.checked)}
                  />
                  <span className="switch-text">
                    Rated match <span className="switch-sub">(Affects Glicko-2 ratings)</span>
                  </span>
                </label>
              </div>

              <div className="modal-actions">
                <button type="button" className="btn btn-ghost" onClick={onClose}>
                  Cancel
                </button>
                <button
                  type="button"
                  className="btn btn-primary"
                  onClick={handleSendChallenge}
                >
                  Send Challenge ({selectedTc.label})
                </button>
              </div>
            </>
          ) : (
            <div className="challenge-pending-state">
              <div className="pending-spinner">⚔️</div>
              <h3 className="pending-title">Challenge Sent</h3>
              <p className="pending-sub">
                Waiting for <strong>@{target.handle}</strong> to respond...
              </p>
              <div className="pending-timer tabular-nums">Expires in {timeLeft}s</div>
              <button
                type="button"
                className="btn btn-ghost btn-sm"
                onClick={handleCancelChallenge}
              >
                Cancel Challenge
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
