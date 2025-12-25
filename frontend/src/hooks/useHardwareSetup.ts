import { useEffect, useState, useCallback } from 'react'
import { useNats } from './useNats'
import type { HardwareSetupState, PinState } from '../types/hardware-setup'

interface HardwareSetupHookState {
  armed: boolean
  pins: PinState[]
  loading: boolean
}

export function useHardwareSetup() {
  const { subscribe, request, connected } = useNats()
  const [state, setState] = useState<HardwareSetupHookState>({
    armed: false,
    pins: [],
    loading: false,
  })

  // Subscribe to live state updates
  useEffect(() => {
    if (!connected) return

    subscribe<HardwareSetupState>('hardware-setup.v1.live.state', (setupState) => {
      setState(prev => ({
        ...prev,
        armed: setupState.armed,
        pins: setupState.pins,
      }))
    }).catch(console.error)
  }, [connected, subscribe])

  const arm = useCallback(async () => {
    setState(prev => ({ ...prev, loading: true }))
    try {
      const response = await request<HardwareSetupState>('hardware-setup.v1.arm', {})
      setState({
        armed: response.armed,
        pins: response.pins,
        loading: false,
      })
    } catch (error) {
      console.error('Failed to arm hardware setup:', error)
      setState(prev => ({ ...prev, loading: false }))
    }
  }, [request])

  const disarm = useCallback(async () => {
    setState(prev => ({ ...prev, loading: true }))
    try {
      const response = await request<HardwareSetupState>('hardware-setup.v1.disarm', {})
      setState({
        armed: response.armed,
        pins: response.pins,
        loading: false,
      })
    } catch (error) {
      console.error('Failed to disarm hardware setup:', error)
      setState(prev => ({ ...prev, loading: false }))
    }
  }, [request])

  return {
    ...state,
    connected,
    arm,
    disarm,
  }
}
