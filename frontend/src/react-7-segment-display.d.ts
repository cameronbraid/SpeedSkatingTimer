declare module 'react-7-segment-display' {
  import { FC } from 'react'

  interface DisplayProps {
    value: string | number
    color?: string
    height?: number
    count?: number
    backgroundColor?: string
    skew?: boolean
  }

  export const Display: FC<DisplayProps>
}
