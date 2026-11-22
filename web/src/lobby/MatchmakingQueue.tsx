import { useState, useEffect } from 'react';
import { TimeControlConfig } from '../types';

interface MatchmakingQueueProps {
  isQueued: boolean;
  timeControl: TimeControlConfig;
  estWaitSeconds?: number;
  onCancel: () => void;
}

export function MatchmakingQueue({
  isQueued,
  timeControl,
  estWaitSeconds = 8,
  onCancel,
}: MatchmakingQueueProps) {
  const [elapsedSeconds, setElapsedSeconds] = useState(0);

  useEffect(() => {
    if (!isQueued) {
      setElapsedSeconds(0);
      return;
    }

    const timer = setInterval(() => {
      setElapsedSeconds((prev) => prev + 1);
    }, 1000);

    return () => clearInterval(timer);
  }, [isQueued]);

  if (!isQueued) return null;

  const formatTime = (secs: number) => {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  };

  return (
    <div className="queue-overlay" role="dialog" aria-modal="true" aria-labelledby="queue-status-title">
      <div className="queue-modal-card">
        <div className="radar-pulse-container">
          <div className="radar-ring ring-1"></div>
          <div className="radar-ring ring-2"></div>
          <div className="radar-ring ring-3"></div>
          <div className="radar-center">♞</div>
        </div>

        <h3 id="queue-status-title" className="queue-title">
          Searching for Opponent…
        </h3>

        <div className="queue-details">
          <div className="queue-chip">
            <span className="queue-pool">{timeControl.pool.toUpperCase()}</span>
            <span className="queue-tc tabular-nums">{timeControl.label}</span>
          </div>

          <div className="queue-timer-display">
            <span className="timer-label">Time Elapsed</span>
            <span className="timer-value tabular-nums">{formatTime(elapsedSeconds)}</span>
          </div>

          <div className="queue-est">
            Estimated wait: <strong className="tabular-nums">~{estWaitSeconds}s</strong>
          </div>
        </div>

        <div className="queue-actions">
          <button
            type="button"
            className="btn btn-secondary btn-block"
            onClick={onCancel}
          >
            Cancel Search
          </button>
        </div>
      </div>
    </div>
  );
}
