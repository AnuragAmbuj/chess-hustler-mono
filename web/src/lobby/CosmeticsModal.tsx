import { useState, useEffect, useCallback } from 'react';
import { api } from '../api/client';
import { CosmeticItem, CosmeticCategory, UserLoadout } from '../types';
import { useAuth } from '../auth/useAuth';

interface CosmeticsModalProps {
  isOpen: boolean;
  onClose: () => void;
  onLoadoutChange?: (loadout: UserLoadout) => void;
}

const CATEGORY_TABS: { key: string; label: string; icon: string }[] = [
  { key: 'all', label: 'All Items', icon: '✨' },
  { key: 'board', label: 'Boards', icon: '🏁' },
  { key: 'pieces', label: 'Piece Sets', icon: '♟️' },
  { key: 'avatar', label: 'Avatars', icon: '👤' },
  { key: 'frame', label: 'Frames', icon: '🖼️' },
];

const ITEM_NAMES: Record<string, string> = {
  board_classic_green: 'Classic Green',
  pieces_classic_cream: 'Classic Cream',
  board_metro_map: 'Metro Transit Blue',
  board_chaturanga: 'Chaturanga Heritage',
  board_dark_slate: 'Dark Slate',
  pieces_modern_wood: 'Modern Walnut',
  avatar_street_hustler: 'Street Hustler',
  avatar_grandmaster: 'Grandmaster Crown',
  avatar_cyber_king: 'Cyber King',
  frame_bronze: 'Bronze Edge',
  frame_gold_trim: 'Gold Trim',
  frame_neon_edge: 'Neon Cyan Edge',
};

