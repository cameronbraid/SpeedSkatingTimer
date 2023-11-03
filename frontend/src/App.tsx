import React, { useState, useCallback, useEffect } from "react"
import useWebSocket, { ReadyState } from "react-use-websocket"
import { TickingClock } from "./TickingClock"
import "./App.scss"
import { Clock } from "./Clock"
import { Route, Routes } from "react-router-dom"
import { Setup } from "./Setup"
import { useBackend } from "./useBackend"

type TsMessage = {
  type: "timestamp"
  timestamp: number
  duration: number | null // in nanoseconds
}

type ResetMessage = {
  type: "reset"
}
type Message = TsMessage | ResetMessage

export const App = () => {
  return (
    <Routes>
      <Route path="/setup" element={<Setup />}></Route>
      <Route path="/" element={<SingleLapTimer />}></Route>
      <Route path="/timer" element={<LapTimer />}></Route>
    </Routes>
  )
}

const CONNECTION_STATUS = {
  [ReadyState.CONNECTING]: "Connecting",
  [ReadyState.OPEN]: "Connected",
  [ReadyState.CLOSING]: "Disconnecting",
  [ReadyState.CLOSED]: "Disconnected",
  [ReadyState.UNINSTANTIATED]: "Uninstantiated",
}

const SingleLapTimer = () => {
  const [messageHistory, setMessageHistory] = useState<Array<TsMessage>>([])

  const { sendJsonMessage, lastJsonMessage, readyState } = useBackend()

  const reset = useCallback(() => {
    sendJsonMessage({ type: "reset" })
  }, [sendJsonMessage])

  useEffect(() => {

    if (lastJsonMessage !== null) {
      let msg = lastJsonMessage as Message
      if (msg.type === "timestamp") {
        if (messageHistory.length < 3) {
          setMessageHistory((prev) => prev.concat(msg as TsMessage))
        }
      } else if (msg.type === "reset") {
        setMessageHistory([])
      }
    }
  }, [lastJsonMessage, setMessageHistory])

  return (
    <div className="App">
      {messageHistory.length < 2 ? (
        <Clock duration={0} mode={"miniPlaceholder"} />
      ) : (
        <>
          <Clock duration={messageHistory[1]?.duration / 1000000} mode="mini" />
        </>
      )}
      {messageHistory.length === 0 ? (
        <Clock duration={0} mode="big" />
      ) : messageHistory.length < 3 ? (
        <TickingClock key={messageHistory.length} timestamp={messageHistory[messageHistory.length - 1].timestamp} mode="big" />
      ) : (
        <Clock duration={messageHistory[2].duration / 1000000} mode="big" />
      )}

      <span>{CONNECTION_STATUS[readyState]}</span>
      <button onClick={reset}>Reset</button>
    </div>
  )
}

const LapTimer = () => {
  const [messages, setMessages] = useState<TsMessage[]>([])
  const [lapTimes, setLapTimes] = useState<TsMessage[]>([])

  const { sendJsonMessage, lastJsonMessage, readyState } = useBackend()

  const reset = useCallback(() => {
    sendJsonMessage({ type: "reset" })
    setMessages([])
  }, [sendJsonMessage])


  useEffect(() => {
    if (lastJsonMessage !== null) {
      let m = lastJsonMessage as Message
      if (m.type === "timestamp") {
        let msg = m as TsMessage
        if (messages.length > 0) {
          setLapTimes(s => [...s, msg])
        }

        setMessages(msgs => {
          msgs = [...msgs]
          msgs.push(msg as TsMessage)
          while (msgs.length > 2) {
            msgs.splice(0, 1)
          }
          return msgs
        })
      } else if (m.type === "reset") {
        setMessages([])
      }
    }
  }, [lastJsonMessage, setMessages])

  return (
    <div className="App">
      <Clock key='0' duration={0} mode="miniPlaceholder" />

      {messages.length == 0 ?
        <>
          <Clock key='0' duration={0} mode="miniPlaceholder" />
          <Clock key='1' duration={0} mode="big" />
        </>
        : messages.length == 1 ?
          <>
            <TickingClock key={messages[0].timestamp} timestamp={messages[0].timestamp} mode="mini" />
            <Clock duration={0} mode="big" />
          </>
          :
          <>
            <TickingClock key={messages[1].timestamp} timestamp={messages[1].timestamp} mode="mini" />
            <Clock duration={messages[1].duration / 1000000} mode="big" />
          </>
      }

      <span>{CONNECTION_STATUS[readyState]}</span>
      <button onClick={reset}>Reset</button>
      {/* <button onClick={sample}>Sample</button> */}
      <div >{lapTimes.map((lapTime => <span>{(lapTime.duration / 1000000000).toFixed(3)} </span>))}</div>
    </div>
  )
}
