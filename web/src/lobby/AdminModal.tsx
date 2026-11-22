import { useState, useEffect, useCallback } from 'react';
import { useAuth } from '../auth/useAuth';
import { api } from '../api/client';

interface AnomalyFlag {
  id: number;
  kind: string;
  subject_user_id: number;
  subject_handle?: string;
  related_user_id?: number;
  related_handle?: string;
  score: number;
  evidence: Record<string, any>;
  status: string;
  reviewed_by?: string;
  reviewed_at?: string;
  created_at: string;
}

interface ReportItem {
  id: number;
  reporter_id: string;
  category: string;
  target_handle: string;
  game_id?: string;
  note?: string;
  status: string;
  created_at: string;
}

interface SanctionItem {
  id: number;
  user_id: number;
  handle?: string;
  kind: string;
  reason: string;
  issued_by: string;
  evidence?: Record<string, any>;
  created_at: string;
  expires_at?: string;
}

interface PlayerAdminDetail {
  id: number;
  public_id: string;
  handle: string;
  display_name: string;
  role: string;
  status: string;
  phone_e164?: string;
  email?: string;
  created_at: string;
  gold_balance: number;
  ratings: Record<string, number>;
  active_sanctions: SanctionItem[];
  anomaly_flags: AnomalyFlag[];
  reports_received: number;
}

interface AuditItem {
  id: number;
  actor: string;
  action: string;
  target_type?: string;
  target_id?: string;
  payload: Record<string, any>;
  created_at: string;
}

interface AdminModalProps {
  isOpen: boolean;
  onClose: () => void;
}

type AdminTab = 'flags' | 'reports' | 'player' | 'audit';

