import { GameResultPayload, GameSnapshot } from '../types';

interface GameResultModalProps {
  isOpen: boolean;
  result: GameResultPayload | null;
  snapshot: GameSnapshot;
  myColor: 'white' | 'black';
  onRematch?: () => void;
  onReturnToLobby: () => void;
}

export function GameResultModal({
  isOpen,
  result,
  snapshot,
  myColor,
  onRematch,
  onReturnToLobby,
}: GameResultModalProps) {
  if (!isOpen || !result) return null;

  const isWin =
    (result.result === 'white' && myColor === 'white') ||
    (result.result === 'black' && myColor === 'black');
  const isDraw = result.result === 'draw';

  const outcomeTitle = isWin ? 'Victory' : isDraw ? 'Draw' : 'Defeat';

  const myRatingAdjustment =
    myColor === 'white' ? result.ratings?.white : result.ratings?.black;
  const oppRatingAdjustment =
    myColor === 'white' ? result.ratings?.black : result.ratings?.white;

  const myGoldEarned =
    myColor === 'white'
      ? result.gold?.white_earned ?? result.gold?.earned ?? (isWin ? 25 : isDraw ? 10 : 5)
      : result.gold?.black_earned ?? result.gold?.earned ?? (isWin ? 25 : isDraw ? 10 : 5);

  const formatDelta = (delta: number) => {
    return delta > 0 ? `+${delta}` : `${delta}`;
  };

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="result-dialog-title">
      <div className="modal-card result-dialog">
        <div className="result-header">
          <div className="result-badge-wrap">
            <span className={`result-tag ${isWin ? 'tag-win' : isDraw ? 'tag-draw' : 'tag-loss'}`}>
              {outcomeTitle}
            </span>
          </div>
          <h2 id="result-dialog-title" className="result-title">
            {result.termination || 'Game finished'}
          </h2>
          <span className="result-subtitle">
            {snapshot.pool.toUpperCase()} • {snapshot.tc.initial_s / 60}+{snapshot.tc.increment_s} • {snapshot.rated ? 'Rated' : 'Casual'}
          </span>
        </div>

        <div className="result-metrics">
          {/* Gold Earned Chip */}
          <div className="metric-box">
            <span className="metric-label">Gold Earned</span>
            <span className="metric-value metric-gold tabular-nums">+{myGoldEarned} 🪙</span>
          </div>

          {/* Rating Change */}
          <div className="metric-box">
            <span className="metric-label">Your Rating</span>
            {myRatingAdjustment ? (
              <span className="metric-value tabular-nums">
                {myRatingAdjustment.after}
                <span className={`delta-tag ${myRatingAdjustment.delta >= 0 ? 'delta-pos' : 'delta-neg'}`}>
                  {formatDelta(myRatingAdjustment.delta)}
                </span>
              </span>
            ) : (
              <span className="metric-value tabular-nums">
                {myColor === 'white' ? snapshot.white.rating : snapshot.black.rating}
              </span>
            )}
          </div>
        </div>

        {/* Players Summary */}
        <div className="result-players-summary">
          <div className="summary-player-row">
            <span className="summary-role">White</span>
            <span className="summary-name">{snapshot.white.handle}</span>
            <span className="summary-rating tabular-nums">{snapshot.white.rating}</span>
            {oppRatingAdjustment && myColor === 'black' && (
              <span className="summary-delta tabular-nums">{formatDelta(oppRatingAdjustment.delta)}</span>
            )}
          </div>
          <div className="summary-player-row">
            <span className="summary-role">Black</span>
            <span className="summary-name">{snapshot.black.handle}</span>
            <span className="summary-rating tabular-nums">{snapshot.black.rating}</span>
            {oppRatingAdjustment && myColor === 'white' && (
              <span className="summary-delta tabular-nums">{formatDelta(oppRatingAdjustment.delta)}</span>
            )}
          </div>
        </div>

        <div className="modal-actions">
          {onRematch && (
            <button
              type="button"
              className="btn btn-primary"
              onClick={onRematch}
            >
              Play Again
            </button>
          )}
          <button
            type="button"
            className="btn btn-secondary"
            onClick={onReturnToLobby}
          >
            Back to Lobby
          </button>
        </div>
      </div>
    </div>
  );
}
