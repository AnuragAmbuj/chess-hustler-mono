import React, { useState } from 'react';
import { useTranslation } from '../i18n/useTranslation';
import { TacticalPuzzle } from '../types';

interface DailyPuzzleModalProps {
  isOpen: boolean;
  onClose: () => void;
  onRewardClaimed?: (amount: number) => void;
}

const PUZZLES: TacticalPuzzle[] = [
  {
    id: 'puzzle_01',
    title: "Vishy's Operatic Fork",
    theme: 'Knight Fork',
    difficulty: 'Novice',
    fen: 'r1bqkb1r/pppp1ppp/2n5/4p3/2B1n3/5N2/PPPP1PPP/RNBQK2R w KQkq - 0 4',
    playerColor: 'white',
    moves: ['Bxf7+', 'Kxf7', 'Nxe5+'],
    hint: 'Sacrifice on f7 to draw the king into an exposed file with fork potential.',
    rewardGold: 50,
  },
  {
    id: 'puzzle_02',
    title: 'Chennai Express Back-Rank Mate',
    theme: 'Back-Rank Mate',
    difficulty: 'Intermediate',
    fen: '6k1/5ppp/8/8/8/8/4QPPP/6K1 w - - 0 1',
    playerColor: 'white',
    moves: ['Qe8#'],
    hint: 'Black king is trapped behind pawn shields on the back rank.',
    rewardGold: 50,
  },
  {
    id: 'puzzle_03',
    title: 'Pragg Smothered Mate',
    theme: 'Smothered Mate',
    difficulty: 'Master',
    fen: '6rk/5Npp/8/8/8/8/8/6K1 w - - 0 1',
    playerColor: 'white',
    moves: ['Nh6#'],
    hint: 'The defending king has no escape square because his own pieces surround him.',
    rewardGold: 50,
  },
];

export const DailyPuzzleModal: React.FC<DailyPuzzleModalProps> = ({
  isOpen,
  onClose,
  onRewardClaimed,
}) => {
  const { t } = useTranslation();
  const [currentIdx, setCurrentIdx] = useState(0);
  const [showHint, setShowHint] = useState(false);
  const [solvedPuzzles, setSolvedPuzzles] = useState<Record<string, boolean>>({});
  const [message, setMessage] = useState<string | null>(null);

  if (!isOpen) return null;

  const defaultPuzzle: TacticalPuzzle = PUZZLES[0]!;
  const puzzle: TacticalPuzzle = PUZZLES[currentIdx] || defaultPuzzle;
  const isSolved = !!solvedPuzzles[puzzle.id];

  const handleSimulateSolve = () => {
    if (isSolved) return;
    setSolvedPuzzles((prev) => ({ ...prev, [puzzle.id]: true }));
    setMessage(t.puzzleSuccess);
    if (onRewardClaimed) {
      onRewardClaimed(puzzle.rewardGold);
    }
  };

  const handleNextPuzzle = () => {
    setShowHint(false);
    setMessage(null);
    setCurrentIdx((prev) => (prev + 1) % PUZZLES.length);
  };

  return (
    <div className="modal-backdrop" onClick={onClose} role="dialog" aria-modal="true">
      <div
        className="modal-card"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: '600px', width: '92%' }}
      >
        <div className="modal-header">
          <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
            <span style={{ fontSize: '1.4rem' }}>🧩</span>
            <div>
              <h2 className="modal-title" style={{ margin: 0 }}>
                {t.dailyPuzzleTitle}
              </h2>
              <span style={{ fontSize: '0.8rem', color: 'var(--text-secondary)' }}>
                Puzzle #{currentIdx + 1} of {PUZZLES.length} · +{puzzle.rewardGold} Gold Bounty
              </span>
            </div>
          </div>
          <button type="button" className="modal-close-btn" onClick={onClose} aria-label={t.close}>
            ✕
          </button>
        </div>

        <div style={{ padding: '1.25rem', display: 'flex', flexDirection: 'column', gap: '1rem' }}>
          {/* Puzzle Header Details */}
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              padding: '0.75rem 1rem',
              borderRadius: '8px',
              background: 'var(--bg-card-alt, rgba(255,255,255,0.03))',
              border: '1px solid var(--border-color)',
            }}
          >
            <div>
              <div style={{ fontWeight: 600, color: 'var(--text-primary)' }}>{puzzle.title}</div>
              <div style={{ fontSize: '0.8rem', color: 'var(--text-secondary)' }}>
                Theme: {puzzle.theme} · Play as {puzzle.playerColor === 'white' ? '⚪ White' : '⚫ Black'}
              </div>
            </div>
            <span
              style={{
                fontSize: '0.75rem',
                fontWeight: 600,
                padding: '0.2rem 0.6rem',
                borderRadius: '9999px',
                background:
                  puzzle.difficulty === 'Novice'
                    ? 'rgba(34,197,94,0.15)'
                    : puzzle.difficulty === 'Intermediate'
                    ? 'rgba(234,179,8,0.15)'
                    : 'rgba(239,68,68,0.15)',
                color:
                  puzzle.difficulty === 'Novice'
                    ? '#22c55e'
                    : puzzle.difficulty === 'Intermediate'
                    ? '#eab308'
                    : '#ef4444',
              }}
            >
              {puzzle.difficulty}
            </span>
          </div>

          {/* Graphical Board Simulation */}
          <div
            style={{
              padding: '1.5rem',
              borderRadius: '8px',
              background: 'var(--bg-board-frame, #262421)',
              border: '2px solid var(--border-color)',
              textAlign: 'center',
            }}
          >
            <div style={{ color: '#e2e8f0', fontFamily: 'monospace', fontSize: '0.9rem', marginBottom: '0.75rem' }}>
              FEN: {puzzle.fen}
            </div>
            <div
              style={{
                fontSize: '2rem',
                letterSpacing: '0.5rem',
                margin: '1rem 0',
                userSelect: 'none',
              }}
            >
              ♜ ♞ ♝ ♛ ♚ ♝ ♞ ♜
            </div>
            <div style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}>
              {puzzle.playerColor === 'white' ? 'White to move and win.' : 'Black to move and win.'}
            </div>
          </div>

          {/* Status Message */}
          {message && (
            <div className="banner-alert banner-alert-success" style={{ margin: 0 }}>
              <span>{message}</span>
            </div>
          )}

          {/* Hint */}
          {showHint && (
            <div
              style={{
                padding: '0.75rem 1rem',
                borderRadius: '8px',
                background: 'rgba(59,130,246,0.1)',
                border: '1px solid rgba(59,130,246,0.3)',
                fontSize: '0.85rem',
                color: 'var(--text-primary)',
              }}
            >
              💡 <strong>Hint:</strong> {puzzle.hint}
            </div>
          )}

          {/* Controls */}
          <div style={{ display: 'flex', gap: '0.5rem', marginTop: '0.5rem' }}>
            <button
              type="button"
              className="btn btn-secondary btn-md"
              onClick={() => setShowHint(!showHint)}
              style={{ flex: 1 }}
            >
              {showHint ? 'Hide Hint' : '💡 ' + t.puzzleHint}
            </button>

            {!isSolved ? (
              <button
                type="button"
                className="btn btn-primary btn-md"
                onClick={handleSimulateSolve}
                style={{ flex: 2 }}
              >
                ✓ Solve & Claim +{puzzle.rewardGold} Gold
              </button>
            ) : (
              <button
                type="button"
                className="btn btn-primary btn-md"
                onClick={handleNextPuzzle}
                style={{ flex: 2 }}
              >
                Next Puzzle ➔
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
