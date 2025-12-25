# Hardware Setup Page

## Overview

Add a hardware setup page that displays real-time GPIO pin states to help align IR diode/sensor pairs. The feature includes authenticated arm/disarm controls and a live ~10Hz stream of pin states via NATS.

## Architecture

```
GPIO Trigger (owns chip) --publishes--> stopwatch.v1.trigger ---> Stopwatch Service
                         --publishes--> hardware-setup.v1.live.state ---> Frontend Setup Page
```

- **Setup mode lives entirely in the GPIO trigger code** (since it owns the chip)
- **Stopwatch continues working normally** - it still receives trigger events during setup mode
- **No conflict** - setup just reads pin values alongside the existing edge monitoring

## NATS Subject Design

Using prefix `hardware-setup.v1`:

| Subject | Type | Purpose |
|---------|------|---------|
| `hardware-setup.v1.arm` | Request/Reply | Enter setup mode, returns current `HardwareSetupState` |
| `hardware-setup.v1.disarm` | Request/Reply | Exit setup mode, returns final `HardwareSetupState` |
| `hardware-setup.v1.live.state` | Publish | Live pin states at ~10Hz while armed |

Note: `get_state` is not needed - clients can call `arm` (idempotent) and subscribe to `live.state` to get updates within 100ms.

### Message Types

```rust
struct HardwareSetupState {
    armed: bool,
    pins: Vec<PinState>,
}

struct PinState {
    pin: u32,
    active: bool,  // true = beam broken/closed, false = beam clear/open
}
```

## Implementation Tasks

### Backend

1. **Add types to `backend/src/types.rs`**
   - `HardwareSetupState` struct
   - `PinState` struct

2. **Expand `backend/src/trigger/gpio_redundant.rs`**
   - Handle `hardware-setup.v1.arm` / `hardware-setup.v1.disarm` NATS commands
   - When armed, poll pin values and publish `hardware-setup.v1.live.state` at ~10Hz
   - Keep existing trigger event monitoring (coexists with setup mode)

3. **Remove single GPIO trigger**
   - Delete `backend/src/trigger/gpio.rs`
   - Update `backend/src/trigger/mod.rs` to remove `TriggerConfig::Gpio`
   - Update `backend/src/bin/server.rs` to remove `TriggerSource::Gpio` CLI option
   - The redundant trigger works fine with a single pin

### Frontend

4. **Add TypeScript types in `frontend/src/types/hardware-setup.ts`**
   - `HardwareSetupState` interface
   - `PinState` interface

5. **Create `frontend/src/hooks/useHardwareSetup.ts`**
   - Subscribe to `hardware-setup.v1.live.state`
   - Request handlers for arm/disarm
   - Track pin states array

6. **Create `frontend/src/components/HardwareSetupPage.tsx`**
   - Visual grid of pin indicators (adapts to pin count)
   - Green/red indicators for clear/broken beam states
   - Arm/Disarm toggle requiring authentication
   - Reuse existing auth pattern from StopwatchPage

7. **Add `/setup` route**
   - Create `frontend/src/routes/setup.tsx`

8. **Add Setup button in StopwatchPage header**
   - Only visible when authenticated
   - Links to `/setup` page

## Auth Configuration

The existing admin PIN (1234) with `>` permissions will work. For restricted access, specific permissions can be added:

```yaml
publish:
  - hardware-setup.v1.arm
  - hardware-setup.v1.disarm
subscribe:
  - hardware-setup.v1.>
```

## UI Design

- Header with arm/disarm toggle and auth status (reusing existing pattern)
- Grid of pin indicators that adapts to the number of configured pins
- Each indicator shows: pin number, visual state (green = clear, red = broken)
- Clear visual feedback for IR alignment testing
