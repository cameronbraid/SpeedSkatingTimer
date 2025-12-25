declare module '7-segment-display' {
  import { FC } from 'react'

  interface SevenSegmentDisplayProps {
    number: number
    color?: string
    segmentThickness?: number
    segmentSpacing?: number
    width?: number
    height?: number
  }

  const SevenSegmentDisplay: FC<SevenSegmentDisplayProps>
  export default SevenSegmentDisplay
}