export function AdminModal({ isOpen, onClose }: AdminModalProps) {
  const { user } = useAuth();
  const token = api.getToken() || localStorage.getItem('hustle_access_token');
  const [activeTab, setActiveTab] = useState<AdminTab>('flags');

  // Flags state
  const [flags, setFlags] = useState<AnomalyFlag[]>([]);
  const [flagFilter, setFlagFilter] = useState<'open' | 'confirmed' | 'cleared' | ''>('open');
  const [resolvingFlagId, setResolvingFlagId] = useState<number | null>(null);

  // Reports state
  const [reports, setReports] = useState<ReportItem[]>([]);
  const [reportFilter, setReportFilter] = useState<'open' | 'actioned' | 'dismissed' | ''>('open');

  // Player inspection state
  const [searchHandle, setSearchHandle] = useState('');
  const [inspectedPlayer, setInspectedPlayer] = useState<PlayerAdminDetail | null>(null);
  const [sanctionKind, setSanctionKind] = useState('warning');
  const [sanctionReason, setSanctionReason] = useState('');

  // Audit state
  const [auditLogs, setAuditLogs] = useState<AuditItem[]>([]);

  // Feedback & Loading
  const [loading, setLoading] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [successMsg, setSuccessMsg] = useState<string | null>(null);

  const clearMessages = () => {
    setErrorMsg(null);
    setSuccessMsg(null);
  };

  // Fetch flags
  const fetchFlags = useCallback(async () => {
    if (!token) return;
    setLoading(true);
    try {
      const url = flagFilter ? `/v1/admin/flags?status=${flagFilter}` : '/v1/admin/flags';
      const res = await fetch(url, {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (res.ok) {
        const data = await res.json();
        setFlags(data);
      } else if (res.status === 403) {
        setErrorMsg('Role required: admin or moderator');
      }
    } catch (err: any) {
      setErrorMsg(err.message);
    } finally {
      setLoading(false);
    }
  }, [token, flagFilter]);

  // Fetch reports
  const fetchReports = useCallback(async () => {
    if (!token) return;
    setLoading(true);
    try {
      const url = reportFilter ? `/v1/admin/reports?status=${reportFilter}` : '/v1/admin/reports';
      const res = await fetch(url, {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (res.ok) {
        const data = await res.json();
        setReports(data);
      }
    } catch (err: any) {
      setErrorMsg(err.message);
    } finally {
      setLoading(false);
    }
  }, [token, reportFilter]);

  // Fetch audit logs
  const fetchAuditLogs = useCallback(async () => {
    if (!token) return;
    setLoading(true);
    try {
      const res = await fetch('/v1/admin/audit?limit=30', {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (res.ok) {
        const data = await res.json();
        setAuditLogs(data);
      }
    } catch (err: any) {
      setErrorMsg(err.message);
    } finally {
      setLoading(false);
    }
  }, [token]);

  // Inspect Player
  const inspectPlayer = async (target: string) => {
    if (!token || !target.trim()) return;
    setLoading(true);
    clearMessages();
    try {
      const res = await fetch(`/v1/admin/players/${encodeURIComponent(target.trim())}`, {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (res.ok) {
        const data = await res.json();
        setInspectedPlayer(data);
      } else {
        setErrorMsg(`Player "${target}" not found`);
        setInspectedPlayer(null);
      }
    } catch (err: any) {
      setErrorMsg(err.message);
    } finally {
      setLoading(false);
    }
  };

  // Resolve Flag
  const handleResolveFlag = async (flagId: number, resolution: 'cleared' | 'confirmed') => {
    if (!token) return;
    setResolvingFlagId(flagId);
    clearMessages();
    try {
      const res = await fetch(`/v1/admin/flags/${flagId}/resolve`, {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${token}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          resolution,
          notes: `Resolved by ${user?.handle || 'admin'}`,
        }),
      });
      if (res.ok) {
        setSuccessMsg(`Flag #${flagId} marked as ${resolution}`);
        fetchFlags();
      } else {
        const err = await res.json();
        setErrorMsg(err.error?.message || 'Failed to resolve flag');
      }
    } catch (err: any) {
      setErrorMsg(err.message);
    } finally {
      setResolvingFlagId(null);
    }
  };

  // Action Report
  const handleActionReport = async (reportId: number, action: 'actioned' | 'dismissed') => {
    if (!token) return;
    clearMessages();
    try {
      const res = await fetch(`/v1/admin/reports/${reportId}/action`, {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${token}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          action,
          resolution: { decision: action, actor: user?.handle },
        }),
      });
      if (res.ok) {
        setSuccessMsg(`Report #${reportId} set to ${action}`);
        fetchReports();
      } else {
        const err = await res.json();
        setErrorMsg(err.error?.message || 'Failed to update report');
      }
    } catch (err: any) {
      setErrorMsg(err.message);
    }
  };

  // Issue Sanction
  const handleApplySanction = async () => {
    if (!token || !inspectedPlayer || !sanctionReason.trim()) return;
    setLoading(true);
    clearMessages();
    try {
      const res = await fetch(`/v1/admin/players/${inspectedPlayer.id}/sanctions`, {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${token}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          kind: sanctionKind,
          reason: sanctionReason.trim(),
        }),
      });
      if (res.ok) {
        setSuccessMsg(`Sanction applied to @${inspectedPlayer.handle}`);
        setSanctionReason('');
        inspectPlayer(inspectedPlayer.handle);
      } else {
        const err = await res.json();
        setErrorMsg(err.error?.message || 'Failed to apply sanction');
      }
    } catch (err: any) {
      setErrorMsg(err.message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (!isOpen) return;
    clearMessages();
    if (activeTab === 'flags') fetchFlags();
    if (activeTab === 'reports') fetchReports();
    if (activeTab === 'audit') fetchAuditLogs();
  }, [isOpen, activeTab, fetchFlags, fetchReports, fetchAuditLogs]);

  if (!isOpen) return null;

  return (
    <div className="modal-backdrop" onClick={onClose} role="dialog" aria-modal="true">
      <div
        className="modal-container admin-modal-container"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: '840px', width: '95%' }}
      >
        <div className="modal-header">
          <div className="modal-title-wrap">
            <span className="modal-title-icon">🛡️</span>
            <div>
              <h2 className="modal-title">Moderation & Anti-Cheat Console</h2>
              <p className="modal-subtitle">
                Layer 2 statistical findings & Layer 3 human review (ADR-014)
              </p>
            </div>
          </div>
          <button
            type="button"
            className="modal-close-btn"
            onClick={onClose}
            aria-label="Close"
          >
            ✕
          </button>
        </div>

        {/* Tab navigation */}
        <div className="tabs-bar">
          <button
            type="button"
            className={`tab-btn ${activeTab === 'flags' ? 'active' : ''}`}
            onClick={() => setActiveTab('flags')}
          >
            <span className="tab-icon">⚡</span>
            Anomaly Flags
          </button>
          <button
            type="button"
            className={`tab-btn ${activeTab === 'reports' ? 'active' : ''}`}
            onClick={() => setActiveTab('reports')}
          >
            <span className="tab-icon">🚩</span>
            User Reports
          </button>
          <button
            type="button"
            className={`tab-btn ${activeTab === 'player' ? 'active' : ''}`}
            onClick={() => setActiveTab('player')}
          >
            <span className="tab-icon">🔍</span>
            Player Inspection & Sanctions
          </button>
          <button
            type="button"
            className={`tab-btn ${activeTab === 'audit' ? 'active' : ''}`}
            onClick={() => setActiveTab('audit')}
          >
            <span className="tab-icon">📜</span>
            Audit Log
          </button>
        </div>

        {/* Alerts / Feedback */}
        {errorMsg && (
          <div className="alert-banner alert-error" style={{ margin: '1rem 1.5rem 0' }}>
            <span>⚠️</span> {errorMsg}
          </div>
        )}
        {successMsg && (
          <div className="alert-banner alert-success" style={{ margin: '1rem 1.5rem 0' }}>
            <span>✓</span> {successMsg}
          </div>
        )}

        <div className="modal-body" style={{ maxHeight: '65vh', overflowY: 'auto' }}>
          {/* TAB 1: ANOMALY FLAGS */}
          {activeTab === 'flags' && (
            <div className="admin-tab-content">
              <div className="filter-row" style={{ display: 'flex', gap: '0.5rem', marginBottom: '1rem' }}>
                <span style={{ fontSize: '0.85rem', color: 'var(--text-secondary)', alignSelf: 'center' }}>Status:</span>
                {(['open', 'confirmed', 'cleared', ''] as const).map((st) => (
                  <button
                    key={st}
                    type="button"
                    className={`btn btn-sm ${flagFilter === st ? 'btn-primary' : 'btn-ghost'}`}
                    onClick={() => setFlagFilter(st)}
                  >
                    {st === '' ? 'All' : st.toUpperCase()}
                  </button>
                ))}
              </div>

              {loading && flags.length === 0 ? (
                <div className="empty-state">Loading anomaly queue...</div>
              ) : flags.length === 0 ? (
                <div className="empty-state">No anomaly flags matching current filter.</div>
              ) : (
                <div className="flags-list" style={{ display: 'flex', flexDirection: 'column', gap: '0.75rem' }}>
                  {flags.map((flag) => (
                    <div
                      key={flag.id}
                      className="flag-card"
                      style={{
                        padding: '1rem',
                        border: '1px solid var(--border-color)',
                        borderRadius: '0.5rem',
                        background: 'var(--bg-card)',
                      }}
                    >
                      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '0.5rem' }}>
                        <div>
                          <span
                            className="badge"
                            style={{
                              marginRight: '0.5rem',
                              padding: '0.2rem 0.5rem',
                              borderRadius: '0.25rem',
                              background: flag.kind === 'move_time' ? '#f59e0b20' : '#ef444420',
                              color: flag.kind === 'move_time' ? '#d97706' : '#dc2626',
                              fontWeight: 600,
                              fontSize: '0.8rem',
                            }}
                          >
                            {flag.kind.toUpperCase()}
                          </span>
                          <span style={{ fontWeight: 600 }}>
                            Subject: {flag.subject_handle ? `@${flag.subject_handle}` : `User #${flag.subject_user_id}`}
                          </span>
                          {flag.related_user_id && (
                            <span style={{ color: 'var(--text-secondary)', marginLeft: '0.5rem', fontSize: '0.85rem' }}>
                              (vs {flag.related_handle ? `@${flag.related_handle}` : `#${flag.related_user_id}`})
                            </span>
                          )}
                        </div>
                        <div style={{ display: 'flex', gap: '0.5rem', alignItems: 'center' }}>
                          <span style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}>
                            Confidence: <strong>{(flag.score * 100).toFixed(0)}%</strong>
                          </span>
                          <span
                            className="status-pill"
                            style={{
                              fontSize: '0.75rem',
                              padding: '0.15rem 0.4rem',
                              borderRadius: '0.25rem',
                              background: flag.status === 'open' ? '#3b82f620' : '#10b98120',
                              color: flag.status === 'open' ? '#2563eb' : '#059669',
                              fontWeight: 600,
                            }}
                          >
                            {flag.status.toUpperCase()}
                          </span>
                        </div>
                      </div>

                      {/* Evidence JSON inspection */}
                      <div
                        style={{
                          fontSize: '0.8rem',
                          background: 'var(--bg-base)',
                          padding: '0.5rem',
                          borderRadius: '0.35rem',
                          marginBottom: '0.75rem',
                          fontFamily: 'monospace',
                        }}
                      >
                        {Object.entries(flag.evidence).map(([k, v]) => (
                          <div key={k}>
                            <strong>{k}:</strong> {typeof v === 'object' ? JSON.stringify(v) : String(v)}
                          </div>
                        ))}
                      </div>

                      {/* Actions */}
                      {flag.status === 'open' && (
                        <div style={{ display: 'flex', gap: '0.5rem', justifyContent: 'flex-end' }}>
                          <button
                            type="button"
                            className="btn btn-sm btn-ghost"
                            disabled={resolvingFlagId === flag.id}
                            onClick={() => handleResolveFlag(flag.id, 'cleared')}
                          >
                            Dismiss (False Positive)
                          </button>
                          <button
                            type="button"
                            className="btn btn-sm btn-primary"
                            disabled={resolvingFlagId === flag.id}
                            onClick={() => handleResolveFlag(flag.id, 'confirmed')}
                          >
                            Confirm Violation
                          </button>
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* TAB 2: USER REPORTS */}
          {activeTab === 'reports' && (
            <div className="admin-tab-content">
              <div className="filter-row" style={{ display: 'flex', gap: '0.5rem', marginBottom: '1rem' }}>
                <span style={{ fontSize: '0.85rem', color: 'var(--text-secondary)', alignSelf: 'center' }}>Status:</span>
                {(['open', 'actioned', 'dismissed', ''] as const).map((st) => (
                  <button
                    key={st}
                    type="button"
                    className={`btn btn-sm ${reportFilter === st ? 'btn-primary' : 'btn-ghost'}`}
                    onClick={() => setReportFilter(st)}
                  >
                    {st === '' ? 'All' : st.toUpperCase()}
                  </button>
                ))}
              </div>

              {reports.length === 0 ? (
                <div className="empty-state">No user reports found.</div>
              ) : (
                <div className="reports-list" style={{ display: 'flex', flexDirection: 'column', gap: '0.75rem' }}>
                  {reports.map((rep) => (
                    <div
                      key={rep.id}
                      className="report-card"
                      style={{
                        padding: '1rem',
                        border: '1px solid var(--border-color)',
                        borderRadius: '0.5rem',
                        background: 'var(--bg-card)',
                      }}
                    >
                      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '0.5rem' }}>
                        <div>
                          <strong style={{ color: 'var(--text-primary)' }}>@{rep.target_handle}</strong>
                          <span style={{ marginLeft: '0.5rem', fontSize: '0.85rem', color: 'var(--text-secondary)' }}>
                            Category: <strong>{rep.category}</strong>
                          </span>
                        </div>
                        <span style={{ fontSize: '0.8rem', color: 'var(--text-secondary)' }}>
                          Status: <strong>{rep.status}</strong>
                        </span>
                      </div>
                      {rep.note && (
                        <p style={{ fontSize: '0.9rem', color: 'var(--text-secondary)', margin: '0.5rem 0' }}>
                          "{rep.note}"
                        </p>
                      )}
                      {rep.status === 'open' && (
                        <div style={{ display: 'flex', gap: '0.5rem', justifyContent: 'flex-end', marginTop: '0.5rem' }}>
                          <button
                            type="button"
                            className="btn btn-sm btn-ghost"
                            onClick={() => handleActionReport(rep.id, 'dismissed')}
                          >
                            Dismiss
                          </button>
                          <button
                            type="button"
                            className="btn btn-sm btn-primary"
                            onClick={() => handleActionReport(rep.id, 'actioned')}
                          >
                            Action & Resolve
                          </button>
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* TAB 3: PLAYER INSPECTION & SANCTIONS */}
          {activeTab === 'player' && (
            <div className="admin-tab-content">
              <div style={{ display: 'flex', gap: '0.5rem', marginBottom: '1.25rem' }}>
                <input
                  type="text"
                  className="input-field"
                  placeholder="Enter player handle or ID..."
                  value={searchHandle}
                  onChange={(e) => setSearchHandle(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && inspectPlayer(searchHandle)}
                  style={{ flex: 1, padding: '0.5rem', borderRadius: '0.35rem', border: '1px solid var(--border-color)' }}
                />
                <button
                  type="button"
                  className="btn btn-primary btn-sm"
                  onClick={() => inspectPlayer(searchHandle)}
                >
                  Inspect Player
                </button>
              </div>

              {inspectedPlayer && (
                <div className="player-detail-card" style={{ padding: '1rem', border: '1px solid var(--border-color)', borderRadius: '0.5rem', background: 'var(--bg-card)' }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem' }}>
                    <div>
                      <h3 style={{ margin: 0, fontSize: '1.2rem' }}>@{inspectedPlayer.handle}</h3>
                      <span style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}>
                        Status: <strong>{inspectedPlayer.status}</strong> | Role: <strong>{inspectedPlayer.role}</strong>
                      </span>
                    </div>
                    <div style={{ textAlign: 'right' }}>
                      <span style={{ fontSize: '0.9rem', color: 'var(--gold-color)' }}>
                        🪙 {inspectedPlayer.gold_balance.toLocaleString()} Gold
                      </span>
                    </div>
                  </div>

                  <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: '0.5rem', marginBottom: '1rem', background: 'var(--bg-base)', padding: '0.5rem', borderRadius: '0.35rem' }}>
                    {Object.entries(inspectedPlayer.ratings).map(([pool, rating]) => (
                      <div key={pool} style={{ textAlign: 'center' }}>
                        <span style={{ display: 'block', fontSize: '0.75rem', color: 'var(--text-secondary)', textTransform: 'uppercase' }}>{pool}</span>
                        <strong style={{ fontSize: '1rem' }}>{rating}</strong>
                      </div>
                    ))}
                  </div>

                  {/* Active Sanctions */}
                  <h4 style={{ margin: '1rem 0 0.5rem', fontSize: '0.95rem' }}>Active Sanctions ({inspectedPlayer.active_sanctions.length})</h4>
                  {inspectedPlayer.active_sanctions.length === 0 ? (
                    <p style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}>No active sanctions.</p>
                  ) : (
                    <div style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem', marginBottom: '1rem' }}>
                      {inspectedPlayer.active_sanctions.map((s) => (
                        <div key={s.id} style={{ fontSize: '0.85rem', padding: '0.5rem', border: '1px solid var(--border-color)', borderRadius: '0.25rem' }}>
                          <span style={{ fontWeight: 600, color: '#dc2626' }}>{s.kind.toUpperCase()}</span>: {s.reason}
                          <span style={{ display: 'block', fontSize: '0.75rem', color: 'var(--text-secondary)', marginTop: '0.2rem' }}>
                            Issued by {s.issued_by} at {s.created_at}
                          </span>
                        </div>
                      ))}
                    </div>
                  )}

                  {/* Apply Sanction Section */}
                  <div style={{ marginTop: '1.25rem', paddingTop: '1rem', borderTop: '1px solid var(--border-color)' }}>
                    <h4 style={{ margin: '0 0 0.5rem', fontSize: '0.95rem' }}>Apply Sanction</h4>
                    <div style={{ display: 'flex', gap: '0.5rem', marginBottom: '0.5rem' }}>
                      <select
                        className="select-field"
                        value={sanctionKind}
                        onChange={(e) => setSanctionKind(e.target.value)}
                        style={{ padding: '0.4rem', borderRadius: '0.25rem', border: '1px solid var(--border-color)' }}
                      >
                        <option value="warning">1. Formal Warning</option>
                        <option value="mute">2. Chat Mute</option>
                        <option value="ad_suspension">3. Ad-Earning Suspension</option>
                        <option value="mm_suspension">4. Matchmaking Suspension</option>
                        <option value="suspension">5. Account Suspension</option>
                      </select>
                      <input
                        type="text"
                        className="input-field"
                        placeholder="Reason for sanction..."
                        value={sanctionReason}
                        onChange={(e) => setSanctionReason(e.target.value)}
                        style={{ flex: 1, padding: '0.4rem', borderRadius: '0.25rem', border: '1px solid var(--border-color)' }}
                      />
                      <button
                        type="button"
                        className="btn btn-sm btn-primary"
                        onClick={handleApplySanction}
                        disabled={!sanctionReason.trim()}
                      >
                        Apply Sanction
                      </button>
                    </div>
                  </div>
                </div>
              )}
            </div>
          )}

          {/* TAB 4: AUDIT LOG */}
          {activeTab === 'audit' && (
            <div className="admin-tab-content">
              {auditLogs.length === 0 ? (
                <div className="empty-state">No audit log entries recorded.</div>
              ) : (
                <div className="audit-list" style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem' }}>
                  {auditLogs.map((log) => (
                    <div
                      key={log.id}
                      style={{
                        padding: '0.75rem',
                        border: '1px solid var(--border-color)',
                        borderRadius: '0.35rem',
                        background: 'var(--bg-card)',
                        fontSize: '0.85rem',
                      }}
                    >
                      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '0.25rem' }}>
                        <span>
                          <strong>{log.actor}</strong> executed <code>{log.action}</code>
                          {log.target_type && (
                            <span style={{ color: 'var(--text-secondary)', marginLeft: '0.35rem' }}>
                              on {log.target_type}:{log.target_id}
                            </span>
                          )}
                        </span>
                        <span style={{ color: 'var(--text-secondary)', fontSize: '0.75rem' }}>
                          {log.created_at}
                        </span>
                      </div>
                      <div style={{ fontFamily: 'monospace', fontSize: '0.75rem', color: 'var(--text-secondary)' }}>
                        {JSON.stringify(log.payload)}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
