import { useState, useEffect, useRef } from 'react';
import { api } from '../api/client';
import { useRealtime } from '../realtime/useRealtime';
import { useAuth } from '../auth/useAuth';
import { ChatMessageItem } from '../types';

interface GameChatProps {
  gameId: string;
  opponentHandle: string;
  onNewMessage?: () => void;
}

const PRESET_MESSAGES = ['Good luck!', 'Well played!', 'Thanks for the game!'];

export function GameChat({ gameId, opponentHandle, onNewMessage }: GameChatProps) {
  const { user } = useAuth();
  const { subscribe } = useRealtime();
  const [messages, setMessages] = useState<ChatMessageItem[]>([]);
  const [inputText, setInputText] = useState('');
  const [isSending, setIsSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const chatEndRef = useRef<HTMLDivElement>(null);

  // Load chat history
  useEffect(() => {
    let isMounted = true;
    api
      .getGameChat(gameId)
      .then((history) => {
        if (isMounted) setMessages(history);
      })
      .catch((err) => {
        console.error('Failed to load chat history:', err);
      });

    return () => {
      isMounted = false;
    };
  }, [gameId]);

  // Listen for realtime chat messages
  useEffect(() => {
    const unsubscribe = subscribe('chat.message', (envelope) => {
      const data = envelope.data as ChatMessageItem;
      if (data && data.game_id === gameId) {
        setMessages((prev) => {
          if (prev.some((m) => m.id === data.id)) return prev;
          return [...prev, data];
        });
        if (onNewMessage) onNewMessage();
      }
    });

    return () => {
      unsubscribe();
    };
  }, [gameId, subscribe, onNewMessage]);

  // Auto-scroll to bottom
  useEffect(() => {
    chatEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const handleSendMessage = async (textToSend?: string) => {
    const text = (textToSend ?? inputText).trim();
    if (!text || isSending) return;

    setIsSending(true);
    setError(null);

    try {
      const sent = await api.sendGameChat(gameId, text);
      setMessages((prev) => {
        if (prev.some((m) => m.id === sent.id)) return prev;
        return [...prev, sent];
      });
      setInputText('');
    } catch (err: any) {
      setError(err.message || 'Failed to send message');
    } finally {
      setIsSending(false);
    }
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    handleSendMessage();
  };

  return (
    <div className="game-chat-container">
      {/* Messages Scroll Area */}
      <div className="chat-messages-scroll">
        {messages.length === 0 ? (
          <div className="chat-empty-hint">
            <span>In-game chat with @{opponentHandle}</span>
            <span className="chat-empty-sub">Messages are filtered for fair play and safety</span>
          </div>
        ) : (
          messages.map((m) => {
            const isMe = user?.handle && m.handle === user.handle;
            const timeStr = m.created_at
              ? new Date(m.created_at).toLocaleTimeString([], {
                  hour: '2-digit',
                  minute: '2-digit',
                })
              : '';

            return (
              <div
                key={m.id}
                className={`chat-bubble-row ${isMe ? 'chat-me' : 'chat-them'}`}
              >
                <div className="chat-bubble">
                  <div className="chat-meta">
                    <span className="chat-sender">{isMe ? 'You' : `@${m.handle}`}</span>
                    <span className="chat-time">{timeStr}</span>
                  </div>
                  <div className="chat-body-text">
                    {m.body}
                    {m.masked && <span className="masked-tag" title="Filtered profanity">[filtered]</span>}
                  </div>
                </div>
              </div>
            );
          })
        )}
        <div ref={chatEndRef} />
      </div>

      {error && <div className="chat-error-banner">{error}</div>}

      {/* Preset Quick Actions */}
      <div className="chat-presets-bar">
        {PRESET_MESSAGES.map((msg) => (
          <button
            key={msg}
            type="button"
            className="preset-btn"
            onClick={() => handleSendMessage(msg)}
            disabled={isSending}
          >
            {msg}
          </button>
        ))}
      </div>

      {/* Input Form */}
      <form onSubmit={handleSubmit} className="chat-input-form">
        <input
          type="text"
          className="chat-text-input"
          placeholder="Type a message (max 200 chars)..."
          maxLength={200}
          value={inputText}
          onChange={(e) => setInputText(e.target.value)}
          disabled={isSending}
        />
        <button
          type="submit"
          className="btn btn-primary btn-sm chat-send-btn"
          disabled={isSending || !inputText.trim()}
        >
          Send
        </button>
      </form>
    </div>
  );
}
