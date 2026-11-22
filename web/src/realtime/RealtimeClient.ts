import { api } from '../api/client';
import { RealtimeEnvelope } from '../types';

export type EventHandler<T = any> = (envelope: RealtimeEnvelope<T>) => void;

export class RealtimeClient {
  private ws: WebSocket | null = null;
  private lastIds: Record<string, string> = {};
  private eventHandlers: Map<string, Set<EventHandler>> = new Map();
  private pingInterval: number | null = null;
  private reconnectTimeout: number | null = null;
  private reconnectAttempts = 0;
  private isConnecting = false;
  private isExplicitlyClosed = false;
  private activeGameId: string | null = null;

  constructor() {
    // Restore any cached last_ids from session storage
    try {
      const stored = sessionStorage.getItem('hustle_last_ids');
      if (stored) {
        this.lastIds = JSON.parse(stored);
      }
    } catch {
      this.lastIds = {};
    }
  }

  on(type: string, handler: EventHandler) {
    let handlers = this.eventHandlers.get(type);
    if (!handlers) {
      handlers = new Set();
      this.eventHandlers.set(type, handlers);
    }
    handlers.add(handler);
    return () => {
      handlers?.delete(handler);
    };
  }

  off(type: string, handler: EventHandler) {
    const handlers = this.eventHandlers.get(type);
    if (handlers) {
      handlers.delete(handler);
    }
  }

  private dispatch(envelope: RealtimeEnvelope) {
    // Track cursor for gap resume
    if (envelope.ch && envelope.id) {
      this.lastIds[envelope.ch] = envelope.id;
      try {
        sessionStorage.setItem('hustle_last_ids', JSON.stringify(this.lastIds));
      } catch {
        // ignore
      }
    }

    const handlers = this.eventHandlers.get(envelope.type);
    if (handlers) {
      handlers.forEach((h) => {
        try {
          h(envelope);
        } catch (err) {
          console.error(`Error in realtime handler for ${envelope.type}:`, err);
        }
      });
    }

    // Also dispatch to wildcard '*' handlers
    const wildcardHandlers = this.eventHandlers.get('*');
    if (wildcardHandlers) {
      wildcardHandlers.forEach((h) => {
        try {
          h(envelope);
        } catch (err) {
          console.error('Error in wildcard realtime handler:', err);
        }
      });
    }
  }

  async connect() {
    if (this.ws && (this.ws.readyState === WebSocket.OPEN || this.ws.readyState === WebSocket.CONNECTING)) {
      return;
    }
    if (this.isConnecting) return;
    this.isConnecting = true;
    this.isExplicitlyClosed = false;

    try {
      // Step 1: Request single-use ticket from GET /v1/realtime/ticket
      const ticketData = await api.getRealtimeTicket();
      const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const wsHost = window.location.host;
      const wsUrl = ticketData.ws_url.startsWith('ws')
        ? `${ticketData.ws_url}?ticket=${ticketData.ticket}`
        : `${wsProtocol}//${wsHost}${ticketData.ws_url}?ticket=${ticketData.ticket}`;

      const socket = new WebSocket(wsUrl);
      this.ws = socket;

      socket.onopen = () => {
        this.isConnecting = false;
        this.reconnectAttempts = 0;

        // Step 2: Handshake hello with last_ids for gap resume
        this.send({
          type: 'hello',
          data: {
            client: 'web@1.0.0',
            last_ids: this.lastIds,
          },
        });

        // If there's an active game, ensure subscription
        if (this.activeGameId) {
          this.subscribeChannel(`g:${this.activeGameId}`);
        }

        // Step 3: Setup heartbeat ping every 15s
        this.startHeartbeat();
      };

      socket.onmessage = (event) => {
        try {
          const envelope = JSON.parse(event.data) as RealtimeEnvelope;
          if (envelope.type === 'ping') {
            this.send({ type: 'pong' });
            return;
          }
          this.dispatch(envelope);
        } catch {
          // ignore non-json messages
        }
      };

      socket.onclose = () => {
        this.stopHeartbeat();
        this.isConnecting = false;
        if (!this.isExplicitlyClosed) {
          this.scheduleReconnect();
        }
      };

      socket.onerror = () => {
        this.stopHeartbeat();
        this.isConnecting = false;
      };
    } catch {
      this.isConnecting = false;
      if (!this.isExplicitlyClosed) {
        this.scheduleReconnect();
      }
    }
  }

  private startHeartbeat() {
    this.stopHeartbeat();
    this.pingInterval = window.setInterval(() => {
      if (this.ws && this.ws.readyState === WebSocket.OPEN) {
        this.send({ type: 'ping' });
      }
    }, 15000);
  }

  private stopHeartbeat() {
    if (this.pingInterval) {
      clearInterval(this.pingInterval);
      this.pingInterval = null;
    }
  }

  private scheduleReconnect() {
    if (this.reconnectTimeout) return;
    this.reconnectAttempts++;
    // Exponential backoff with jitter: min(8000, 500 * 1.5^attempt) + random(0, 500)
    const delay = Math.min(8000, 500 * Math.pow(1.5, this.reconnectAttempts)) + Math.random() * 500;
    this.reconnectTimeout = window.setTimeout(() => {
      this.reconnectTimeout = null;
      this.connect();
    }, delay);
  }

  subscribeChannel(channel: string) {
    if (channel.startsWith('g:')) {
      this.activeGameId = channel.replace('g:', '');
    }
    this.send({
      type: 'subscribe',
      data: { ch: channel },
    });
  }

  unsubscribeChannel(channel: string) {
    if (channel.startsWith('g:') && this.activeGameId === channel.replace('g:', '')) {
      this.activeGameId = null;
    }
    this.send({
      type: 'unsubscribe',
      data: { ch: channel },
    });
  }

  sendMove(gameId: string, uci: string) {
    this.send({
      type: 'game.move',
      data: {
        game_id: gameId,
        uci,
      },
    });
  }

  send(payload: Record<string, any>) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(payload));
    }
  }

  disconnect() {
    this.isExplicitlyClosed = true;
    this.stopHeartbeat();
    if (this.reconnectTimeout) {
      clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }

  isConnected(): boolean {
    return this.ws !== null && this.ws.readyState === WebSocket.OPEN;
  }
}

export const realtime = new RealtimeClient();
