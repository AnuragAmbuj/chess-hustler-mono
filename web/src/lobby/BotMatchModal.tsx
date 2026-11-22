import React, { useState } from 'react';
import { useTranslation } from '../i18n/useTranslation';
import { GameSnapshot, TimeControlConfig } from '../types';

interface BotMatchModalProps {
  isOpen: boolean;
  onClose: () => void;
  onStartBotGame: (snapshot: GameSnapshot, myColor: 'white' | 'black') => void;
  userHandle?: string;
  userRating?: number;
}

export type BotLevel = 'novice' | 'club' | 'master';

interface BotConfig {
  id: BotLevel;
  name: string;
  elo: number;
  depth: number;
  description: string;
  avatar: string;
}

const BOTS: BotConfig[] = [
  {
    id: 'novice',
    name: 'Novice Bot (स्तर 1 - नौसिखिया)',
    elo: 1000,
    depth: 1,
    description: 'Fast, tactical openings with human-like blunders. Perfect for warm-ups.',
    avatar: '🥉',
  },
  {
    id: 'club',
    name: 'Club Grinder (स्तर 2 - क्लब खिलाड़ी)',
    elo: 1500,
    depth: 3,
    description: 'Disciplined piece development, active piece play, solid endgame calculation.',
    avatar: '🥈',
  },
  {
    id: 'master',
    name: 'Hustle Master (स्तर 3 - हसल मास्टर)',
    elo: 2000,
    depth: 4,
    description: 'Deep alpha-beta search with piece-square evaluation and aggressive king safety.',
    avatar: '🥇',
  },
];

export const BotMatchModal: React.FC<BotMatchModalProps> = ({
  isOpen,
  onClose,
  onStartBotGame,
  userHandle = 'Player',
  userRating = 1200,
}) => {
  const { t } = useTranslation();
  const [selectedBot, setSelectedBot] = useState<BotLevel>('club');
  const [chosenColor, setChosenColor] = useState<'white' | 'black' | 'random'>('white');
  const selectedTc: TimeControlConfig = {
    initial_s: 180,
    increment_s: 2,
    pool: 'blitz',
    label: '3+2',
  };

  if (!isOpen) return null;

  const handleStart = () => {
    const defaultBot: BotConfig = {
      id: 'club',
      name: 'Club Grinder (स्तर 2 - क्लब खिलाड़ी)',
      elo: 1500,
      depth: 3,
      description: 'Disciplined piece development, active piece play, solid endgame calculation.',
      avatar: '🥈',
    };
    const bot: BotConfig = BOTS.find((b) => b.id === selectedBot) || defaultBot;
    const actualColor =
      chosenColor === 'random' ? (Math.random() > 0.5 ? 'white' : 'black') : chosenColor;

    const snapshot: GameSnapshot = {
      id: `bot_${selectedBot}_${Date.now().toString(36)}`,
      rated: false,
      pool: selectedTc.pool,
      tc: {
        initial_s: selectedTc.initial_s,
        increment_s: selectedTc.increment_s,
      },
      white:
        actualColor === 'white'
          ? { handle: userHandle, rating: userRating }
          : { handle: `${bot.name} [Bot]`, rating: bot.elo },
      black:
        actualColor === 'black'
          ? { handle: userHandle, rating: userRating }
          : { handle: `${bot.name} [Bot]`, rating: bot.elo },
      status: 'active',
      fen: 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1',
      ply: 0,
      version: 0,
      clocks: {
        white_ms: selectedTc.initial_s * 1000,
        black_ms: selectedTc.initial_s * 1000,
        server_ts: new Date().toISOString(),
      },
      moves: [],
    };

    onStartBotGame(snapshot, actualColor);
    onClose();
  };

  return (
    <div className="modal-backdrop" onClick={onClose} role="dialog" aria-modal="true">
      <div
        className="modal-card"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: '580px', width: '92%' }}
      >
        <div className="modal-header">
          <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
            <span style={{ fontSize: '1.4rem' }}>🤖</span>
            <div>
              <h2 className="modal-title" style={{ margin: 0 }}>
                {t.botDifficultyTitle}
              </h2>
              <span style={{ fontSize: '0.8rem', color: 'var(--text-secondary)' }}>
                Server & Local Practice Engine · Unrated
              </span>
            </div>
          </div>
          <button type="button" className="modal-close-btn" onClick={onClose} aria-label={t.close}>
            ✕
          </button>
        </div>

        <div style={{ padding: '1.25rem', display: 'flex', flexDirection: 'column', gap: '1.25rem' }}>
          {/* Difficulty Tiers */}
          <div style={{ display: 'flex', flexDirection: 'column', gap: '0.65rem' }}>
            {BOTS.map((bot) => (
              <div
                key={bot.id}
                onClick={() => setSelectedBot(bot.id)}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '1rem',
                  padding: '0.85rem 1rem',
                  borderRadius: '10px',
                  border: `2px solid ${selectedBot === bot.id ? 'var(--accent, #3b82f6)' : 'var(--border-color)'}`,
                  background:
                    selectedBot === bot.id
                      ? 'var(--accent-glow, rgba(59,130,246,0.08))'
                      : 'var(--bg-card)',
                  cursor: 'pointer',
                  transition: 'all 0.15s ease',
                }}
              >
                <span style={{ fontSize: '1.8rem' }}>{bot.avatar}</span>
                <div style={{ flex: 1 }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                    <span style={{ fontWeight: 600, color: 'var(--text-primary)' }}>{bot.name}</span>
                    <span
                      style={{
                        fontSize: '0.8rem',
                        fontWeight: 700,
                        color: 'var(--accent, #3b82f6)',
                        background: 'rgba(59,130,246,0.1)',
                        padding: '0.15rem 0.5rem',
                        borderRadius: '4px',
                      }}
                    >
                      ~{bot.elo} ELO
                    </span>
                  </div>
                  <p style={{ margin: '0.25rem 0 0', fontSize: '0.8rem', color: 'var(--text-secondary)' }}>
                    {bot.description}
                  </p>
                </div>
              </div>
            ))}
          </div>

          {/* Color Selection */}
          <div>
            <label style={{ display: 'block', fontSize: '0.85rem', fontWeight: 600, marginBottom: '0.4rem', color: 'var(--text-secondary)' }}>
              Play As:
            </label>
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '0.5rem' }}>
              <button
                type="button"
                className={`btn btn-sm ${chosenColor === 'white' ? 'btn-primary' : 'btn-secondary'}`}
                onClick={() => setChosenColor('white')}
              >
                ⚪ White
              </button>
              <button
                type="button"
                className={`btn btn-sm ${chosenColor === 'black' ? 'btn-primary' : 'btn-secondary'}`}
                onClick={() => setChosenColor('black')}
              >
                ⚫ Black
              </button>
              <button
                type="button"
                className={`btn btn-sm ${chosenColor === 'random' ? 'btn-primary' : 'btn-secondary'}`}
                onClick={() => setChosenColor('random')}
              >
                🎲 Random
              </button>
            </div>
          </div>

          {/* Action Button */}
          <button
            type="button"
            className="btn btn-primary btn-lg"
            onClick={handleStart}
            style={{ width: '100%', marginTop: '0.5rem' }}
          >
            <span>{t.startPractice}</span>
          </button>
        </div>
      </div>
    </div>
  );
};
