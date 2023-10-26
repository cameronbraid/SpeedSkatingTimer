import { useEffect, useState } from "react";
import { useBackend } from "./useBackend";
import { ReadyState } from "react-use-websocket";
type Message = SetupMessage | {
    type: string
}
type SetupMessage = {
    type: "setup",
    connected: boolean,
}

export const Setup = () => {

    let [connected, setConnected] = useState<boolean>(null);
    let [subscribed, setSubscribed] = useState(false);

    const { sendJsonMessage, lastJsonMessage, readyState } = useBackend();

    useEffect(() => {

        if (readyState == ReadyState.CLOSED) {
            setConnected(null);
            setSubscribed(false);
        }

        if (lastJsonMessage !== null) {
            let msg = lastJsonMessage as Message
            if (msg.type === "setup") {
                setConnected((msg as SetupMessage).connected);
            }
        }
    }, [readyState, lastJsonMessage, setConnected]);

    useEffect(() => {
        // tell the backend to start sending us setup data
        if (readyState == ReadyState.OPEN) {
            if (!subscribed) {
                setSubscribed(true)
                sendJsonMessage({ type: "subscribe-setup" })
            }
        }

        return () => {

            if (readyState == ReadyState.OPEN) {
                if (subscribed) {
                    sendJsonMessage({ type: "unsubscribe-setup" })
                }
            }
        }

    }, [subscribed, readyState])

    return <div style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0, background: `${connected === null ? "white" : connected ? 'green' : 'red'}` }}></div>
}