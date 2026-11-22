import { useState, useEffect } from 'react';
import { useRealtime } from '../realtime/useRealtime';
import { api } from '../api/client';

interface IncomingChallenge {
  challenge_id: string;
  challenger_handle: string;
  challenger_rating: number;
  initial_s: number;
  increment_s: number;
  rated: boolean;
}

interface ChallengeReceivedModalProps {
  onGameAccepted: (gameId: string) => void;
}

export function ChallengeReceivedModal({ onGameAccepted }: ChallengeReceivedModalProps) {
  const { subscribe } = useRealtime();
  const [invite, setInvite] = useState<IncomingChallenge | null>(null);
  const [isActing, setIsActing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const unsubscribe = subscribe('game.invite', (envelope) => {
      const data = envelope.data as IncomingChallenge;
      if (data && data.challenge_id) {
        setInvite(data);
        setError(null);
      }
    });

    return () => {
      unsubscribe();
    };
  }, [subscribe]);

  if (!invite) return null;

  const tcLabel = `${Math.floor(invite.initial_s / 60)}m${
    invite.increment_s > 0 ? `+${invite.increment_s}s` : ''
  }`;

  const handleAccept = async () => {
    setIsActing(true);
    setError(null);
    try {
      const result = await api.acceptChallenge(invite.challenge_id);
      setInvite(null);
      if (result.game_id) {
        onGameAccepted(result.game_id);
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to accept challenge';
      setError(msg);
    } finally {
      setIsActing(false);
    }
  };

  const handleDecline = async () => {
    setIsActing(true);
    try {
      await api.declineChallenge(invite.challenge_id);
    } catch {
      // ignore
    } finally {
      setInvite(null);
      setIsActing(false);
    }
  };

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="modal-card modal-invite" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <div className="modal-title-group">
            <h2 className="modal-title">⚔️ Direct Challenge Received!</h2>
            <span className="modal-subtitle">A player has challenged you to a game</span>
          </div>
        </div>

        <div className="modal-body invite-modal-body">
          <div className="invite-player-card">
            <div className="user-avatar-circle invite-avatar">
              {invite.challenger_handle.slice(0, 2).toUpperCase()}
            </div>
            <div className="invite-player-info">
              <span className="invite-handle">@{invite.challenger_handle}</span>
              <span className="invite-rating tabular-nums">{invite.challenger_rating} Elo</span>
            </div>
          </div>

          <div className="invite-details-grid">
            <div className="invite-detail-item">
              <span className="detail-label">Time Control</span>
              <span className="detail-value">{tcLabel}</span>
            </div>
            <div className="invite-detail-item">
              <span className="detail-label">Mode</span>
              <span className="detail-value">{invite.rated ? 'Rated' : 'Casual'}</span>
            </div>
          </div>

          {error && (
            <div className="banner-alert banner-alert-error" style={{ margin: '0.75rem 0 0' }}>
              <span>{error}</span>
            </div>
          )}

          <div className="modal-actions invite-actions">
            <button
              type="button"
              className="btn btn-ghost"
              onClick={handleDecline}
              disabled={isActing}
            >
              Decline
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={handleAccept}
              disabled={isActing}
            >
              {isActing ? 'Connecting...' : 'Accept Challenge'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
