import React, { useState, useEffect } from 'react';
import { api } from '../api/client';
import { LeaderboardEntry } from '../types';
import { useTranslation } from '../i18n/useTranslation';

interface LeaderboardsModalProps {
  isOpen: boolean;
  onClose: () => void;
  onChallengePlayer?: (handle: string) => void;
}

export const LeaderboardsModal: React.FC<LeaderboardsModalProps> = ({
  isOpen,
  onClose,
  onChallengePlayer,
}) => {
  const { t } = useTranslation();
  const [activePool, setActivePool] = useState<'blitz' | 'bullet' | 'rapid' | 'classical'>('blitz');
  const [entries, setEntries] = useState<LeaderboardEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    setLoading(true);
    setError(null);
    api
      .getLeaderboard(activePool, 50)
      .then((res) => {
        setEntries(res.entries);
      })
      .catch((err) => {
        setError(err.message || 'Failed to load leaderboard');
      })
      .finally(() => {
        setLoading(false);
      });
  }, [isOpen, activePool]);

  if (!isOpen) return null;

  const getRankBadge = (rank: number) => {
    if (rank === 1) return <span style={{ color: '#f59e0b', fontWeight: 'bold' }}>🥇 1</span>;
    if (rank === 2) return <span style={{ color: '#94a3b8', fontWeight: 'bold' }}>🥈 2</span>;
    if (rank === 3) return <span style={{ color: '#b45309', fontWeight: 'bold' }}>🥉 3</span>;
    return <span style={{ color: 'var(--text-secondary)' }}>#{rank}</span>;
  };

  return (
    <div className="modal-backdrop" onClick={onClose} role="dialog" aria-modal="true">
      <div
        className="modal-card"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: '780px', width: '92%', maxHeight: '85vh', display: 'flex', flexDirection: 'column' }}
      >
        <div className="modal-header">
          <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
            <span style={{ fontSize: '1.4rem' }}>🏆</span>
            <div>
              <h2 className="modal-title" style={{ margin: 0 }}>
                {t.leaderboardTitle}
              </h2>
              <span style={{ fontSize: '0.8rem', color: 'var(--text-secondary)' }}>
                Official Glicko-2 ratings · Weekly snapshot
              </span>
            </div>
          </div>
          <button type="button" className="modal-close-btn" onClick={onClose} aria-label={t.close}>
            ✕
          </button>
        </div>

        {/* Pool Selector Tabs */}
        <div
          style={{
            display: 'flex',
            gap: '0.5rem',
            padding: '0.75rem 1.25rem',
            borderBottom: '1px solid var(--border-color)',
            background: 'var(--bg-card-alt, rgba(255,255,255,0.02))',
          }}
        >
          {(['bullet', 'blitz', 'rapid', 'classical'] as const).map((pool) => (
            <button
              key={pool}
              type="button"
              className={`btn btn-sm ${activePool === pool ? 'btn-primary' : 'btn-ghost'}`}
              onClick={() => setActivePool(pool)}
              style={{ textTransform: 'capitalize' }}
            >
              {pool === 'bullet' && '⚡ '}
              {pool === 'blitz' && '🔥 '}
              {pool === 'rapid' && '⏱️ '}
              {pool === 'classical' && '⏳ '}
              {pool}
            </button>
          ))}
        </div>

        {/* Table Content */}
        <div style={{ flex: 1, overflowY: 'auto', padding: '1rem 1.25rem' }}>
          {loading ? (
            <div style={{ textAlign: 'center', padding: '3rem', color: 'var(--text-secondary)' }}>
              Loading leaderboard standings...
            </div>
          ) : error ? (
            <div className="banner-alert banner-alert-error">{error}</div>
          ) : entries.length === 0 ? (
            <div style={{ textAlign: 'center', padding: '3rem', color: 'var(--text-secondary)' }}>
              No rated players found in this pool yet.
            </div>
          ) : (
            <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '0.9rem' }}>
              <thead>
                <tr style={{ borderBottom: '2px solid var(--border-color)', textAlign: 'left', color: 'var(--text-secondary)' }}>
                  <th style={{ padding: '0.6rem 0.5rem', width: '60px' }}>{t.rank}</th>
                  <th style={{ padding: '0.6rem 0.5rem' }}>{t.player}</th>
                  <th style={{ padding: '0.6rem 0.5rem', textAlign: 'right' }}>{t.rating}</th>
                  <th style={{ padding: '0.6rem 0.5rem', textAlign: 'right' }}>{t.peak}</th>
                  <th style={{ padding: '0.6rem 0.5rem', textAlign: 'right' }}>{t.games}</th>
                  <th style={{ padding: '0.6rem 0.5rem', textAlign: 'right' }}>{t.winRate}</th>
                  {onChallengePlayer && <th style={{ padding: '0.6rem 0.5rem', textAlign: 'center', width: '90px' }}>Action</th>}
                </tr>
              </thead>
              <tbody>
                {entries.map((entry) => {
                  const winRate =
                    entry.games_count > 0
                      ? Math.round((entry.win_count / entry.games_count) * 100)
                      : 0;

                  return (
                    <tr
                      key={entry.user_id}
                      style={{
                        borderBottom: '1px solid var(--border-color)',
                        transition: 'background 0.15s ease',
                      }}
                    >
                      <td style={{ padding: '0.65rem 0.5rem' }}>{getRankBadge(entry.rank)}</td>
                      <td style={{ padding: '0.65rem 0.5rem' }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                          <span
                            style={{
                              width: '28px',
                              height: '28px',
                              borderRadius: '50%',
                              background: 'var(--accent-glow, #3b82f620)',
                              color: 'var(--accent, #3b82f6)',
                              display: 'inline-flex',
                              alignItems: 'center',
                              justifyContent: 'center',
                              fontWeight: 600,
                              fontSize: '0.75rem',
                            }}
                          >
                            {entry.handle.slice(0, 2).toUpperCase()}
                          </span>
                          <div>
                            <div style={{ fontWeight: 600, color: 'var(--text-primary)' }}>
                              {entry.display_name}
                            </div>
                            <div style={{ fontSize: '0.75rem', color: 'var(--text-secondary)' }}>
                              @{entry.handle}
                            </div>
                          </div>
                        </div>
                      </td>
                      <td
                        style={{
                          padding: '0.65rem 0.5rem',
                          textAlign: 'right',
                          fontWeight: 700,
                          color: 'var(--accent, #3b82f6)',
                        }}
                      >
                        {entry.rating}
                      </td>
                      <td style={{ padding: '0.65rem 0.5rem', textAlign: 'right', color: 'var(--text-secondary)' }}>
                        {entry.peak_rating}
                      </td>
                      <td style={{ padding: '0.65rem 0.5rem', textAlign: 'right', color: 'var(--text-secondary)' }}>
                        {entry.games_count}
                      </td>
                      <td style={{ padding: '0.65rem 0.5rem', textAlign: 'right' }}>
                        <span
                          style={{
                            padding: '0.15rem 0.4rem',
                            borderRadius: '4px',
                            background: winRate >= 60 ? 'rgba(34, 197, 94, 0.15)' : 'rgba(255,255,255,0.05)',
                            color: winRate >= 60 ? '#22c55e' : 'inherit',
                            fontWeight: 500,
                          }}
                        >
                          {winRate}%
                        </span>
                      </td>
                      {onChallengePlayer && (
                        <td style={{ padding: '0.65rem 0.5rem', textAlign: 'center' }}>
                          <button
                            type="button"
                            className="btn btn-ghost btn-sm"
                            onClick={() => onChallengePlayer(entry.handle)}
                            title={`Challenge @${entry.handle}`}
                            style={{ padding: '0.2rem 0.5rem', fontSize: '0.75rem' }}
                          >
                            ⚔️
                          </button>
                        </td>
                      )}
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
};
