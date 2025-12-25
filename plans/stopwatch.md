# System Service

## Time heartbeat (pub/sub, lossy)

```
system.v1.heartbeat
```

> Server publishes current monotonic time at regular intervals (2 Hz).
> Clients use this to measure network latency and dynamically adjust intentional lag.
> This is a separate service from the stopwatch, allowing other clients to also consume heartbeats.


Published to `system.v1.heartbeat` at 2 Hz (500 ms intervals).

```json
{
  "server_time_ns": 9876543210123456789
}
```

**Fields:**
- `server_time_ns` (required, `u64`): Server's current monotonic time





# Stopwatch Service – Implementation Plan (Integrated + Trigger Abstraction)

## 1. Core goals
- **Trigger‑driven authoritative timing system**
- Arm / unarm control via NATS
- Trigger via NATS
- Live snapshot‑based lap + state events
- SQL‑backed history queries
- Browser UI via NATS WebSocket
- Client‑side predictive rendering with intentional lag
- Client never displays time the server has not confirmed

---



## 2. Subject layout

| Subject | Type | Description | Notes |
|---------|------|-------------|-------|
| `stopwatch.v1.arm` | Request/Reply | Arm the stopwatch (Disarmed → Armed) | State command |
| `stopwatch.v1.unarm` | Request/Reply | Unarm the stopwatch (Armed → Disarmed, Running → Disarmed) | State command |
| `stopwatch.v1.reset` | Request/Reply | Reset the stopwatch (*any* → Armed) | State command |
| `stopwatch.v1.get_state` | Request/Reply | Get current stopwatch state | State command |
| `stopwatch.v1.list_runs` | Request/Reply | List historical runs | History command |
| `stopwatch.v1.get_run` | Request/Reply | Get details for a specific run | History command |
| `stopwatch.v1.get_laps` | Request/Reply | Get laps for a specific run | History command |
| `stopwatch.v1.trigger` | Pub/Sub | Logical trigger pulses | Any source may publish (GPIO, keyboard, test harness). Represents logical pulses, not physical GPIO. |
| `stopwatch.v1.live.state` | Pub/Sub | Stopwatch state changes | Published whenever state changes (armed, unarmed, reset, etc.) |
| `stopwatch.v1.live.tick` | Pub/Sub | Periodic time updates | Published at 5–10 Hz while running. Allows frontend to display time accurately in real-time. |
| `stopwatch.v1.live.lap` | Pub/Sub (JetStream) | Lap start/finish events | Published whenever a lap starts or stops. JetStream enabled: server records X minutes of data, client can request Y minutes on re-connect. |


---

## 3. FSM states (authoritative on server)

```text
Disarmed – Triggers ignored
Armed    – Triggers start timing
Running  – Triggers create laps
```

### Transitions
- `arm`: Disarmed → Armed
- `reset`: !Disarmed → Armed (clear run)
- `unarm`: Armed → Disarmed, Running → Disarmed
- `trigger` Armed → Running (create run, lap 1)
- `trigger` Running → Running (next lap)

---

## 4. Identifiers & invariants

- `run_id`
  - Created + incremented on **first trigger**
  - `None` when not running
- `lap`
  - Starts at 1
  - Increments per trigger

---

## 5. Trigger abstraction

### Concept
- The stopwatch service **does not know about GPIO**
- It consumes a stream of **logical trigger events**
- Trigger events are **edge‑like** and stateless
- The trigger source should perform basic noise filtering or debounce to prevent spurious trigger events

### Trigger event semantics
- Each message represents **one pulse**
- Payload must include `timestamp_ns` (required)
- Optional metadata allowed (`source`)
- Ordering matters, delivery is best‑effort

Example payload:
```json
{
  "timestamp_ns": 1234567890123456789,
  "source": "gpio" | "keyboard" | "test" | ""
}
```

