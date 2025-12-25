import { useEffect, useState, useCallback } from 'react'
import { useNats } from './useNats'
import { useLatency } from './useLatency'
import type {
  StateSnapshot,
  TickUpdate,
  LapEvent,
  StopwatchState,
} from '../types/stopwatch'

interface StopwatchHookState {
  state: StopwatchState
  runId: number
  elapsedNs: number
  lapCount: number
  running: boolean
  serverTimeNs: number
  lagMs: number
  rttMs: number
  laps: Array<{ runId: number; lap: number; lapTimeNs: number }>
  currentLapTimeNs: number
  currentLapId: number
}

export function useStopwatch() {
  const { subscribe, request, connected, subscribeJetStream } = useNats()
  const { lagMs, rttMs } = useLatency()
  const [snapshot, setSnapshot] = useState<StateSnapshot | null>(null)
  const [laps, setLaps] = useState<Array<{ runId: number; lap: number; lapTimeNs: number }>>([])
  const [currentLapTimeNs, setCurrentLapTimeNs] = useState(0)
  const [currentLapId, setCurrentLapId] = useState(0)

  // Subscribe to state changes
  useEffect(() => {
    if (!connected) return

    subscribe<StateSnapshot>('stopwatch.v1.live.state', (stateSnapshot) => {
      setSnapshot(stateSnapshot)
    }).catch(console.error)
  }, [connected, subscribe])

  // Subscribe to tick updates
  useEffect(() => {
    if (!connected) return

    subscribe<TickUpdate>('stopwatch.v1.live.tick', (tick) => {
      // Update snapshot with latest tick data - tick.elapsed_ns is now the current lap time
      setSnapshot((prev) => {
        if (!prev) {
          // Create a minimal snapshot if we don't have one yet
          setCurrentLapTimeNs(tick.elapsed_ns) // Tick contains current lap time
          setCurrentLapId(tick.lap_id)
          return {
            state: 'Running',
            run_id: 0,
            elapsed_ns: tick.elapsed_ns,
            server_time_ns: tick.server_time_ns,
            lap_count: 0,
            running: true,
          }
        }
        const updated = {
          ...prev,
          elapsed_ns: tick.elapsed_ns,
          server_time_ns: tick.server_time_ns,
        }

        // Tick event elapsed_ns is the current lap time directly
        setCurrentLapTimeNs(tick.elapsed_ns)
        setCurrentLapId(tick.lap_id)

        return updated
      })
    }).catch(console.error)
  }, [connected, subscribe])

  // Fetch lap history from JetStream on connect and continue streaming
  useEffect(() => {
    if (!connected) return

    let unsubscribe: (() => void) | null = null

    // Stream laps from JetStream (last 60 minutes of history + live updates)
    // Messages arrive in chronological order, so newer laps come later
    subscribeJetStream<LapEvent>(
      'STOPWATCH_LAPS',
      'stopwatch.v1.live.lap',
      10,
      lapEvent => {
        if (lapEvent.event_type === 'finish' && lapEvent.lap_time_ns !== undefined) {
          const newLap = {
            runId: lapEvent.run_id,
            lap: lapEvent.lap,
            lapTimeNs: lapEvent.lap_time_ns,
          }
          // New laps go at the front (newest first), limit to 20
          setLaps((prev) => [newLap, ...prev].slice(0, 20))
        }
      },
    )
      .then((result) => {
        unsubscribe = result.unsubscribe
      })
      .catch(console.error)

    // Cleanup: unsubscribe when component unmounts or connection changes
    return () => {
      if (unsubscribe) {
        unsubscribe()
      }
    }
  }, [connected, subscribeJetStream])

  // Request initial state
  useEffect(() => {
    if (!connected) return

    request<StateSnapshot>('stopwatch.v1.get_state', {})
      .then(setSnapshot)
      .catch(console.error)
  }, [connected, request])

  const arm = useCallback(async () => {
    try {
      const response = await request<StateSnapshot>('stopwatch.v1.arm', {})
      setSnapshot(response)
    } catch (error) {
      console.error('Failed to arm stopwatch:', error)
    }
  }, [request])

  const unarm = useCallback(async () => {
    try {
      const response = await request<StateSnapshot>('stopwatch.v1.unarm', {})
      setSnapshot(response)
      setLaps([])
    } catch (error) {
      console.error('Failed to unarm stopwatch:', error)
    }
  }, [request])

  const reset = useCallback(async () => {
    try {
      const response = await request<StateSnapshot>('stopwatch.v1.reset', {})
      setSnapshot(response)
    } catch (error) {
      console.error('Failed to reset stopwatch:', error)
    }
  }, [request])

  // Ensure elapsedNs is 0 when disarmed, regardless of snapshot
  const effectiveElapsedNs = (snapshot?.state === 'Disarmed') ? 0 : (snapshot?.elapsed_ns || 0)

  const state: StopwatchHookState = {
    state: snapshot?.state || 'Disarmed',
    runId: snapshot?.run_id || 0,
    elapsedNs: effectiveElapsedNs,
    lapCount: snapshot?.lap_count || 0,
    running: snapshot?.running || false,
    serverTimeNs: snapshot?.server_time_ns || 0,

    lagMs,
    rttMs,
    laps,
    currentLapTimeNs: snapshot?.running ? currentLapTimeNs : 0,
    currentLapId: snapshot?.running ? currentLapId : 0,
  }

  return {
    ...state,
    connected,
    arm,
    unarm,
    reset,
  }
}
