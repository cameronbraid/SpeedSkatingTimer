import { AutoTextSize } from 'auto-text-size'
import { useEffect, useRef, useState } from 'react'
import './Duration.scss'

export type Mode = 'mini' | 'big' | 'miniPlaceholder'

interface DurationProps {
  duration: number // in milliseconds
  mode?: Mode
  lagMs?: number // intentional lag in milliseconds
  serverTimeNs?: number // server time in nanoseconds
  previousLap?: boolean
  running?: boolean
}

const NON_FRACTIONAL_DIGITS = 2
const FRACTIONAL_DIGITS = 3

export function Duration({ duration, mode = 'big', lagMs = 0, previousLap = false, running = undefined }: DurationProps) {
  // Apply intentional lag: never display time server hasn't confirmed
  // If lagMs > 0, we subtract it from the displayed duration

  const displayDuration = Math.max(0, duration - lagMs)
  const seconds = displayDuration / 1000
  const wholeSeconds = Math.floor(seconds)
  const fractionalPart = (seconds - wholeSeconds).toFixed(FRACTIONAL_DIGITS).split('.')[1] || '000' // Get "XXX" part (digits only)
  const text = `${wholeSeconds.toString().padStart(NON_FRACTIONAL_DIGITS, '0')}.${fractionalPart}`

  const containerRef = useRef<HTMLDivElement>(null)
  const [isStable, setIsStable] = useState(false)

  useEffect(() => {
    const el = containerRef.current
    if (!el) return

    let rafId: number
    let lastWidth = 0
    let lastHeight = 0
    let stableFrames = 0

    const checkStability = () => {
      const { offsetWidth, offsetHeight } = el
      
      if (offsetWidth === lastWidth && offsetHeight === lastHeight) {
        stableFrames++
        if (stableFrames >= 2) {
          setIsStable(true)
          return // Stop polling
        }
      } else {
        stableFrames = 0
        lastWidth = offsetWidth
        lastHeight = offsetHeight
      }
      
      rafId = requestAnimationFrame(checkStability)
    }

    rafId = requestAnimationFrame(checkStability)
    return () => cancelAnimationFrame(rafId)
  }, [])

  return (
    <div
      ref={containerRef}
      className={`Duration ${mode} ${previousLap ? 'previous-lap' : ''} ${running === false ? 'not-running' : ''} ${isStable ? 'stable' : ''}`}
    >
      <AutoTextSize mode="boxoneline" minFontSizePx={10} maxFontSizePx={400}>
        {text}
      </AutoTextSize>
    </div>
  )
}

