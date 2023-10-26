import React, { useState, useCallback, useEffect } from "react";
import useWebSocket, { ReadyState } from "react-use-websocket";
import { TickingClock } from "./TickingClock";
import "./App.scss";
import { Clock } from "./Clock";
import { Route, Routes } from "react-router-dom";
import { Setup } from "./Setup";
import { useBackend } from "./useBackend";

type TsMessage = {
  type: "timestamp",
  timestamp: number,
  duration: number | null, // in nanoseconds
}

type ResetMessage = {
  type: "reset"
}
type Message = TsMessage | ResetMessage



export const App = () => {

  return <Routes>
    <Route path="/setup" element={<Setup />}>
    </Route>
    <Route path="/" element={<SingleLapTimer />}>
    </Route>
  </Routes>
}



const CONNECTION_STATUS = {
  [ReadyState.CONNECTING]: "Connecting",
  [ReadyState.OPEN]: "Connected",
  [ReadyState.CLOSING]: "Disconnecting",
  [ReadyState.CLOSED]: "Disconnected",
  [ReadyState.UNINSTANTIATED]: "Uninstantiated",
}

const SingleLapTimer = () => {
  const [messageHistory, setMessageHistory] = useState<Array<TsMessage>>([]);

  const { sendJsonMessage, lastJsonMessage, readyState } = useBackend();

  const reset = useCallback(() => {
    sendJsonMessage({ type: "reset" });
  }, [sendJsonMessage])

  useEffect(() => {
    if (lastJsonMessage !== null) {

      let msg = lastJsonMessage as Message
      if (msg.type === "timestamp") {
        if (messageHistory.length < 3) {
          setMessageHistory((prev) => prev.concat(msg as TsMessage));
        }
      }
      else if (msg.type === "reset") {
        setMessageHistory([]);
      }

    }
  }, [lastJsonMessage, setMessageHistory]);


  return (
    <div className="App">
      {messageHistory.length < 2 ? (
        <Clock duration={0} mode={"miniPlaceholder"} />
      ) : (
        <>
          <Clock
            duration={
              messageHistory[1]?.duration / 1000000
            }
            mode="mini"
          />
        </>
      )}
      {messageHistory.length === 0 ? (
        <Clock duration={0} mode="big" />
      ) : messageHistory.length < 3 ? (
        <TickingClock
          key={messageHistory.length}
          timestamp={messageHistory[messageHistory.length - 1].timestamp}
          mode="big"
        />
      ) : (
        <Clock
          duration={messageHistory[2].duration / 1000000}
          mode="big"
        />
      )}

      <span>
        {CONNECTION_STATUS[readyState]}
      </span>
      <button onClick={reset}>Reset</button>
    </div>
  );
};
