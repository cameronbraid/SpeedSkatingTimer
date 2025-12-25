import { useEffect, useRef, useState, useCallback, createContext, useContext, ReactNode } from 'react'
import { wsconnect, NatsConnection, Subscription, jwtAuthenticator, ConnectionOptions } from '@nats-io/nats-core'
import { jetstream, DeliverPolicy } from '@nats-io/jetstream'

// Import default JWT - Vite will inline it at build time
// @ts-ignore - Vite handles ?raw imports
import defaultJwt from '../assets/frontend.jwt?raw'
// @ts-ignore - Vite handles ?raw imports
import defaultSeed from '../assets/frontend.seed?raw'


export interface NatsConnectionState {
  connected: boolean
  authenticated: boolean
  error: string | null
}

export interface NatsContextValue extends NatsConnectionState {
  subscribe: <T = unknown>(subject: string, callback: (data: T) => void) => Promise<void>
  request: <T = unknown>(subject: string, payload?: unknown) => Promise<T>
  publish: (subject: string, payload?: unknown) => Promise<void>
  authenticate: (pin: string) => Promise<string>
  logout: () => Promise<void>
  subscribeJetStream: <T = unknown>(
    stream: string,
    subject: string,
    maxAgeMinutes: number,
    callback: (message: T) => void,
  ) => Promise<{ messages: T[], unsubscribe: () => void }>
}

const NatsContext = createContext<NatsContextValue | null>(null)

// Hook to use the shared NATS connection
export function useNats(): NatsContextValue {
  const context = useContext(NatsContext)
  if (!context) {
    throw new Error('useNats must be used within a NatsProvider')
  }
  return context
}

interface NatsProviderProps {
  children: ReactNode
}

