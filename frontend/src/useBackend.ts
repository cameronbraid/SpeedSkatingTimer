import { useState } from "react";
import useWebSocket from "react-use-websocket";

export const useBackend = () => {
  const [socketUrl] = useState(() => {
    return import.meta.env.VITE_WS_URL || `ws://${window.location.host}/ws`;
  });

  return useWebSocket(socketUrl, {
    share: true,
    shouldReconnect: () => true,
    retryOnError: true,
    reconnectAttempts: Number.POSITIVE_INFINITY,
    reconnectInterval: 500,
  });
};
