import { useState, useEffect, useCallback } from 'react';
import { api } from '../api/client';
import { Tournament, TournamentDetail, CreateTournamentRequest } from '../types';
import { useAuth } from '../auth/useAuth';

interface TournamentsModalProps {
  isOpen: boolean;
  onClose: () => void;
  onOpenGame?: (gameId: string) => void;
}

export function TournamentsModal({ isOpen, onClose, onOpenGame }: TournamentsModalProps) {
  const { user, isAuthenticated } = useAuth();
  const [activeTab, setActiveTab] = useState<'browse' | 'detail' | 'create'>('browse');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [tournaments, setTournaments] = useState<Tournament[]>([]);
  const [selectedTournamentId, setSelectedTournamentId] = useState<string | null>(null);
  const [tournamentDetail, setTournamentDetail] = useState<TournamentDetail | null>(null);
  const [selectedRound, setSelectedRound] = useState<number>(1);
  const [loading, setLoading] = useState(false);
  const [actionLoading, setActionLoading] = useState(false);
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  // Admin Create Tournament Form State
  const [createName, setCreateName] = useState('');
  const [createInitialS, setCreateInitialS] = useState(180);
  const [createIncrementS, setCreateIncrementS] = useState(2);
  const [createRounds, setCreateRounds] = useState(5);
  const [createRoundMinutes, setCreateRoundMinutes] = useState(15);
  const [createCapacity, setCreateCapacity] = useState(32);
  const [createPrizePool, setCreatePrizePool] = useState(1000);

  const fetchTournaments = useCallback(async () => {
    setLoading(true);
    try {
      const list = await api.getTournaments({
        status: statusFilter === 'all' ? undefined : statusFilter,
      });
      setTournaments(list);
    } catch (err: unknown) {
      console.error('Failed to load tournaments:', err);
    } finally {
      setLoading(false);
    }
  }, [statusFilter]);

  const fetchTournamentDetail = useCallback(async (publicId: string) => {
    setLoading(true);
    try {
      const detail = await api.getTournament(publicId);
      setTournamentDetail(detail);
      // Select the current or latest round
      if (detail.pairings.length > 0) {
        const maxRound = Math.max(...detail.pairings.map((p) => p.round));
        setSelectedRound(maxRound);
      } else {
        setSelectedRound(1);
      }
    } catch (err: unknown) {
      console.error('Failed to load tournament detail:', err);
      setMessage({ type: 'error', text: 'Failed to load tournament details' });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (isOpen) {
      setMessage(null);
      if (selectedTournamentId && activeTab === 'detail') {
        fetchTournamentDetail(selectedTournamentId);
      } else {
        fetchTournaments();
      }
    }
  }, [isOpen, selectedTournamentId, activeTab, fetchTournaments, fetchTournamentDetail]);

  if (!isOpen) return null;

  const handleSelectTournament = (t: Tournament) => {
    setSelectedTournamentId(t.public_id);
    setActiveTab('detail');
    fetchTournamentDetail(t.public_id);
  };

  const handleJoin = async () => {
    if (!tournamentDetail || !isAuthenticated) return;
    setActionLoading(true);
    setMessage(null);
    try {
      await api.joinTournament(tournamentDetail.tournament.public_id);
      setMessage({ type: 'success', text: 'Successfully joined tournament!' });
      await fetchTournamentDetail(tournamentDetail.tournament.public_id);
      await fetchTournaments();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to join tournament';
      setMessage({ type: 'error', text: msg });
    } finally {
      setActionLoading(false);
    }
  };

  const handleWithdraw = async () => {
    if (!tournamentDetail || !isAuthenticated) return;
    setActionLoading(true);
    setMessage(null);
    try {
      await api.withdrawTournament(tournamentDetail.tournament.public_id);
      setMessage({ type: 'success', text: 'Withdrawn from tournament' });
      await fetchTournamentDetail(tournamentDetail.tournament.public_id);
      await fetchTournaments();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to withdraw';
      setMessage({ type: 'error', text: msg });
    } finally {
      setActionLoading(false);
    }
  };

  const handleCreateTournament = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!createName.trim()) {
      setMessage({ type: 'error', text: 'Tournament name is required' });
      return;
    }

    setActionLoading(true);
    setMessage(null);
    try {
      const now = new Date();
      const start = new Date(now.getTime() + 15 * 60 * 1000); // starts in 15 mins
      const regClose = new Date(start.getTime() - 2 * 60 * 1000); // closes 2 mins before

      const req: CreateTournamentRequest = {
        name: createName.trim(),
        tc: {
          initial_s: Number(createInitialS),
          increment_s: Number(createIncrementS),
        },
        rounds: Number(createRounds),
        round_minutes: Number(createRoundMinutes),
        capacity: Number(createCapacity),
        starts_at: start.toISOString(),
        registration_closes_at: regClose.toISOString(),
        prize_config: {
          pool_gold: Number(createPrizePool),
          distribution: {
            '1': Math.round(Number(createPrizePool) * 0.5),
            '2': Math.round(Number(createPrizePool) * 0.3),
            '3': Math.round(Number(createPrizePool) * 0.2),
          },
        },
      };

      const created = await api.createTournament(req);
      setMessage({ type: 'success', text: `Tournament "${created.name}" created!` });
      setCreateName('');
      setActiveTab('browse');
      await fetchTournaments();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to create tournament';
      setMessage({ type: 'error', text: msg });
    } finally {
      setActionLoading(false);
    }
  };

  // Filter pairings by round
  const currentRoundPairings =
    tournamentDetail?.pairings.filter((p) => p.round === selectedRound) || [];

  return (
    <div className="modal-backdrop" onClick={onClose} role="presentation">
      <div
        className="modal-card modal-tournaments"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="tournaments-modal-title"
        style={{ maxWidth: '840px', width: '95%' }}
      >
        {/* Header */}
        <div className="modal-header">
          <div className="modal-title-group">
            <span className="modal-title-icon">🏆</span>
            <h2 id="tournaments-modal-title" className="modal-title">
              Tournaments Arena
            </h2>
          </div>
          <button
            type="button"
            className="btn-close"
            onClick={onClose}
            aria-label="Close Tournaments Dialog"
          >
            ✕
          </button>
        </div>

        {/* Tab Bar */}
        <div className="modal-tabs">
          <button
            type="button"
            className={`tab-btn ${activeTab === 'browse' ? 'active' : ''}`}
            onClick={() => {
              setActiveTab('browse');
              setMessage(null);
            }}
          >
            <span>Browse Tournaments</span>
          </button>
          {tournamentDetail && (
            <button
              type="button"
              className={`tab-btn ${activeTab === 'detail' ? 'active' : ''}`}
              onClick={() => {
                setActiveTab('detail');
                setMessage(null);
              }}
            >
              <span>{tournamentDetail.tournament.name}</span>
            </button>
          )}
          {user && (
            <button
              type="button"
              className={`tab-btn ${activeTab === 'create' ? 'active' : ''}`}
              onClick={() => {
                setActiveTab('create');
                setMessage(null);
              }}
            >
              <span>+ Create Tournament</span>
            </button>
          )}
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

        {/* Modal Body */}
        <div className="modal-body-scroll" style={{ padding: '1.25rem' }}>
          {/* TAB 1: BROWSE TOURNAMENTS */}
          {activeTab === 'browse' && (
            <div className="tournaments-browse-wrap">
              {/* Filter Pills */}
              <div
                style={{
                  display: 'flex',
                  gap: '0.5rem',
                  marginBottom: '1rem',
                  alignItems: 'center',
                }}
              >
                <span style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>Status:</span>
                {['all', 'registration', 'active', 'completed'].map((filter) => (
                  <button
                    key={filter}
                    type="button"
                    className={`btn ${
                      statusFilter === filter ? 'btn-primary' : 'btn-ghost'
                    } btn-sm`}
                    style={{ textTransform: 'capitalize' }}
                    onClick={() => setStatusFilter(filter)}
                  >
                    {filter}
                  </button>
                ))}
              </div>

              {loading ? (
                <div className="loading-state" style={{ padding: '2rem', textAlign: 'center' }}>
                  <span>Loading tournaments...</span>
                </div>
              ) : tournaments.length === 0 ? (
                <div
                  style={{
                    padding: '3rem 1rem',
                    textAlign: 'center',
                    color: 'var(--text-muted)',
                    background: 'var(--bg-elevated)',
                    borderRadius: '8px',
                    border: '1px dashed var(--border)',
                  }}
                >
                  <p style={{ fontSize: '1.1rem', marginBottom: '0.5rem' }}>
                    No tournaments found in this category.
                  </p>
                  <p style={{ fontSize: '0.85rem' }}>
                    Scheduled Swiss tournaments with zero entry fees and gold prizes appear here.
                  </p>
                </div>
              ) : (
                <div
                  style={{
                    display: 'grid',
                    gridTemplateColumns: 'repeat(auto-fill, minmax(340px, 1fr))',
                    gap: '1rem',
                  }}
                >
                  {tournaments.map((t) => {
                    const poolLabel = t.tc?.label || `${Math.round(t.tc.initial_s / 60)}+${t.tc.increment_s}`;
                    const isRegOpen = t.status === 'registration';
                    const isActive = t.status === 'active';

                    return (
                      <div
                        key={t.id}
                        className="tournament-card"
                        style={{
                          background: 'var(--bg-elevated)',
                          border: '1px solid var(--border)',
                          borderRadius: '8px',
                          padding: '1rem',
                          display: 'flex',
                          flexDirection: 'column',
                          justifyContent: 'space-between',
                          gap: '0.75rem',
                          cursor: 'pointer',
                          transition: 'border-color 0.15s ease',
                        }}
                        onClick={() => handleSelectTournament(t)}
                      >
                        <div>
                          <div
                            style={{
                              display: 'flex',
                              justifyContent: 'space-between',
                              alignItems: 'center',
                              marginBottom: '0.35rem',
                            }}
                          >
                            <span
                              style={{
                                fontSize: '0.75rem',
                                fontWeight: 600,
                                textTransform: 'uppercase',
                                padding: '0.2rem 0.5rem',
                                borderRadius: '4px',
                                background: isRegOpen
                                  ? 'var(--success-soft)'
                                  : isActive
                                  ? 'var(--accent-soft)'
                                  : 'var(--bg-surface)',
                                color: isRegOpen
                                  ? 'var(--success)'
                                  : isActive
                                  ? 'var(--accent)'
                                  : 'var(--text-muted)',
                              }}
                            >
                              {t.status}
                            </span>
                            <span
                              style={{
                                fontSize: '0.8rem',
                                color: 'var(--accent)',
                                fontWeight: 600,
                              }}
                            >
                              🪙 {t.prize_config?.pool_gold?.toLocaleString() || 1000} Gold Pool
                            </span>
                          </div>

                          <h3
                            style={{
                              margin: '0 0 0.5rem 0',
                              fontSize: '1.05rem',
                              color: 'var(--text-main)',
                            }}
                          >
                            {t.name}
                          </h3>

                          <div
                            style={{
                              fontSize: '0.85rem',
                              color: 'var(--text-muted)',
                              display: 'flex',
                              flexDirection: 'column',
                              gap: '0.25rem',
                            }}
                          >
                            <div>
                              ⏱️ <strong>{poolLabel}</strong> • {t.rounds} Rounds Swiss
                            </div>
                            <div>👥 Max {t.capacity} Players</div>
                            <div>
                              📅 Starts: {new Date(t.starts_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                            </div>
                          </div>
                        </div>

                        <div
                          style={{
                            display: 'flex',
                            justifyContent: 'flex-end',
                            borderTop: '1px solid var(--border)',
                            paddingTop: '0.5rem',
                          }}
                        >
                          <button
                            type="button"
                            className="btn btn-secondary btn-sm"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleSelectTournament(t);
                            }}
                          >
                            View Standings & Boards →
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          )}

          {/* TAB 2: TOURNAMENT DETAIL */}
          {activeTab === 'detail' && tournamentDetail && (
            <div className="tournament-detail-wrap" style={{ display: 'flex', flexDirection: 'column', gap: '1.25rem' }}>
              {/* Back to list button */}
              <div>
                <button
                  type="button"
                  className="btn btn-ghost btn-sm"
                  onClick={() => setActiveTab('browse')}
                >
                  ← Back to tournaments
                </button>
              </div>

              {/* Tournament Info Banner */}
              <div
                style={{
                  background: 'var(--bg-elevated)',
                  border: '1px solid var(--border)',
                  borderRadius: '8px',
                  padding: '1.25rem',
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'flex-start',
                  flexWrap: 'wrap',
                  gap: '1rem',
                }}
              >
                <div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginBottom: '0.35rem' }}>
                    <span
                      style={{
                        fontSize: '0.75rem',
                        fontWeight: 600,
                        textTransform: 'uppercase',
                        padding: '0.2rem 0.5rem',
                        borderRadius: '4px',
                        background:
                          tournamentDetail.tournament.status === 'registration'
                            ? 'var(--success-soft)'
                            : tournamentDetail.tournament.status === 'active'
                            ? 'var(--accent-soft)'
                            : 'var(--bg-surface)',
                        color:
                          tournamentDetail.tournament.status === 'registration'
                            ? 'var(--success)'
                            : tournamentDetail.tournament.status === 'active'
                            ? 'var(--accent)'
                            : 'var(--text-muted)',
                      }}
                    >
                      {tournamentDetail.tournament.status}
                    </span>
                    <span style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>
                      Free Entry (ADR-011 Non-Monetary)
                    </span>
                  </div>

                  <h2 style={{ margin: '0 0 0.5rem 0', fontSize: '1.35rem', color: 'var(--text-main)' }}>
                    {tournamentDetail.tournament.name}
                  </h2>

                  <div style={{ fontSize: '0.85rem', color: 'var(--text-muted)', display: 'flex', gap: '1.25rem', flexWrap: 'wrap' }}>
                    <span>
                      ⏱️ Time Control:{' '}
                      <strong style={{ color: 'var(--text-main)' }}>
                        {tournamentDetail.tournament.tc?.label ||
                          `${Math.round(tournamentDetail.tournament.tc.initial_s / 60)}+${tournamentDetail.tournament.tc.increment_s}`}
                      </strong>
                    </span>
                    <span>
                      🔄 Rounds:{' '}
                      <strong style={{ color: 'var(--text-main)' }}>
                        {tournamentDetail.tournament.rounds}
                      </strong>
                    </span>
                    <span>
                      👥 Registered:{' '}
                      <strong style={{ color: 'var(--text-main)' }}>
                        {tournamentDetail.entries_count} / {tournamentDetail.tournament.capacity}
                      </strong>
                    </span>
                    <span>
                      🪙 Prize Pool:{' '}
                      <strong style={{ color: 'var(--accent)' }}>
                        {tournamentDetail.tournament.prize_config?.pool_gold?.toLocaleString() || 1000} Gold
                      </strong>
                    </span>
                  </div>
                </div>

                {/* Registration Actions */}
                <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-end', gap: '0.5rem' }}>
                  {tournamentDetail.my_entry ? (
                    tournamentDetail.my_entry.withdrawn ? (
                      <span style={{ color: 'var(--danger)', fontSize: '0.85rem' }}>
                        Withdrawn from tournament
                      </span>
                    ) : (
                      <div style={{ display: 'flex', gap: '0.5rem', alignItems: 'center' }}>
                        <span
                          style={{
                            padding: '0.35rem 0.75rem',
                            background: 'var(--success-soft)',
                            color: 'var(--success)',
                            borderRadius: '4px',
                            fontWeight: 600,
                            fontSize: '0.85rem',
                          }}
                        >
                          ✓ Registered
                        </span>
                        {tournamentDetail.tournament.status === 'registration' && (
                          <button
                            type="button"
                            className="btn btn-ghost btn-sm"
                            style={{ color: 'var(--danger)' }}
                            onClick={handleWithdraw}
                            disabled={actionLoading}
                          >
                            Withdraw
                          </button>
                        )}
                      </div>
                    )
                  ) : tournamentDetail.tournament.status === 'registration' ? (
                    <button
                      type="button"
                      className="btn btn-primary"
                      onClick={handleJoin}
                      disabled={actionLoading || tournamentDetail.entries_count >= tournamentDetail.tournament.capacity}
                    >
                      {tournamentDetail.entries_count >= tournamentDetail.tournament.capacity
                        ? 'Capacity Full'
                        : 'Join Free Tournament'}
                    </button>
                  ) : (
                    <span style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>
                      Registration Closed
                    </span>
                  )}
                </div>
              </div>

              {/* Standings Section */}
              <div
                style={{
                  background: 'var(--bg-elevated)',
                  border: '1px solid var(--border)',
                  borderRadius: '8px',
                  padding: '1rem',
                }}
              >
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '0.75rem' }}>
                  <h3 style={{ margin: 0, fontSize: '1rem', color: 'var(--text-main)' }}>
                    📊 Swiss Standings & Tie-Breaks
                  </h3>
                  <span style={{ fontSize: '0.75rem', color: 'var(--text-muted)' }}>
                    Buchholz (BH) • Sonneborn-Berger (SB)
                  </span>
                </div>

                {tournamentDetail.standings.length === 0 ? (
                  <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', margin: 0 }}>
                    Standings will be updated once Round 1 begins.
                  </p>
                ) : (
                  <div style={{ overflowX: 'auto' }}>
                    <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '0.85rem' }}>
                      <thead>
                        <tr style={{ borderBottom: '1px solid var(--border)', color: 'var(--text-muted)', textAlign: 'left' }}>
                          <th style={{ padding: '0.5rem' }}>Rank</th>
                          <th style={{ padding: '0.5rem' }}>Player</th>
                          <th style={{ padding: '0.5rem', textAlign: 'center' }}>Points</th>
                          <th style={{ padding: '0.5rem', textAlign: 'center' }}>Buchholz</th>
                          <th style={{ padding: '0.5rem', textAlign: 'center' }}>SB</th>
                          <th style={{ padding: '0.5rem', textAlign: 'center' }}>W - D - L</th>
                        </tr>
                      </thead>
                      <tbody>
                        {tournamentDetail.standings.map((s) => {
                          const isMe = user?.handle === s.handle;
                          return (
                            <tr
                              key={s.user_id}
                              style={{
                                borderBottom: '1px solid var(--border)',
                                background: isMe ? 'var(--accent-soft)' : 'transparent',
                              }}
                            >
                              <td style={{ padding: '0.5rem', fontWeight: 600 }}>
                                {s.rank === 1 ? '🥇 1' : s.rank === 2 ? '🥈 2' : s.rank === 3 ? '🥉 3' : s.rank}
                              </td>
                              <td style={{ padding: '0.5rem', fontWeight: isMe ? 700 : 500 }}>
                                {s.handle} {isMe && '(You)'}
                              </td>
                              <td style={{ padding: '0.5rem', textAlign: 'center', fontWeight: 700, color: 'var(--accent)' }}>
                                {s.points.toFixed(1)}
                              </td>
                              <td style={{ padding: '0.5rem', textAlign: 'center', color: 'var(--text-muted)' }}>
                                {s.buchholz.toFixed(1)}
                              </td>
                              <td style={{ padding: '0.5rem', textAlign: 'center', color: 'var(--text-muted)' }}>
                                {s.sonneborn_berger.toFixed(1)}
                              </td>
                              <td style={{ padding: '0.5rem', textAlign: 'center', color: 'var(--text-muted)' }}>
                                {s.wins} - {s.draws} - {s.losses}
                              </td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  </div>
                )}
              </div>

              {/* Round Pairings Section */}
              <div
                style={{
                  background: 'var(--bg-elevated)',
                  border: '1px solid var(--border)',
                  borderRadius: '8px',
                  padding: '1rem',
                }}
              >
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '0.75rem' }}>
                  <h3 style={{ margin: 0, fontSize: '1rem', color: 'var(--text-main)' }}>
                    ⚔️ Round Pairings & Boards
                  </h3>
                  {/* Round Selector Buttons */}
                  <div style={{ display: 'flex', gap: '0.35rem' }}>
                    {Array.from({ length: tournamentDetail.tournament.rounds }, (_, i) => i + 1).map((r) => (
                      <button
                        key={r}
                        type="button"
                        className={`btn btn-sm ${selectedRound === r ? 'btn-primary' : 'btn-ghost'}`}
                        onClick={() => setSelectedRound(r)}
                      >
                        Round {r}
                      </button>
                    ))}
                  </div>
                </div>

                {currentRoundPairings.length === 0 ? (
                  <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', margin: 0 }}>
                    Round {selectedRound} pairings have not been generated yet.
                  </p>
                ) : (
                  <div style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem' }}>
                    {currentRoundPairings.map((p) => {
                      const whiteName = p.white_id ? `Player #${p.white_id}` : 'BYE';
                      const blackName = p.black_id ? `Player #${p.black_id}` : 'BYE';
                      const isBye = !p.white_id || !p.black_id;

                      return (
                        <div
                          key={p.id}
                          style={{
                            display: 'flex',
                            justifyContent: 'space-between',
                            alignItems: 'center',
                            padding: '0.65rem 1rem',
                            background: 'var(--bg-surface)',
                            border: '1px solid var(--border)',
                            borderRadius: '6px',
                            fontSize: '0.85rem',
                          }}
                        >
                          <div style={{ display: 'flex', alignItems: 'center', gap: '0.75rem' }}>
                            <span style={{ fontWeight: 600, color: 'var(--text-dim)', minWidth: '45px' }}>
                              Board {p.board}
                            </span>
                            <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                              <span style={{ fontWeight: 600 }}>⚪ {whiteName}</span>
                              <span style={{ color: 'var(--text-dim)' }}>vs</span>
                              <span style={{ fontWeight: 600 }}>⚫ {blackName}</span>
                            </div>
                          </div>

                          <div style={{ display: 'flex', alignItems: 'center', gap: '0.75rem' }}>
                            <span
                              style={{
                                fontWeight: 700,
                                color: p.result ? 'var(--accent)' : 'var(--text-muted)',
                              }}
                            >
                              {p.result || (isBye ? '1 - 0 (Bye)' : 'In Progress')}
                            </span>
                            {p.game_id && onOpenGame && (
                              <button
                                type="button"
                                className="btn btn-ghost btn-sm"
                                onClick={() => onOpenGame(String(p.game_id))}
                              >
                                Watch Game
                              </button>
                            )}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            </div>
          )}

          {/* TAB 3: CREATE TOURNAMENT (ADMIN) */}
          {activeTab === 'create' && (
            <form onSubmit={handleCreateTournament} style={{ display: 'flex', flexDirection: 'column', gap: '1rem' }}>
              <div
                style={{
                  background: 'var(--bg-elevated)',
                  border: '1px solid var(--border)',
                  borderRadius: '8px',
                  padding: '1.25rem',
                  display: 'flex',
                  flexDirection: 'column',
                  gap: '1rem',
                }}
              >
                <h3 style={{ margin: 0, fontSize: '1.1rem', color: 'var(--text-main)' }}>
                  Schedule New Swiss Tournament
                </h3>
                <p style={{ margin: 0, fontSize: '0.85rem', color: 'var(--text-muted)' }}>
                  Per ADR-011, tournaments on Chess Hustle are strictly non-monetary with zero entry fees.
                  Prize pools are sponsored virtual Gold.
                </p>

                <div style={{ display: 'flex', flexDirection: 'column', gap: '0.35rem' }}>
                  <label style={{ fontSize: '0.85rem', fontWeight: 600 }}>Tournament Name</label>
                  <input
                    type="text"
                    className="input-text"
                    placeholder="e.g. Mumbai Blitz Championship"
                    value={createName}
                    onChange={(e) => setCreateName(e.target.value)}
                    required
                  />
                </div>

                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1rem' }}>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: '0.35rem' }}>
                    <label style={{ fontSize: '0.85rem', fontWeight: 600 }}>Initial Time (Seconds)</label>
                    <input
                      type="number"
                      className="input-text"
                      value={createInitialS}
                      onChange={(e) => setCreateInitialS(Number(e.target.value))}
                      min={60}
                      step={60}
                    />
                  </div>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: '0.35rem' }}>
                    <label style={{ fontSize: '0.85rem', fontWeight: 600 }}>Increment (Seconds)</label>
                    <input
                      type="number"
                      className="input-text"
                      value={createIncrementS}
                      onChange={(e) => setCreateIncrementS(Number(e.target.value))}
                      min={0}
                    />
                  </div>
                </div>

                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: '1rem' }}>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: '0.35rem' }}>
                    <label style={{ fontSize: '0.85rem', fontWeight: 600 }}>Rounds</label>
                    <input
                      type="number"
                      className="input-text"
                      value={createRounds}
                      onChange={(e) => setCreateRounds(Number(e.target.value))}
                      min={3}
                      max={11}
                    />
                  </div>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: '0.35rem' }}>
                    <label style={{ fontSize: '0.85rem', fontWeight: 600 }}>Round Minutes</label>
                    <input
                      type="number"
                      className="input-text"
                      value={createRoundMinutes}
                      onChange={(e) => setCreateRoundMinutes(Number(e.target.value))}
                      min={5}
                      max={60}
                    />
                  </div>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: '0.35rem' }}>
                    <label style={{ fontSize: '0.85rem', fontWeight: 600 }}>Max Capacity</label>
                    <input
                      type="number"
                      className="input-text"
                      value={createCapacity}
                      onChange={(e) => setCreateCapacity(Number(e.target.value))}
                      min={4}
                      max={256}
                    />
                  </div>
                </div>

                <div style={{ display: 'flex', flexDirection: 'column', gap: '0.35rem' }}>
                  <label style={{ fontSize: '0.85rem', fontWeight: 600 }}>Sponsored Prize Gold</label>
                  <input
                    type="number"
                    className="input-text"
                    value={createPrizePool}
                    onChange={(e) => setCreatePrizePool(Number(e.target.value))}
                    min={100}
                    step={100}
                  />
                  <span style={{ fontSize: '0.75rem', color: 'var(--text-muted)' }}>
                    Distributed 50% / 30% / 20% to Top 3 Finishers.
                  </span>
                </div>

                <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: '0.5rem' }}>
                  <button
                    type="submit"
                    className="btn btn-primary"
                    disabled={actionLoading || !createName.trim()}
                  >
                    {actionLoading ? 'Creating...' : 'Create & Schedule Tournament'}
                  </button>
                </div>
              </div>
            </form>
          )}
        </div>
      </div>
    </div>
  );
}
