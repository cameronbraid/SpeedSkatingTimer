import { useEffect, useState, useRef, useCallback } from 'react'
import { useNats } from './useNats'
import type { PingResponse } from '../types/stopwatch'

const MIN_LAG_MS = 75

interface LatencyState {
  /** Calculated one-way lag in milliseconds (half of average RTT, minimum MIN_LAG_MS) */
  lagMs: number
  /** Average round-trip time in milliseconds */
  rttMs: number
}

/**
 * Hook for measuring network latency to the server via periodic pings
 * 
 * Returns:
 * - lagMs: estimated one-way latency (half of RTT, minimum MIN_LAG_MS)
 * - rttMs: average round-trip time over last 10 measurements
 */
export function useLatency(): LatencyState {
  const { request, connected } = useNats()
  const [lagMs, setLagMs] = useState(0)
  const [rttMs, setRttMs] = useState(0)
  
  const rttMeasurementsRef = useRef<number[]>([])
  const pingIntervalRef = useRef<number | null>(null)

  // Calculate dynamic lag based on RTT measurements
  const updateLagFromRtt = useCallback((rtt: number) => {
    rttMeasurementsRef.current.push(rtt)
    
    // Keep only last 10 measurements
    while (rttMeasurementsRef.current.length > 10) {
      rttMeasurementsRef.current.shift()
    }

    // Calculate lag using average RTT for stability
    // RTT is round-trip time, so one-way latency is approximately RTT/2
    const avgRtt = rttMeasurementsRef.current.reduce((a, b) => a + b, 0) / rttMeasurementsRef.current.length
    const calculatedLag = Math.max(avgRtt / 2, MIN_LAG_MS) // Minimum MIN_LAG_MS lag, use half RTT as one-way latency
    setLagMs(calculatedLag)
    setRttMs(avgRtt)
  }, [])

  // Send periodic ping requests for RTT calculation
  useEffect(() => {
    if (!connected) return

    const pingInterval = 2000 // Ping every 2 seconds
    
    const sendPing = async () => {
      try {
        const requestTime = performance.now()
        const _response = await request<PingResponse>('system.v1.ping', {})
        const responseTime = performance.now()
        
        // Calculate RTT (round-trip time)
        const rtt = responseTime - requestTime
        updateLagFromRtt(rtt)
      } catch (error) {
        console.error('Failed to ping server:', error)
      }
    }

    // Send initial ping immediately
    sendPing()

    // Set up interval for periodic pings
    const intervalId = window.setInterval(sendPing, pingInterval)
    pingIntervalRef.current = intervalId

    return () => {
      if (pingIntervalRef.current !== null) {
        clearInterval(pingIntervalRef.current)
        pingIntervalRef.current = null
      }
    }
  }, [connected, request, updateLagFromRtt])

  return { lagMs, rttMs }
}

