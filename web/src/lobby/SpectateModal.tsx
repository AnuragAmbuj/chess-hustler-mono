import React, { useState } from 'react';
import { useTranslation } from '../i18n/useTranslation';
import { GameSnapshot, LiveGameSummary } from '../types';

interface SpectateModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSpectateGame: (snapshot: GameSnapshot) => void;
}

const MOCK_LIVE_GAMES: LiveGameSummary[] = [
  {
    id: 'live_gm_01',
    white: { handle: 'gukesh_d', rating: 2795 },
    black: { handle: 'pragg_r', rating: 2772 },
    pool: 'blitz',
    tc: { initial_s: 180, increment_s: 2 },
    ply: 28,
    started_at: '2 min ago',
    spectators: 342,
  },
  {
    id: 'live_gm_02',
    white: { handle: 'erigaisi_a', rating: 2765 },
    black: { handle: 'vidit_g', rating: 2740 },
    pool: 'bullet',
    tc: { initial_s: 60, increment_s: 0 },
    ply: 42,
    started_at: '45 sec ago',
    spectators: 189,
  },
  {
    id: 'live_gm_03',
    white: { handle: 'humpy_k', rating: 2715 },
    black: { handle: 'vaishali_r', rating: 2650 },
    pool: 'rapid',
    tc: { initial_s: 600, increment_s: 5 },
    ply: 16,
    started_at: '5 min ago',
    spectators: 215,
  },
];

export const SpectateModal: React.FC<SpectateModalProps> = ({
  isOpen,
  onClose,
  onSpectateGame,
}) => {
  const { t } = useTranslation();
  const [games] = useState<LiveGameSummary[]>(MOCK_LIVE_GAMES);

  if (!isOpen) return null;

  const handleSelectGame = (game: LiveGameSummary) => {
    const snapshot: GameSnapshot = {
      id: game.id,
      rated: true,
      pool: game.pool,
      tc: game.tc,
      white: game.white,
      black: game.black,
      status: 'active',
      fen: 'r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQK2R b KQkq - 0 5',
      ply: game.ply,
      version: game.ply,
      clocks: {
        white_ms: 112000,
        black_ms: 98000,
        server_ts: new Date().toISOString(),
      },
      moves: [],
    };

    onSpectateGame(snapshot);
    onClose();
  };

  return (
    <div className="modal-backdrop" onClick={onClose} role="dialog" aria-modal="true">
      <div
        className="modal-card"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: '650px', width: '92%' }}
      >
        <div className="modal-header">
          <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
            <span style={{ fontSize: '1.4rem' }}>👁️</span>
            <div>
              <h2 className="modal-title" style={{ margin: 0 }}>
                Live Spectate Streams
              </h2>
              <span style={{ fontSize: '0.8rem', color: 'var(--text-secondary)' }}>
                Real-time GM and high-tier competitive games
              </span>
            </div>
          </div>
          <button type="button" className="modal-close-btn" onClick={onClose} aria-label={t.close}>
            ✕
          </button>
        </div>

        <div style={{ padding: '1.25rem', display: 'flex', flexDirection: 'column', gap: '0.75rem' }}>
          {games.map((g) => (
            <div
              key={g.id}
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                padding: '0.9rem 1.1rem',
                borderRadius: '8px',
                background: 'var(--bg-card-alt, rgba(255,255,255,0.03))',
                border: '1px solid var(--border-color)',
                transition: 'background 0.15s ease',
              }}
            >
              <div>
                <div style={{ display: 'flex', alignItems: 'center', gap: '0.6rem', marginBottom: '0.25rem' }}>
                  <span style={{ fontWeight: 600, color: 'var(--text-primary)' }}>
                    ⚪ @{g.white.handle} ({g.white.rating})
                  </span>
                  <span style={{ color: 'var(--text-secondary)', fontSize: '0.8rem' }}>vs</span>
                  <span style={{ fontWeight: 600, color: 'var(--text-primary)' }}>
                    ⚫ @{g.black.handle} ({g.black.rating})
                  </span>
                </div>
                <div style={{ fontSize: '0.8rem', color: 'var(--text-secondary)' }}>
                  {g.pool.toUpperCase()} ({g.tc.initial_s / 60}+{g.tc.increment_s}) · Ply {g.ply} · {g.started_at} · 👁️ {g.spectators} watching
                </div>
              </div>

              <button
                type="button"
                className="btn btn-primary btn-sm"
                onClick={() => handleSelectGame(g)}
              >
                Watch Live
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};
