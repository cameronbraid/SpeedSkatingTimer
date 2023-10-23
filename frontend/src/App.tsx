import React, { useState, useCallback, useEffect } from "react";
import useWebSocket, { ReadyState } from "react-use-websocket";
import { TickingClock } from "./TickingClock";
import "./App.scss";
import { Clock } from "./Clock";

type TsMessage = {
  type: "timestamp";
  timestamp: number;
};

export const App = () => {
  //Public API that will echo messages sent to it back to the client
  const [socketUrl, setSocketUrl] = useState("ws://192.168.0.157:8000/ws"); // ws://localhost:1233
  const [messageHistory, setMessageHistory] = useState<Array<TsMessage>>([]);

  const { sendMessage, lastMessage, readyState } = useWebSocket(socketUrl);

  useEffect(() => {
    if (lastMessage !== null) {
      if (messageHistory.length < 3) {
        setMessageHistory((prev) => prev.concat(JSON.parse(lastMessage.data)));
      }
    }
  }, [lastMessage, setMessageHistory]);

  // const handleClickSendMessage = useCallback(() => sendMessage('Hello'), []);

  const connectionStatus = {
    [ReadyState.CONNECTING]: "Connecting",
    [ReadyState.OPEN]: "Connected",
    [ReadyState.CLOSING]: "Disconnecting",
    [ReadyState.CLOSED]: "Disconnected",
    [ReadyState.UNINSTANTIATED]: "Uninstantiated",
  }[readyState];

  return (
    <div className="App">
      {messageHistory.length < 2 ? (
        <Clock duration={0} mode={"miniPlaceholder"} />
      ) : (
        <>
          <Clock
            duration={
              messageHistory[1]?.timestamp - messageHistory[0]?.timestamp
            }
            mode={"mini"}
          />
        </>
      )}
      {messageHistory.length === 0 ? (
        <Clock duration={0} mode={"big"} />
      ) : messageHistory.length < 3 ? (
        <TickingClock
          key={messageHistory.length}
          timestamp={messageHistory[messageHistory.length - 1].timestamp}
          mode={"big"}
        />
      ) : (
        <Clock
          duration={messageHistory[2].timestamp - messageHistory[1].timestamp}
          mode={"big"}
        />
      )}

      <span>
        <b>{connectionStatus}</b>
      </span>
      <button onClick={() => setMessageHistory([])}>Reset</button>
    </div>
  );
};
