import { useEffect, useRef } from 'react';
import { MoveRecord } from '../types';

interface MoveHistoryProps {
  moves: MoveRecord[];
  currentPly: number;
  onSelectPly?: (ply: number) => void;
}

export function MoveHistory({ moves, currentPly, onSelectPly }: MoveHistoryProps) {
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [moves]);

  // Group moves into pairs (White & Black)
  const rows: { turn: number; white?: MoveRecord; black?: MoveRecord }[] = [];
  for (let i = 0; i < moves.length; i += 2) {
    rows.push({
      turn: Math.floor(i / 2) + 1,
      white: moves[i],
      black: moves[i + 1],
    });
  }

  return (
    <div className="move-history-card">
      <div className="move-history-header">
        <span className="move-history-title">Moves</span>
        <span className="move-history-count tabular-nums">{moves.length} plies</span>
      </div>

      <div className="move-history-scroll" ref={scrollRef}>
        {rows.length === 0 ? (
          <div className="move-history-empty">Game begins. White to move.</div>
        ) : (
          <div className="moves-list">
            {rows.map((row) => (
              <div key={row.turn} className="move-row">
                <span className="move-number tabular-nums">{row.turn}.</span>
                <button
                  type="button"
                  className={`move-btn tabular-nums ${
                    row.white && row.white.ply === currentPly ? 'move-btn-current' : ''
                  }`}
                  onClick={() => row.white && onSelectPly?.(row.white.ply)}
                  disabled={!row.white}
                  title={row.white ? `Move ${row.white.ply}: ${row.white.san} (${row.white.uci})` : ''}
                >
                  {row.white?.san || '—'}
                </button>
                <button
                  type="button"
                  className={`move-btn tabular-nums ${
                    row.black && row.black.ply === currentPly ? 'move-btn-current' : ''
                  }`}
                  onClick={() => row.black && onSelectPly?.(row.black.ply)}
                  disabled={!row.black}
                  title={row.black ? `Move ${row.black.ply}: ${row.black.san} (${row.black.uci})` : ''}
                >
                  {row.black?.san || '—'}
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