export function NatsProvider({ children }: NatsProviderProps) {
  const [state, setState] = useState<NatsConnectionState>({
    connected: false,
    authenticated: false,
    error: null,
  })
  const ncRef = useRef<NatsConnection | null>(null)
  const subscriptionsRef = useRef<Map<string, Subscription>>(new Map())
  const jetStreamConsumersRef = useRef<Set<() => void>>(new Set())
  const isConnectingRef = useRef(false)
  const jwtRef = useRef(defaultJwt)
  const jwtInitializedRef = useRef(false)

  const connectNats = useCallback(async (forceNew = false) => {
    // If force reconnect is requested, close existing connection first
    if (forceNew && ncRef.current) {
      // Clear subscriptions before closing
      subscriptionsRef.current.forEach((sub) => sub.unsubscribe())
      subscriptionsRef.current.clear()

      // Clean up JetStream consumers before closing connection
      jetStreamConsumersRef.current.forEach((unsubscribe) => {
        try {
          unsubscribe()
        } catch (e) {
          // Ignore errors during cleanup
        }
      })
      jetStreamConsumersRef.current.clear()

      if (!ncRef.current.isClosed()) {
        await ncRef.current.close()
        await ncRef.current.closed()
      }
      ncRef.current = null
      isConnectingRef.current = false
      await new Promise(resolve => setTimeout(resolve, 100))
    }

    if (ncRef.current && !ncRef.current.isClosed() && !forceNew) {
      return ncRef.current
    }

    if (isConnectingRef.current) {
      return new Promise<NatsConnection>((resolve, reject) => {
        const checkConnection = setInterval(() => {
          if (ncRef.current && !ncRef.current.isClosed()) {
            clearInterval(checkConnection)
            resolve(ncRef.current)
          } else if (!isConnectingRef.current) {
            clearInterval(checkConnection)
            reject(new Error('Connection failed'))
          }
        }, 100)
      })
    }

    isConnectingRef.current = true

    try {
      const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
      const serverUrl = import.meta.env.VITE_NATS_URL || `${wsProtocol}//${window.location.host}/nats`

      const connectOptions: ConnectionOptions = {
        servers: serverUrl,
        maxReconnectAttempts: -1,
        authenticator: jwtAuthenticator(
          () => jwtRef.current,
          () => new TextEncoder().encode(defaultSeed.trim()),
        ),
      }

      const nc = await wsconnect(connectOptions)

      if (nc.isClosed()) {
        throw new Error('Connection closed during authentication')
      }

      ncRef.current = nc

      nc.closed().then(() => {
        setState(prev => ({ ...prev, connected: false, authenticated: false }))
        subscriptionsRef.current.clear()
        // Clean up JetStream consumers when connection closes
        jetStreamConsumersRef.current.forEach((unsubscribe) => {
          try {
            unsubscribe()
          } catch (e) {
            // Ignore errors during cleanup
          }
        })
        jetStreamConsumersRef.current.clear()
        isConnectingRef.current = false
      })

      setState(prev => ({
        ...prev,
        connected: true,
        error: null,
      }))
      isConnectingRef.current = false

      return nc
    } catch (error) {
      isConnectingRef.current = false
      const errorMessage = error instanceof Error ? error.message : 'Unknown error'
      setState(prev => ({ ...prev, connected: false, error: errorMessage }))
      throw error
    }
  }, [])

  const subscribe = useCallback(
    async <T = unknown>(subject: string, callback: (data: T) => void) => {
      if (subscriptionsRef.current.has(subject)) {
        return
      }

      const nc = await connectNats()
      const sub = nc.subscribe(subject)
      subscriptionsRef.current.set(subject, sub)

        ; (async () => {
          for await (const msg of sub) {
            try {
              const parsed = msg.json<T>()
              callback(parsed)
            } catch (error) {
              console.error(`Failed to parse message on ${subject}:`, error)
            }
          }
        })().catch(() => {
          subscriptionsRef.current.delete(subject)
        })
    },
    [connectNats]
  )

  const request = useCallback(
    async <T = unknown>(subject: string, payload?: unknown): Promise<T> => {
      const nc = await connectNats()
      const encoder = new TextEncoder()
      const decoder = new TextDecoder()

      const data = payload ? encoder.encode(JSON.stringify(payload)) : new Uint8Array(0)
      const response = await nc.request(subject, data, { timeout: 5000 })
      return JSON.parse(decoder.decode(response.data)) as T
    },
    [connectNats]
  )

  const publish = useCallback(
    async (subject: string, payload?: unknown) => {
      const nc = await connectNats()
      const encoder = new TextEncoder()
      const data = payload ? encoder.encode(JSON.stringify(payload)) : new Uint8Array(0)
      nc.publish(subject, data)
    },
    [connectNats]
  )

  const authenticate = useCallback(
    async (pin: string): Promise<string> => {
      const nc = await connectNats()
      const encoder = new TextEncoder()
      const pinData = encoder.encode(pin)

      const response = await nc.request('system.v1.auth', pinData, { timeout: 5000 })
      const authResponse = await response.json<{ type: 'jwt' | 'error'; jwt?: string; error?: string; code?: string }>()

      if (authResponse.type === 'error') {
        const errorMessage = authResponse.error || authResponse.code || 'Authentication failed'
        throw new Error(errorMessage)
      }

      if (authResponse.type === 'jwt' && authResponse.jwt) {
        const jwt = authResponse.jwt

        // Store JWT and reconnect
        jwtRef.current = jwt
        localStorage.setItem('nats_jwt', jwt)

        await connectNats(true)

        setState(prev => ({ ...prev, authenticated: true }))

        return jwt
      }

      throw new Error('Invalid auth response format')
    },
    [connectNats]
  )

  const logout = useCallback(
    async () => {
      // Remove JWT from localStorage
      localStorage.removeItem('nats_jwt')

      // Reset JWT to default
      jwtRef.current = defaultJwt

      // Update state to unauthenticated
      setState(prev => ({ ...prev, authenticated: false }))

      // Reconnect with default JWT
      await connectNats(true)
    },
    [connectNats]
  )

  const subscribeJetStream = useCallback(
    async <T = unknown>(
      streamName: string,
      subject: string,
      maxAgeMinutes: number,
      callback: (message: T) => void,
    ): Promise<{ messages: T[], unsubscribe: () => void }> => {
      const nc = await connectNats()

      try {
        // Create JetStream client
        const js = jetstream(nc)

        // Get the stream
        const stream = await js.streams.get(streamName)

        // Calculate start time for replay
        const startTime = new Date(Date.now() - maxAgeMinutes * 60 * 1000)

        // Create an ordered consumer that replays from the start time
        // Ordered consumers are ephemeral and handle message ordering automatically
        const consumer = await stream.getConsumer({
          deliver_policy: DeliverPolicy.StartTime,
          opt_start_time: startTime.toISOString(),
          filter_subjects: [subject],
        })

        // Start consuming messages
        const consumerMessages = await consumer.consume()

        // Collect messages and stream them via callback
        const messages: T[] = []
        let isActive = true

        // Process messages continuously
        const processMessages = async () => {
          try {
            for await (const msg of consumerMessages) {
              if (!isActive) break

              try {
                const parsed = msg.json() as T
                messages.push(parsed)

                callback(parsed)

              } catch (error) {
                console.error('Failed to parse JetStream message:', error)
              }
            }
          } catch (error) {
            if (isActive) {
              console.error('Error in JetStream message stream:', error)
            }
          }
        }

        // Create unsubscribe function
        const unsubscribe = () => {
          isActive = false
          consumerMessages.close()
          jetStreamConsumersRef.current.delete(unsubscribe)
        }

        // Register for cleanup during reconnection
        jetStreamConsumersRef.current.add(unsubscribe)

        // Start processing messages
        processMessages().catch(console.error)

        // Wait a moment to collect initial historical messages before returning
        await new Promise(resolve => setTimeout(resolve, 500))

        return {
          messages,
          unsubscribe
        }
      } catch (error) {
        console.error('Failed to fetch JetStream messages:', error)
        // Return empty array on error - live subscription will still work
        return {
          messages: [],
          unsubscribe: () => { }
        }
      }
    },
    [connectNats]
  )

  useEffect(() => {
    if (!jwtInitializedRef.current) {
      const storedJwt = localStorage.getItem('nats_jwt')
      if (storedJwt) {
        jwtRef.current = storedJwt
        if (storedJwt !== defaultJwt) {
          setState(prev => ({ ...prev, authenticated: true }))
        }
      }
      jwtInitializedRef.current = true
    }

    connectNats().catch(() => { })

    return () => {
      subscriptionsRef.current.forEach((sub) => sub.unsubscribe())
      subscriptionsRef.current.clear()

      // Clean up JetStream consumers
      jetStreamConsumersRef.current.forEach((unsubscribe) => {
        try {
          unsubscribe()
        } catch (e) {
          // Ignore errors during cleanup
        }
      })
      jetStreamConsumersRef.current.clear()

      if (ncRef.current) {
        ncRef.current.close()
        ncRef.current = null
      }
    }
  }, [connectNats])

  const contextValue: NatsContextValue = {
    ...state,
    subscribe,
    request,
    publish,
    authenticate,
    logout,
    subscribeJetStream,
  }

  return (
    <NatsContext.Provider value={contextValue}>
      {children}
    </NatsContext.Provider>
  )
}
