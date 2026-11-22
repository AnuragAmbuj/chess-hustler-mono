import { useState } from 'react';

interface GameControlsProps {
  plyCount: number;
  isDrawOffered: boolean;
  canClaimTimeout?: boolean;
  onResign: () => void;
  onAbort: () => void;
  onOfferDraw: () => void;
  onClaimTimeout?: () => void;
  disabled?: boolean;
}

export function GameControls({
  plyCount,
  isDrawOffered,
  canClaimTimeout = false,
  onResign,
  onAbort,
  onOfferDraw,
  onClaimTimeout,
  disabled = false,
}: GameControlsProps) {
  const [confirmingResign, setConfirmingResign] = useState(false);

  // Per REST spec §2.3: Abort valid ≤ 4 plies
  const canAbort = plyCount <= 4;

  const handleResignClick = () => {
    if (confirmingResign) {
      onResign();
      setConfirmingResign(false);
    } else {
      setConfirmingResign(true);
      setTimeout(() => setConfirmingResign(false), 4000);
    }
  };

  return (
    <div className="game-action-toolbar">
      {canClaimTimeout && onClaimTimeout && (
        <button
          type="button"
          className="btn btn-accent btn-sm"
          onClick={onClaimTimeout}
          disabled={disabled}
        >
          Claim Timeout
        </button>
      )}

      {canAbort ? (
        <button
          type="button"
          className="btn btn-secondary btn-sm"
          onClick={onAbort}
          disabled={disabled}
          title="Abort match without rating change (first 4 plies only)"
        >
          Abort
        </button>
      ) : (
        <button
          type="button"
          className="btn btn-secondary btn-sm"
          onClick={onOfferDraw}
          disabled={disabled || isDrawOffered}
        >
          {isDrawOffered ? 'Draw Offered' : 'Offer Draw'}
        </button>
      )}

      <button
        type="button"
        className={`btn btn-sm ${confirmingResign ? 'btn-danger' : 'btn-ghost'}`}
        onClick={handleResignClick}
        disabled={disabled}
      >
        {confirmingResign ? 'Confirm Resign?' : 'Resign'}
      </button>
    </div>
  );
}
