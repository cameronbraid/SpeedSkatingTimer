import useRequestAnimationFrame from 'use-request-animation-frame'
import { useState, useEffect, useRef } from 'react'
import { Duration, Mode } from './Duration'

interface TickingDurationProps {
  startTimeNs: number // elapsed time in nanoseconds from server
  lapId: number // current lap id from server
  mode: Mode
  lagMs: number // intentional lag in milliseconds
  running: boolean
}

export function TickingDuration({ startTimeNs, lapId, mode, lagMs, running }: TickingDurationProps) {
  const [duration, setDuration] = useState(0)
  const laggedBaseTimeRef = useRef<number>(0) // Base time with lag applied
  const lastLocalTimeRef = useRef<number>(performance.now())
  const initializedRef = useRef<boolean>(false)
  const maxDurationRef = useRef<number>(0) // Never go below this value
  const lastLapIdRef = useRef<number>(0) // Track for lap reset detection

  useEffect(() => {
    const serverTimeMs = startTimeNs / 1_000_000
    const laggedTimeMs = Math.max(0, serverTimeMs - lagMs)
    
    // Detect lap reset: if lapId changed (new lap started), reset max tracking
    if (lapId !== lastLapIdRef.current && lastLapIdRef.current !== 0) {
      maxDurationRef.current = 0
      initializedRef.current = false
    }
    lastLapIdRef.current = lapId
    
    if (!initializedRef.current) {
      // First initialization: set everything up
      laggedBaseTimeRef.current = laggedTimeMs
      lastLocalTimeRef.current = performance.now()
      maxDurationRef.current = laggedTimeMs
      setDuration(laggedTimeMs)
      initializedRef.current = true
    } else {
      // When server updates: adjust base time and sync local time reference
      // This ensures smooth counting from the new base time
      const now = performance.now()
      laggedBaseTimeRef.current = laggedTimeMs
      lastLocalTimeRef.current = now
    }
  }, [startTimeNs, lapId, lagMs])

  useRequestAnimationFrame(
    () => {
      if (running && initializedRef.current) {
        const now = performance.now()
        const localDelta = now - lastLocalTimeRef.current
        const newDuration = laggedBaseTimeRef.current + localDelta
        // Never go backwards - always use max of new value and previous max
        const safeDuration = Math.max(0, newDuration, maxDurationRef.current)
        maxDurationRef.current = safeDuration
        setDuration(safeDuration)
      }
    },
    { shouldAnimate: running }
  )

  // Pass duration directly without subtracting lag again (already applied)
  return <Duration duration={running ? duration : 0} mode={mode} lagMs={0} running={running} />
}

