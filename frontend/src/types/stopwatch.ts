export type StopwatchState = 'Disarmed' | 'Armed' | 'Running' | 'Stopped'

export interface TriggerEvent {
  timestamp_ns: number
  source?: string
}

export interface TickUpdate {
  elapsed_ns: number
  server_time_ns: number
  lap_id: number
}

export interface LapEvent {
  event_type: 'start' | 'finish'
  run_id: number
  lap: number
  lap_time_ns?: number
  total_time_ns: number
  server_time_ns: number
}

export interface StateSnapshot {
  state: StopwatchState
  run_id: number
  elapsed_ns: number
  server_time_ns: number
  lap_count: number
  running: boolean
}

export interface PingResponse {
  server_time_ns: number
}

export interface Run {
  run_id: number
  start_time_ns: number
  end_time_ns: number
  total_time_ns: number
  lap_count: number
}

export interface Lap {
  lap: number
  lap_time_ns: number
  total_time_ns: number
  timestamp_ns?: number
}

