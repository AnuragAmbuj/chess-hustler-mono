import { useState } from 'react';
import { api } from '../api/client';

interface ReportModalProps {
  isOpen: boolean;
  onClose: () => void;
  targetHandle: string;
  gameId?: string;
}

const CATEGORIES = [
  { value: 'cheating', label: 'Engine Assistance / Cheating' },
  { value: 'abuse', label: 'Chat Abuse / Harassment' },
  { value: 'inappropriate_name', label: 'Inappropriate Name or Bio' },
  { value: 'other', label: 'Other Fair-Play Violation' },
];

export function ReportModal({
  isOpen,
  onClose,
  targetHandle,
  gameId,
}: ReportModalProps) {
  const [category, setCategory] = useState('cheating');
  const [note, setNote] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [submitted, setSubmitted] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!isOpen) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsSubmitting(true);
    setError(null);

    try {
      await api.submitReport({
        category,
        target_handle: targetHandle,
        game_id: gameId,
        note: note.trim() || undefined,
      });
      setSubmitted(true);
      setTimeout(() => {
        setSubmitted(false);
        setNote('');
        onClose();
      }, 2000);
    } catch (err: any) {
      setError(err.message || 'Failed to submit report');
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={onClose} role="dialog" aria-modal="true">
      <div className="modal-card modal-report" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <div className="modal-title-group">
            <h2 className="modal-title">🚩 Report Player</h2>
            <span className="modal-subtitle">Submit safety or fair-play report for @{targetHandle}</span>
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

        <div className="modal-body report-modal-body">
          {submitted ? (
            <div className="report-success-state">
              <div className="success-icon">✓</div>
              <h3>Report Received</h3>
              <p>
                Our moderation team reviews all flagged games and messages pursuant to India IT Rules 2021. Thank you for keeping the arena fair.
              </p>
            </div>
          ) : (
            <form onSubmit={handleSubmit} className="report-form">
              {error && <div className="banner-alert banner-alert-error">{error}</div>}

              <div className="form-group">
                <label className="form-label">Violation Category</label>
                <div className="category-radio-group">
                  {CATEGORIES.map((cat) => (
                    <label key={cat.value} className="category-radio-item">
                      <input
                        type="radio"
                        name="reportCategory"
                        value={cat.value}
                        checked={category === cat.value}
                        onChange={(e) => setCategory(e.target.value)}
                      />
                      <span className="radio-label-text">{cat.label}</span>
                    </label>
                  ))}
                </div>
              </div>

              <div className="form-group">
                <label className="form-label">Additional Details (Optional, max 500 chars)</label>
                <textarea
                  className="form-textarea"
                  rows={3}
                  maxLength={500}
                  placeholder="Describe suspicious play, moves, or abusive chat messages..."
                  value={note}
                  onChange={(e) => setNote(e.target.value)}
                />
              </div>

              <div className="compliance-notice-box">
                <span>
                  ⚖️ Reviewed under Information Technology (Intermediary Guidelines) Rules, 2021. Grievance officer SLA: 24h ack, 15d disposition.
                </span>
              </div>

              <div className="modal-actions">
                <button
                  type="button"
                  className="btn btn-ghost"
                  onClick={onClose}
                  disabled={isSubmitting}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="btn btn-danger"
                  disabled={isSubmitting}
                >
                  {isSubmitting ? 'Submitting...' : 'Submit Report'}
                </button>
              </div>
            </form>
          )}
        </div>
      </div>
    </div>
  );
}
