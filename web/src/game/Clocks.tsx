import { useState, useEffect } from 'react';
import { GameClocks, GamePlayerInfo } from '../types';

interface PlayerBarProps {
  player: GamePlayerInfo;
  color: 'white' | 'black';
  isYou: boolean;
  isActive: boolean;
  remainingMs: number;
  capturedPieces?: string[];
  materialAdvantage?: number;
}

export function formatTime(ms: number): string {
  const totalSeconds = Math.max(0, ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = Math.floor(totalSeconds % 60);
  const tenths = Math.floor((ms % 1000) / 100);

  const mStr = minutes.toString().padStart(2, '0');
  const sStr = seconds.toString().padStart(2, '0');

  if (minutes === 0 && seconds < 20) {
    return `${mStr}:${sStr}.${tenths}`;
  }
  return `${mStr}:${sStr}`;
}

export function PlayerBar({
  player,
  color,
  isYou,
  isActive,
  remainingMs,
  capturedPieces = [],
  materialAdvantage = 0,
}: PlayerBarProps) {
  const isLowTime = remainingMs < 20000;

  return (
    <div
      className={`player-bar ${isActive ? 'player-bar-active' : ''}`}
      data-color={color}
    >
      <div className="player-meta">
        <div className={`player-avatar-badge ${color === 'white' ? 'avatar-white' : 'avatar-black'}`}>
          {color === 'white' ? '♔' : '♚'}
        </div>
        <div className="player-text">
          <div className="player-name-row">
            <span className="player-handle">{player.handle}</span>
            {isYou && <span className="player-you-tag">You</span>}
          </div>
          <div className="player-sub-row">
            <span className="player-rating tabular-nums">{player.rating}</span>
            {capturedPieces.length > 0 && (
              <span className="captured-pieces-list">
                {capturedPieces.map((p, i) => (
                  <span key={i} className="captured-glyph">{p}</span>
                ))}
                {materialAdvantage > 0 && (
                  <span className="material-advantage tabular-nums">+{materialAdvantage}</span>
                )}
              </span>
            )}
          </div>
        </div>
      </div>

      <div
        className={`clock-timer tabular-nums ${isActive ? 'clock-active' : ''} ${isLowTime ? 'clock-low' : ''}`}
        aria-label={`${player.handle} clock: ${formatTime(remainingMs)}`}
      >
        {formatTime(remainingMs)}
      </div>
    </div>
  );
}

interface ClocksManagerProps {
  clocks: GameClocks;
  activeColor: 'white' | 'black';
  isPaused?: boolean;
  onTimeout?: (timedOutColor: 'white' | 'black') => void;
  children: (remaining: { whiteMs: number; blackMs: number }) => React.ReactNode;
}

export function ClocksManager({
  clocks,
  activeColor,
  isPaused = false,
  onTimeout,
  children,
}: ClocksManagerProps) {
  const [whiteRemaining, setWhiteRemaining] = useState(clocks.white_ms);
  const [blackRemaining, setBlackRemaining] = useState(clocks.black_ms);

  useEffect(() => {
    if (isPaused) {
      setWhiteRemaining(clocks.white_ms);
      setBlackRemaining(clocks.black_ms);
      return;
    }

    const anchorTime = new Date(clocks.server_ts).getTime();

    const interval = setInterval(() => {
      const now = Date.now();
      const elapsedSinceAnchor = Math.max(0, now - anchorTime);

      if (activeColor === 'white') {
        const remaining = Math.max(0, clocks.white_ms - elapsedSinceAnchor);
        setWhiteRemaining(remaining);
        setBlackRemaining(clocks.black_ms);
        if (remaining <= 0 && onTimeout) {
          onTimeout('white');
        }
      } else {
        const remaining = Math.max(0, clocks.black_ms - elapsedSinceAnchor);
        setBlackRemaining(remaining);
        setWhiteRemaining(clocks.white_ms);
        if (remaining <= 0 && onTimeout) {
          onTimeout('black');
        }
      }
    }, 100);

    return () => clearInterval(interval);
  }, [clocks, activeColor, isPaused, onTimeout]);

  return <>{children({ whiteMs: whiteRemaining, blackMs: blackRemaining })}</>;
}