The `timestamp_ns` field:
- Required: all trigger events must include it
- Represents when the trigger actually occurred (from the trigger source's perspective)
- Uses monotonic clock time (nanoseconds)
- Should be captured as close to the physical trigger event as possible (e.g., in GPIO interrupt handler)

### Trigger sources
- GPIO service (production)
- Keyboard / UI tool (development)
- Test harness / replay (testing)

All publish to:
```
stopwatch.v1.trigger
```

---

## 6. Time model (critical)

- Server uses monotonic clock only
- Server is sole authority for elapsed time
- Clients never accumulate time without bounds

All live snapshots include:
- `elapsed_ns`
- `server_time_ns`
- `running`

---

## 7. Data model

All data structures use JSON serialization (via Serde in Rust). All timestamps use monotonic clock time in nanoseconds (`u64`).

### Enums

#### `StopwatchState`
```rust
enum StopwatchState {
    Disarmed,  // Triggers ignored
    Armed,     // Triggers will start timing
    Running,   // Triggers create laps
    Stopped,   // Timing stopped, holding last run
}
```

### Commands (request/reply)

Commands are sent to `stopwatch.v1.*` subjects (NATS service system). All commands may include optional metadata fields.

#### `CmdArm`
**Request:**
```json
{}
```
**Response:** `StateSnapshot`

#### `CmdUnarm`
**Request:**
```json
{}
```
**Response:** `StateSnapshot`

#### `CmdReset`
**Request:**
```json
{}
```
**Response:** `StateSnapshot` (state transitions to Armed)

#### `CmdState`
**Request:**
```json
{}
```
**Response:** `StateSnapshot` (current state)

### Trigger events

#### `TriggerEvent`
Published to `stopwatch.v1.trigger`

```json
{
  "timestamp_ns": 1234567890123456789,
  "source": "gpio" | "keyboard" | "test" | ""
}
```

**Fields:**
- `timestamp_ns` (required, `u64`): Monotonic nanosecond timestamp when trigger occurred (from trigger source's perspective)
- `source` (optional, `string`): Source identifier for debugging/monitoring

### Live events

#### `TickUpdate`
Published to `stopwatch.v1.live.tick` at 5–10 Hz while running.

```json
{
  "elapsed_ns": 1234567890,
  "server_time_ns": 9876543210123456789
}
```

**Fields:**
- `elapsed_ns` (required, `u64`): Elapsed time since run started (nanoseconds)
- `server_time_ns` (required, `u64`): Server's current monotonic time when snapshot was taken

#### `LapEvent`
Published to `stopwatch.v1.live.lap` on each trigger while running. Published twice per trigger: once for lap finish (previous lap), once for lap start (new lap).

**Lap finish event:**
```json
{
  "event_type": "finish",
  "run_id": 42,
  "lap": 2,
  "lap_time_ns": 1234567890,
  "total_time_ns": 2469135780,
  "server_time_ns": 9876543210123456789
}
```

**Lap start event:**
```json
{
  "event_type": "start",
  "run_id": 42,
  "lap": 3,
  "total_time_ns": 2469135780,
  "server_time_ns": 9876543210123456789
}
```

**Fields:**
- `event_type` (required, `"start" | "finish"`): Whether this event marks the start or finish of a lap
- `run_id` (required, `u64`): Current run identifier
- `lap` (required, `u32`): Lap number (starts at 1)
- `lap_time_ns` (optional, `u64`): Time for the completed lap (nanoseconds). Only present on `"finish"` events.
- `total_time_ns` (required, `u64`): Total elapsed time since run started (nanoseconds)
- `server_time_ns` (required, `u64`): Server's current monotonic time when lap event was recorded

#### `StateSnapshot`
Published to `stopwatch.v1.live.state` on state transitions and in response to commands.

```json
{
  "state": "Running",
  "run_id": 42,
  "elapsed_ns": 1234567890,
  "server_time_ns": 9876543210123456789,
  "lap_count": 3,
  "running": true
}
```

**Fields:**
- `state` (required, `StopwatchState`): Current FSM state
- `run_id` (required, `u64`): Current run identifier (0 if no active run)
- `elapsed_ns` (required, `u64`): Elapsed time since run started (0 if not running)
- `server_time_ns` (required, `u64`): Server's current monotonic time when snapshot was taken
- `lap_count` (required, `u32`): Number of laps completed (0 if not running)
- `running` (required, `bool`): Whether stopwatch is currently running

### History queries (request/reply)

#### `HistoryListRuns`
**Request:** Published to `stopwatch.v1.hist_list_runs`
```json
{
  "limit": 100,
  "offset": 0
}
```

**Response:**
```json
{
  "runs": [
    {
      "run_id": 42,
      "start_time_ns": 9876543210123456789,
      "end_time_ns": 9876543211234567890,
      "total_time_ns": 1111111101,
      "lap_count": 5
    }
  ],
  "total": 100
}
```

#### `HistoryGetRun`
**Request:** Published to `stopwatch.v1.hist_get_run`
```json
{
  "run_id": 42
}
```

**Response:**
```json
{
  "run_id": 42,
  "start_time_ns": 9876543210123456789,
  "end_time_ns": 9876543211234567890,
  "total_time_ns": 1111111101,
  "lap_count": 5,
  "laps": [
    {
      "lap": 1,
      "lap_time_ns": 222222222,
      "total_time_ns": 222222222
    }
  ]
}
```

#### `HistoryGetLaps`
**Request:** Published to `stopwatch.v1.hist_get_laps`
```json
{
  "run_id": 42,
  "limit": 50,
  "offset": 0
}
```

**Response:**
```json
{
  "run_id": 42,
  "laps": [
    {
      "lap": 1,
      "lap_time_ns": 222222222,
      "total_time_ns": 222222222,
      "timestamp_ns": 9876543210123456789
    }
  ],
  "total": 5
}
```

---

## 8. Stopwatch service responsibilities

### NATS handlers
- Register as NATS service `stopwatch.v1` (handles all commands via service system)
- Subscribe to:
  - `stopwatch.v1.trigger` (pub/sub)
- Validate FSM transitions
- Mutate in‑memory state
- Publish snapshot events
- Reply to commands (via NATS service system)

### Trigger handling logic
- If `Disarmed` → ignore
- If `Armed`:
  - Create new run (`run_id += 1`)
  - Start timer
  - Emit lap 1 snapshot
  - Emit running state snapshot
- If `Running`:
  - Emit lap snapshot
- If `Stopped` → ignore

---

## 9. Publish behaviour

- Publish snapshots on:
  - state transitions
  - each trigger
- While `Running`:
  - Publish periodic state snapshots (5–10 Hz) (200-100 ms)
- Continuously (regardless of state):
  - Publish time heartbeat at 2 Hz (500 ms intervals) on `system.v1.heartbeat`
- No buffering
- No per‑consumer rates

---

## 10. History storage (SQL)

- Subscribe to live events
- Best‑effort ingestion
- Never block live timing

---

## 11. Frontend behaviour

- Subscribe to live events
- Subscribe to time heartbeat (`system.v1.heartbeat`)
- Request initial state
- Drop old snapshots
- Intentional lag (dynamically adjusted based on network latency)
- Freeze > lie

### Dynamic lag calculation
- Measure network latency using time heartbeat:
  - Record local time when heartbeat received
  - Calculate latency: `local_time - server_time_ns`
  - Track rolling average/minimum latency
- Adjust intentional lag based on measured latency:
  - Use measured latency + safety margin (e.g., 2x minimum observed latency)
  - Ensures client never displays time server hasn't confirmed
  - Adapts to changing network conditions
