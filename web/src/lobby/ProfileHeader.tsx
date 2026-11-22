import { useState, useEffect } from 'react';
import { useAuth } from '../auth/useAuth';
import { usePieceTheme, PieceTheme } from '../game/pieces';
import { DailyStreakModal } from './DailyStreakModal';
import { useTranslation } from '../i18n/useTranslation';

interface ProfileHeaderProps {
  onOpenFriends?: () => void;
  onOpenCosmetics?: () => void;
  onOpenMissions?: () => void;
  onOpenAdmin?: () => void;
  onOpenTournaments?: () => void;
  onOpenAds?: () => void;
  onOpenLeaderboards?: () => void;
  onOpenPuzzle?: () => void;
  onOpenSpectate?: () => void;
  pendingRequestsCount?: number;
  claimableMissionsCount?: number;
}

export function ProfileHeader({
  onOpenFriends,
  onOpenCosmetics,
  onOpenMissions,
  onOpenAdmin,
  onOpenTournaments,
  onOpenAds,
  onOpenLeaderboards,
  onOpenPuzzle,
  onOpenSpectate,
  pendingRequestsCount,
  claimableMissionsCount,
}: ProfileHeaderProps) {
  const { user, isAuthenticated, openAuthModal, logout } = useAuth();
  const { pieceTheme, setPieceTheme } = usePieceTheme();
  const { lang, setLang, t } = useTranslation();
  const [isStreakModalOpen, setIsStreakModalOpen] = useState(false);
  const [isDark, setIsDark] = useState<boolean>(() => {
    return document.documentElement.getAttribute('data-theme') !== 'light';
  });

  useEffect(() => {
    const savedTheme = localStorage.getItem('ch_theme') || 'dark';
    const isDarkTheme = savedTheme === 'dark';
    setIsDark(isDarkTheme);
    document.documentElement.setAttribute('data-theme', savedTheme);
  }, []);

  const toggleTheme = () => {
    const nextTheme = isDark ? 'light' : 'dark';
    setIsDark(!isDark);
    document.documentElement.setAttribute('data-theme', nextTheme);
    localStorage.setItem('ch_theme', nextTheme);
  };

  const toggleLanguage = () => {
    setLang(lang === 'en' ? 'hi' : 'en');
  };

  const handlePieceThemeChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    setPieceTheme(e.target.value as PieceTheme);
  };

  return (
    <header className="site-header">
      <div className="header-inner">
        <div className="header-brand">
          <div className="brand-symbol">♞</div>
          <div className="brand-text">
            <span className="brand-name">{t.brandName}</span>
            <span className="brand-tagline">{t.brandTagline}</span>
          </div>
        </div>

        <div className="header-actions">
          {/* Language Toggle (English / Hindi) */}
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={toggleLanguage}
            title={lang === 'en' ? 'Switch to Hindi (हिन्दी)' : 'Switch to English'}
            style={{
              fontWeight: 600,
              fontSize: '0.8rem',
              padding: '0.3rem 0.6rem',
              borderRadius: '9999px',
              border: '1px solid var(--border-color)',
            }}
          >
            🌐 {lang === 'en' ? 'हिन्दी' : 'English'}
          </button>

          {/* Piece Set Selector */}
          <div className="piece-theme-picker">
            <span className="picker-label">{t.pieces}:</span>
            <select
              className="piece-theme-select"
              value={pieceTheme}
              onChange={handlePieceThemeChange}
              title="Select chess piece set"
              aria-label="Select chess piece set"
            >
              <option value="merida">Merida (Classic)</option>
              <option value="staunty">Staunty (Modern)</option>
              <option value="cburnett">CBurnett (Standard)</option>
            </select>
          </div>

          {/* Theme Switcher */}
          <button
            type="button"
            className="theme-toggle-btn"
            onClick={toggleTheme}
            title={isDark ? 'Switch to Light Theme' : 'Switch to Dark Theme'}
            aria-label="Toggle color theme"
          >
            {isDark ? `☀️ ${t.lightTheme}` : `🌙 ${t.darkTheme}`}
          </button>

          {/* Rankings / Leaderboard Button */}
          {onOpenLeaderboards && (
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={onOpenLeaderboards}
              title="Top Ranked Players Leaderboard"
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '0.3rem',
                borderRadius: '9999px',
                border: '1px solid var(--border-color)',
              }}
            >
              <span>🏆</span>
              <span>{t.leaderboard}</span>
            </button>
          )}

          {/* Daily Tactical Puzzle Button */}
          {onOpenPuzzle && (
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={onOpenPuzzle}
              title="Daily Tactical Puzzle"
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '0.3rem',
                borderRadius: '9999px',
                border: '1px solid var(--border-color)',
              }}
            >
              <span>🧩</span>
              <span>{t.puzzle}</span>
            </button>
          )}

          {/* Live Spectate Button */}
          {onOpenSpectate && (
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={onOpenSpectate}
              title="Watch Live Grandmaster Games"
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '0.3rem',
                borderRadius: '9999px',
                border: '1px solid var(--border-color)',
              }}
            >
              <span>👁️</span>
              <span>{t.spectate}</span>
            </button>
          )}

          {isAuthenticated && user ? (
            <>
              {/* Daily Streak Button */}
              <button
                type="button"
                className="streak-chip"
                onClick={() => setIsStreakModalOpen(true)}
                title="Daily Streak Rewards"
              >
                <span className="streak-icon">⚡</span>
                <span className="streak-text">{t.streak}</span>
                <span className="streak-badge">Claim</span>
              </button>

              {/* Missions & Achievements Button */}
              {onOpenMissions && (
                <button
                  type="button"
                  className="missions-chip"
                  onClick={onOpenMissions}
                  title="Daily & Weekly Missions"
                >
                  <span className="missions-icon">🎯</span>
                  <span className="missions-text">{t.missions}</span>
                  {claimableMissionsCount && claimableMissionsCount > 0 ? (
                    <span className="missions-badge">{claimableMissionsCount}</span>
                  ) : null}
                </button>
              )}

              {/* Vault / Cosmetics Button */}
              {onOpenCosmetics && (
                <button
                  type="button"
                  className="vault-chip"
                  onClick={onOpenCosmetics}
                  title="Vault & Cosmetics"
                >
                  <span className="vault-icon">🎨</span>
                  <span className="vault-text">{t.vault}</span>
                </button>
              )}

              {/* Friends Button */}
              {onOpenFriends && (
                <button
                  type="button"
                  className="friends-chip"
                  onClick={onOpenFriends}
                  title="Friends & Challenges"
                >
                  <span className="friends-icon">👥</span>
                  <span className="friends-text">{t.friends}</span>
                  {pendingRequestsCount && pendingRequestsCount > 0 ? (
                    <span className="friends-badge">{pendingRequestsCount}</span>
                  ) : null}
                </button>
              )}

              {/* Tournaments Button */}
              {onOpenTournaments && (
                <button
                  type="button"
                  className="tournaments-chip"
                  onClick={onOpenTournaments}
                  title="Swiss Tournaments Arena"
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '0.35rem',
                    padding: '0.35rem 0.65rem',
                    borderRadius: '9999px',
                    border: '1px solid var(--border-color)',
                    background: 'var(--bg-card)',
                    color: 'var(--text-primary)',
                    fontSize: '0.85rem',
                    fontWeight: 500,
                    cursor: 'pointer',
                  }}
                >
                  <span>🏆</span>
                  <span>{t.tourneys}</span>
                </button>
              )}

              {/* Rewarded Ads Button */}
              {onOpenAds && (
                <button
                  type="button"
                  className="ads-chip"
                  onClick={onOpenAds}
                  title="Watch Ads for Free Gold"
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '0.35rem',
                    padding: '0.35rem 0.65rem',
                    borderRadius: '9999px',
                    border: '1px solid var(--border-color)',
                    background: 'var(--bg-card)',
                    color: 'var(--accent)',
                    fontSize: '0.85rem',
                    fontWeight: 500,
                    cursor: 'pointer',
                  }}
                >
                  <span>📺</span>
                  <span>{t.ads}</span>
                </button>
              )}

              {/* Admin / Moderation Console Button */}
              {onOpenAdmin && (
                <button
                  type="button"
                  className="admin-chip"
                  onClick={onOpenAdmin}
                  title="Fair Play & Moderation Console"
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '0.35rem',
                    padding: '0.35rem 0.65rem',
                    borderRadius: '9999px',
                    border: '1px solid var(--border-color)',
                    background: 'var(--bg-card)',
                    color: 'var(--text-primary)',
                    fontSize: '0.85rem',
                    fontWeight: 500,
                    cursor: 'pointer',
                  }}
                >
                  <span>🛡️</span>
                  <span>{t.mod}</span>
                </button>
              )}

              {/* Gold Balance Chip */}
              <div className="gold-chip" title="Gold balance">
                <span className="gold-icon">🪙</span>
                <span className="gold-amount tabular-nums">{user.gold.toLocaleString()}</span>
              </div>

              {/* Profile Chip */}
              <div className="user-profile-chip">
                <div className="user-avatar-circle">
                  {user.handle.slice(0, 2).toUpperCase()}
                </div>
                <div className="user-profile-info">
                  <span className="user-profile-handle">@{user.handle}</span>
                  <span className="user-profile-rating tabular-nums">
                    {user.ratings.blitz} Blitz
                  </span>
                </div>
              </div>

              <button
                type="button"
                className="btn btn-ghost btn-sm"
                onClick={logout}
              >
                {t.signOut}
              </button>
            </>
          ) : (
            <button
              type="button"
              className="btn btn-primary btn-sm"
              onClick={openAuthModal}
            >
              {t.signIn}
            </button>
          )}
        </div>
      </div>

      <DailyStreakModal
        isOpen={isStreakModalOpen}
        onClose={() => setIsStreakModalOpen(false)}
      />
    </header>
  );
}