export function CosmeticsModal({ isOpen, onClose, onLoadoutChange }: CosmeticsModalProps) {
  const { user, isAuthenticated, updateGold, refreshProfile } = useAuth();
  const [selectedCategory, setSelectedCategory] = useState<string>('all');
  const [catalog, setCatalog] = useState<CosmeticItem[]>([]);
  const [loadout, setLoadout] = useState<UserLoadout>({});
  const [loading, setLoading] = useState(false);
  const [actionInProgress, setActionInProgress] = useState<string | null>(null);
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  const fetchData = useCallback(async () => {
    setLoading(true);
    try {
      const [items, currentLoadout] = await Promise.all([
        api.getCosmetics(),
        isAuthenticated ? api.getLoadout() : Promise.resolve({}),
      ]);
      setCatalog(items);
      setLoadout(currentLoadout);
      if (onLoadoutChange) {
        onLoadoutChange(currentLoadout);
      }
    } catch (err: unknown) {
      console.error('Failed to load cosmetics:', err);
    } finally {
      setLoading(false);
    }
  }, [isAuthenticated, onLoadoutChange]);

  useEffect(() => {
    if (isOpen) {
      fetchData();
      setMessage(null);
    }
  }, [isOpen, fetchData]);

  if (!isOpen) return null;

  const filteredItems = catalog.filter(
    (item) => selectedCategory === 'all' || item.category === selectedCategory
  );

  const isEquipped = (item: CosmeticItem): boolean => {
    switch (item.category) {
      case 'board':
        return loadout.board_id === item.id;
      case 'pieces':
        return loadout.pieces_id === item.id;
      case 'avatar':
        return loadout.avatar_id === item.id;
      case 'frame':
        return loadout.frame_id === item.id;
      default:
        return false;
    }
  };

  const handlePurchase = async (item: CosmeticItem) => {
    if (!isAuthenticated) return;
    if ((user?.gold ?? 0) < item.price_gold) {
      setMessage({ type: 'error', text: `Insufficient Gold: need ${item.price_gold.toLocaleString()} 🪙` });
      return;
    }

    setActionInProgress(item.id);
    setMessage(null);
    try {
      const idemKey = `buy_${item.id}_${Date.now()}`;
      const res = await api.purchaseCosmetic(item.id, idemKey);
      updateGold(res.balance);
      setMessage({
        type: 'success',
        text: `Unlocked "${ITEM_NAMES[item.id] || item.id}"! Remaining: ${res.balance.toLocaleString()} 🪙`,
      });
      await fetchData();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Purchase failed';
      setMessage({ type: 'error', text: msg });
    } finally {
      setActionInProgress(null);
    }
  };

  const handleEquip = async (item: CosmeticItem) => {
    if (!isAuthenticated) return;
    setActionInProgress(item.id);
    setMessage(null);
    try {
      const patch: Record<string, string | null> = {};
      switch (item.category) {
        case 'board':
          patch.board_id = item.id;
          break;
        case 'pieces':
          patch.pieces_id = item.id;
          break;
        case 'avatar':
          patch.avatar_id = item.id;
          break;
        case 'frame':
          patch.frame_id = item.id;
          break;
      }
      const updated = await api.updateLoadout(patch);
      setLoadout(updated);
      if (onLoadoutChange) {
        onLoadoutChange(updated);
      }
      if (item.category === 'avatar') {
        await refreshProfile();
      }
      setMessage({
        type: 'success',
        text: `Equipped "${ITEM_NAMES[item.id] || item.id}"`,
      });
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Equip failed';
      setMessage({ type: 'error', text: msg });
    } finally {
      setActionInProgress(null);
    }
  };

  const handleUnequip = async (category: CosmeticCategory) => {
    if (!isAuthenticated) return;
    setActionInProgress(`unequip_${category}`);
    setMessage(null);
    try {
      const patch: Record<string, string | null> = {};
      switch (category) {
        case 'board':
          patch.board_id = '';
          break;
        case 'pieces':
          patch.pieces_id = '';
          break;
        case 'avatar':
          patch.avatar_id = '';
          break;
        case 'frame':
          patch.frame_id = '';
          break;
      }
      const updated = await api.updateLoadout(patch);
      setLoadout(updated);
      if (onLoadoutChange) {
        onLoadoutChange(updated);
      }
      if (category === 'avatar') {
        await refreshProfile();
      }
      setMessage({ type: 'success', text: `Unequipped ${category} slot` });
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Unequip failed';
      setMessage({ type: 'error', text: msg });
    } finally {
      setActionInProgress(null);
    }
  };

  const renderItemPreview = (item: CosmeticItem) => {
    if (item.category === 'board') {
      let lightBg = '#ececd0';
      let darkBg = '#739552';
      if (item.id === 'board_metro_map') {
        lightBg = '#e0f2fe';
        darkBg = '#0284c7';
      } else if (item.id === 'board_chaturanga') {
        lightBg = '#fef3c7';
        darkBg = '#b45309';
      } else if (item.id === 'board_dark_slate') {
        lightBg = '#cbd5e1';
        darkBg = '#475569';
      }

      return (
        <div className="cosmetic-preview-board">
          <div className="mini-board-grid">
            {[0, 1, 2, 3].map((r) =>
              [0, 1, 2, 3].map((c) => {
                const isLight = (r + c) % 2 === 0;
                return (
                  <div
                    key={`${r}-${c}`}
                    className="mini-board-square"
                    style={{ backgroundColor: isLight ? lightBg : darkBg }}
                  />
                );
              })
            )}
          </div>
        </div>
      );
    }

    if (item.category === 'pieces') {
      const isWood = item.id.includes('wood');
      return (
        <div className="cosmetic-preview-pieces">
          <span className="sample-piece" style={{ color: isWood ? '#b45309' : '#e2e8f0' }}>
            ♚
          </span>
          <span className="sample-piece" style={{ color: isWood ? '#78350f' : '#64748b' }}>
            ♞
          </span>
        </div>
      );
    }

    if (item.category === 'avatar') {
      const previewObj = item.preview as { icon?: string; badge?: string };
      const icon = previewObj?.icon || '♟️';
      return (
        <div className="cosmetic-preview-avatar">
          <div className="avatar-icon-circle">{icon}</div>
          {previewObj?.badge && <span className="avatar-badge-label">{previewObj.badge}</span>}
        </div>
      );
    }

    if (item.category === 'frame') {
      const previewObj = item.preview as { border_color?: string };
      const borderColor = previewObj?.border_color || '#d97706';
      return (
        <div className="cosmetic-preview-frame">
          <div className="sample-frame-box" style={{ borderColor }}>
            <span className="sample-frame-avatar">👤</span>
          </div>
        </div>
      );
    }

    return null;
  };

  return (
    <div className="modal-backdrop" onClick={onClose} role="dialog" aria-modal="true">
      <div className="modal-card modal-lg" onClick={(e) => e.stopPropagation()}>
        {/* Header */}
        <div className="modal-header">
          <div className="header-left">
            <h2 className="modal-title">🎨 Vault & Cosmetics</h2>
            <span className="modal-subtitle">Customize boards, pieces, avatars & frames with earned Gold</span>
          </div>
          <div className="header-right-group">
            {isAuthenticated && user && (
              <div className="vault-gold-chip" title="Current balance">
                <span className="gold-icon">🪙</span>
                <span className="gold-amount tabular-nums">{user.gold.toLocaleString()}</span>
              </div>
            )}
            <button
              type="button"
              className="modal-close-btn"
              onClick={onClose}
              aria-label="Close modal"
            >
              ✕
            </button>
          </div>
        </div>

        {/* Status Message */}
        {message && (
          <div
            className={`banner-alert ${
              message.type === 'success' ? 'banner-alert-success' : 'banner-alert-error'
            }`}
          >
            <span>{message.text}</span>
          </div>
        )}

        {/* Loadout Summary Bar */}
        {isAuthenticated && (
          <div className="loadout-summary-bar">
            <span className="loadout-label">Active Loadout:</span>
            <div className="loadout-chips">
              <div className="loadout-chip">
                <span className="chip-type">Board:</span>
                <span className="chip-name">
                  {loadout.board_id ? ITEM_NAMES[loadout.board_id] || loadout.board_id : 'Default'}
                </span>
                {loadout.board_id && (
                  <button
                    type="button"
                    className="chip-remove-btn"
                    onClick={() => handleUnequip('board')}
                    title="Reset to default board"
                  >
                    ×
                  </button>
                )}
              </div>
              <div className="loadout-chip">
                <span className="chip-type">Pieces:</span>
                <span className="chip-name">
                  {loadout.pieces_id ? ITEM_NAMES[loadout.pieces_id] || loadout.pieces_id : 'Default'}
                </span>
                {loadout.pieces_id && (
                  <button
                    type="button"
                    className="chip-remove-btn"
                    onClick={() => handleUnequip('pieces')}
                    title="Reset to default pieces"
                  >
                    ×
                  </button>
                )}
              </div>
              <div className="loadout-chip">
                <span className="chip-type">Avatar:</span>
                <span className="chip-name">
                  {loadout.avatar_id ? ITEM_NAMES[loadout.avatar_id] || loadout.avatar_id : 'Default'}
                </span>
                {loadout.avatar_id && (
                  <button
                    type="button"
                    className="chip-remove-btn"
                    onClick={() => handleUnequip('avatar')}
                    title="Reset to default avatar"
                  >
                    ×
                  </button>
                )}
              </div>
              <div className="loadout-chip">
                <span className="chip-type">Frame:</span>
                <span className="chip-name">
                  {loadout.frame_id ? ITEM_NAMES[loadout.frame_id] || loadout.frame_id : 'Default'}
                </span>
                {loadout.frame_id && (
                  <button
                    type="button"
                    className="chip-remove-btn"
                    onClick={() => handleUnequip('frame')}
                    title="Reset to default frame"
                  >
                    ×
                  </button>
                )}
              </div>
            </div>
          </div>
        )}

        {/* Category Tabs */}
        <div className="cosmetics-tab-nav" role="tablist">
          {CATEGORY_TABS.map((tab) => (
            <button
              key={tab.key}
              type="button"
              role="tab"
              aria-selected={selectedCategory === tab.key}
              className={`cosmetics-tab ${selectedCategory === tab.key ? 'active' : ''}`}
              onClick={() => setSelectedCategory(tab.key)}
            >
              <span>{tab.icon}</span>
              <span>{tab.label}</span>
            </button>
          ))}
        </div>

        {/* Item Cards Grid */}
        <div className="modal-body-scroll">
          {loading ? (
            <div className="loading-state">
              <span className="loading-spinner">⌛</span>
              <span>Loading Vault Catalog...</span>
            </div>
          ) : filteredItems.length === 0 ? (
            <div className="empty-state">
              <span>No items found in this category.</span>
            </div>
          ) : (
            <div className="cosmetics-grid">
              {filteredItems.map((item) => {
                const equipped = isEquipped(item);
                const canAfford = (user?.gold ?? 0) >= item.price_gold;
                const busy = actionInProgress === item.id;

                return (
                  <div
                    key={item.id}
                    className={`cosmetic-card ${equipped ? 'equipped' : ''} ${
                      item.owned ? 'owned' : ''
                    }`}
                  >
                    {/* Preview box */}
                    <div className="card-preview-area">{renderItemPreview(item)}</div>

                    {/* Meta info */}
                    <div className="card-info">
                      <div className="card-header-row">
                        <span className="item-name">{ITEM_NAMES[item.id] || item.id}</span>
                        <span className={`rarity-tag rarity-${item.rarity}`}>{item.rarity}</span>
                      </div>

                      <div className="card-price-row">
                        {item.owned ? (
                          <span className="status-owned-badge">✓ Owned</span>
                        ) : (
                          <span className="item-price tabular-nums">
                            {item.price_gold.toLocaleString()} 🪙
                          </span>
                        )}
                      </div>

                      {/* Action buttons */}
                      <div className="card-actions">
                        {equipped ? (
                          <button
                            type="button"
                            className="btn btn-secondary btn-sm btn-full"
                            onClick={() => handleUnequip(item.category)}
                            disabled={busy}
                          >
                            <span>✓ Equipped (Unequip)</span>
                          </button>
                        ) : item.owned ? (
                          <button
                            type="button"
                            className="btn btn-primary btn-sm btn-full"
                            onClick={() => handleEquip(item)}
                            disabled={busy}
                          >
                            <span>{busy ? 'Equipping...' : 'Equip'}</span>
                          </button>
                        ) : (
                          <button
                            type="button"
                            className="btn btn-accent btn-sm btn-full"
                            onClick={() => handlePurchase(item)}
                            disabled={busy || !canAfford || !isAuthenticated}
                            title={!canAfford ? 'Insufficient Gold balance' : 'Unlock item'}
                          >
                            <span>
                              {busy
                                ? 'Purchasing...'
                                : !canAfford
                                ? 'Need more Gold'
                                : `Buy (${item.price_gold.toLocaleString()} 🪙)`}
                            </span>
                          </button>
                        )}
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
