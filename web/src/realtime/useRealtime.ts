import { useEffect, useState } from 'react';
import { realtime, EventHandler } from './RealtimeClient';
import { useAuth } from '../auth/useAuth';

export function useRealtime() {
  const { isAuthenticated } = useAuth();
  const [isConnected, setIsConnected] = useState(realtime.isConnected());

  useEffect(() => {
    if (isAuthenticated) {
      realtime.connect();
    } else {
      realtime.disconnect();
    }

    const checkInterval = setInterval(() => {
      setIsConnected(realtime.isConnected());
    }, 1000);

    return () => {
      clearInterval(checkInterval);
    };
  }, [isAuthenticated]);

  const subscribe = (eventType: string, handler: EventHandler) => {
    return realtime.on(eventType, handler);
  };

  const subscribeChannel = (channel: string) => {
    realtime.subscribeChannel(channel);
  };

  const unsubscribeChannel = (channel: string) => {
    realtime.unsubscribeChannel(channel);
  };

  const sendMove = (gameId: string, uci: string) => {
    realtime.sendMove(gameId, uci);
  };

  return {
    isConnected,
    subscribe,
    subscribeChannel,
    unsubscribeChannel,
    sendMove,
  };
}
