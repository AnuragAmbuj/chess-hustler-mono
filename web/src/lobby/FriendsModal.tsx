import { useState, useEffect, useCallback } from 'react';
import { api } from '../api/client';
import { FriendInfo, FriendRequestsResponse, BlockedUserItem } from '../types';

interface FriendsModalProps {
  isOpen: boolean;
  onClose: () => void;
  onChallengeFriend?: (friend: FriendInfo) => void;
}

export function FriendsModal({ isOpen, onClose, onChallengeFriend }: FriendsModalProps) {
  const [tab, setTab] = useState<'friends' | 'requests' | 'blocked'>('friends');
  const [friends, setFriends] = useState<FriendInfo[]>([]);
  const [requests, setRequests] = useState<FriendRequestsResponse>({ incoming: [], outgoing: [] });
  const [blocks, setBlocks] = useState<BlockedUserItem[]>([]);
  const [loading, setLoading] = useState(false);

  // Add friend form
  const [addHandle, setAddHandle] = useState('');
  const [formStatus, setFormStatus] = useState<{ message: string; isError: boolean } | null>(null);
  const [actionLoadingId, setActionLoadingId] = useState<number | string | null>(null);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const [friendsList, requestsData, blocksList] = await Promise.all([
        api.getFriends(),
        api.getFriendRequests(),
        api.getBlocks(),
      ]);
      setFriends(friendsList);
      setRequests(requestsData);
      setBlocks(blocksList);
    } catch (err) {
      console.error('Failed to load friends data:', err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (isOpen) {
      loadData();
      setFormStatus(null);
    }
  }, [isOpen, loadData]);

  if (!isOpen) return null;

  const handleSendRequest = async (e: React.FormEvent) => {
    e.preventDefault();
    const handle = addHandle.trim().replace(/^@/, '');
    if (!handle) return;

    setFormStatus(null);
    try {
      await api.sendFriendRequest(handle);
      setFormStatus({ message: `Friend request sent to @${handle}!`, isError: false });
      setAddHandle('');
      loadData();
    } catch (err: any) {
      setFormStatus({
        message: err.message || 'Failed to send friend request',
        isError: true,
      });
    }
  };

  const handleAcceptRequest = async (id: number) => {
    setActionLoadingId(id);
    try {
      await api.acceptFriendRequest(id);
      await loadData();
    } catch (err: any) {
      alert(err.message || 'Failed to accept request');
    } finally {
      setActionLoadingId(null);
    }
  };

  const handleDeclineRequest = async (id: number) => {
    setActionLoadingId(id);
    try {
      await api.declineFriendRequest(id);
      await loadData();
    } catch (err: any) {
      alert(err.message || 'Failed to decline request');
    } finally {
      setActionLoadingId(null);
    }
  };

  const handleRemoveFriend = async (id: number, handle: string) => {
    if (!confirm(`Are you sure you want to unfriend @${handle}?`)) return;
    setActionLoadingId(id);
    try {
      await api.removeFriend(id);
      await loadData();
    } catch (err: any) {
      alert(err.message || 'Failed to unfriend');
    } finally {
      setActionLoadingId(null);
    }
  };

  const handleBlockUser = async (handle: string) => {
    if (!confirm(`Block @${handle}? You won't receive challenges or requests from them.`)) return;
    setActionLoadingId(handle);
    try {
      await api.blockUser(handle);
      await loadData();
    } catch (err: any) {
      alert(err.message || 'Failed to block user');
    } finally {
      setActionLoadingId(null);
    }
  };

  const handleUnblockUser = async (handle: string) => {
    setActionLoadingId(handle);
    try {
      await api.unblockUser(handle);
      await loadData();
    } catch (err: any) {
      alert(err.message || 'Failed to unblock user');
    } finally {
      setActionLoadingId(null);
    }
  };

  return (
    <div className="modal-backdrop" onClick={onClose} role="dialog" aria-modal="true">
      <div className="modal-card modal-friends" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <div className="modal-title-group">
            <h2 className="modal-title">Friends & Community</h2>
            <span className="modal-subtitle">Direct challenges, presence & friends</span>
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

        {/* Tab Navigation */}
        <div className="friends-tab-bar">
          <button
            type="button"
            className={`friends-tab-btn ${tab === 'friends' ? 'active' : ''}`}
            onClick={() => setTab('friends')}
          >
            Friends ({friends.length})
          </button>
          <button
            type="button"
            className={`friends-tab-btn ${tab === 'requests' ? 'active' : ''}`}
            onClick={() => setTab('requests')}
          >
            Requests {requests.incoming.length > 0 && <span className="tab-badge">{requests.incoming.length}</span>}
          </button>
          <button
            type="button"
            className={`friends-tab-btn ${tab === 'blocked' ? 'active' : ''}`}
            onClick={() => setTab('blocked')}
          >
            Blocked ({blocks.length})
          </button>
        </div>

        <div className="modal-body friends-modal-body">
          {/* TAB 1: Friends List */}
          {tab === 'friends' && (
            <div className="friends-tab-content">
              {loading && friends.length === 0 ? (
                <div className="friends-loading-state">Loading friends...</div>
              ) : friends.length === 0 ? (
                <div className="friends-empty-state">
                  <div className="empty-icon">👥</div>
                  <h3>No friends yet</h3>
                  <p>Send a request to play direct challenge games and see when players are online.</p>
                  <button
                    type="button"
                    className="btn btn-secondary btn-sm"
                    onClick={() => setTab('requests')}
                  >
                    + Add Friend
                  </button>
                </div>
              ) : (
                <ul className="friends-list">
                  {friends.map((f) => {
                    const isOnline = f.presence?.online;
                    const statusText = f.presence?.status === 'playing' ? 'In Game' : isOnline ? 'Online' : 'Offline';
                    const statusClass = f.presence?.status === 'playing' ? 'status-playing' : isOnline ? 'status-online' : 'status-offline';

                    return (
                      <li key={f.id} className="friend-item">
                        <div className="friend-item-info">
                          <div className="friend-avatar-wrap">
                            <div className="user-avatar-circle">
                              {f.handle.slice(0, 2).toUpperCase()}
                            </div>
                            <span className={`presence-dot ${statusClass}`} title={statusText} />
                          </div>
                          <div className="friend-details">
                            <div className="friend-name-row">
                              <span className="friend-handle">@{f.handle}</span>
                              <span className="friend-rating tabular-nums">{f.rating}</span>
                            </div>
                            <span className="friend-presence-label">{statusText}</span>
                          </div>
                        </div>

                        <div className="friend-actions">
                          {onChallengeFriend && (
                            <button
                              type="button"
                              className="btn btn-primary btn-sm"
                              onClick={() => {
                                onChallengeFriend(f);
                                onClose();
                              }}
                              title="Direct challenge to a game"
                            >
                              ⚔️ Challenge
                            </button>
                          )}
                          <button
                            type="button"
                            className="btn btn-ghost btn-sm btn-icon"
                            onClick={() => handleRemoveFriend(f.id, f.handle)}
                            disabled={actionLoadingId === f.id}
                            title="Unfriend"
                          >
                            Unfriend
                          </button>
                          <button
                            type="button"
                            className="btn btn-ghost btn-sm btn-icon"
                            onClick={() => handleBlockUser(f.handle)}
                            disabled={actionLoadingId === f.handle}
                            title="Block"
                          >
                            Block
                          </button>
                        </div>
                      </li>
                    );
                  })}
                </ul>
              )}
            </div>
          )}

          {/* TAB 2: Requests (Send & Review) */}
          {tab === 'requests' && (
            <div className="requests-tab-content">
              {/* Send Request Form */}
              <div className="send-request-card">
                <h3 className="section-subheading">Add a Friend</h3>
                <form onSubmit={handleSendRequest} className="send-request-form">
                  <div className="input-group">
                    <span className="input-prefix">@</span>
                    <input
                      type="text"
                      className="form-input"
                      placeholder="player_handle"
                      value={addHandle}
                      onChange={(e) => setAddHandle(e.target.value)}
                      required
                    />
                  </div>
                  <button type="submit" className="btn btn-primary btn-sm">
                    Send Request
                  </button>
                </form>
                {formStatus && (
                  <div className={`form-feedback ${formStatus.isError ? 'error' : 'success'}`}>
                    {formStatus.message}
                  </div>
                )}
              </div>

              {/* Incoming Requests */}
              <div className="requests-section">
                <h3 className="section-subheading">
                  Incoming Requests ({requests.incoming.length})
                </h3>
                {requests.incoming.length === 0 ? (
                  <div className="sub-empty-text">No incoming friend requests.</div>
                ) : (
                  <ul className="requests-list">
                    {requests.incoming.map((req) => (
                      <li key={req.id} className="request-item">
                        <div className="request-info">
                          <span className="request-handle">@{req.handle}</span>
                          <span className="request-rating tabular-nums">{req.rating}</span>
                        </div>
                        <div className="request-actions">
                          <button
                            type="button"
                            className="btn btn-primary btn-sm"
                            onClick={() => handleAcceptRequest(req.id)}
                            disabled={actionLoadingId === req.id}
                          >
                            Accept
                          </button>
                          <button
                            type="button"
                            className="btn btn-ghost btn-sm"
                            onClick={() => handleDeclineRequest(req.id)}
                            disabled={actionLoadingId === req.id}
                          >
                            Decline
                          </button>
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
              </div>

              {/* Outgoing Requests */}
              <div className="requests-section">
                <h3 className="section-subheading">
                  Outgoing Requests ({requests.outgoing.length})
                </h3>
                {requests.outgoing.length === 0 ? (
                  <div className="sub-empty-text">No pending outgoing requests.</div>
                ) : (
                  <ul className="requests-list">
                    {requests.outgoing.map((req) => (
                      <li key={req.id} className="request-item">
                        <div className="request-info">
                          <span className="request-handle">@{req.handle}</span>
                          <span className="request-rating tabular-nums">{req.rating}</span>
                        </div>
                        <span className="badge-pending">Pending</span>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </div>
          )}

          {/* TAB 3: Blocked Users */}
          {tab === 'blocked' && (
            <div className="blocked-tab-content">
              {blocks.length === 0 ? (
                <div className="friends-empty-state">
                  <div className="empty-icon">🛡️</div>
                  <h3>No blocked users</h3>
                  <p>Blocked players cannot send you friend requests, chat, or challenge you.</p>
                </div>
              ) : (
                <ul className="blocked-list">
                  {blocks.map((b) => (
                    <li key={b.handle} className="blocked-item">
                      <div className="blocked-info">
                        <span className="blocked-handle">@{b.handle}</span>
                        <span className="blocked-date">
                          Blocked on {new Date(b.blocked_at).toLocaleDateString()}
                        </span>
                      </div>
                      <button
                        type="button"
                        className="btn btn-ghost btn-sm"
                        onClick={() => handleUnblockUser(b.handle)}
                        disabled={actionLoadingId === b.handle}
                      >
                        Unblock
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
