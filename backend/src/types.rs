//! Types for the Stopwatch service based on stopwatch.md spec

use serde::{Deserialize, Serialize};

// ============================================================================
// FSM States
// ============================================================================

/// Stopwatch finite state machine states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum StopwatchState {
    #[default]
    Disarmed, // Triggers ignored
    Armed,    // Triggers will start timing
    Running,  // Triggers create laps
}

// ============================================================================
// System Service Types
// ============================================================================

/// Ping request (empty, just triggers a response)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PingRequest {}

/// Ping response with server timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingResponse {
    pub server_time_ns: u64,
}

// ============================================================================
// Trigger Types
// ============================================================================

/// Trigger event published to stopwatch.v1.trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerEvent {
    pub timestamp_ns: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

// ============================================================================
// Command Types (Request/Reply)
// ============================================================================

/// Empty request for arm/unarm/reset/get_state commands
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmptyRequest {}

/// State snapshot response for commands and live events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub state: StopwatchState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<u64>,
    pub elapsed_ns: u64,
    pub server_time_ns: u64,
    pub lap_count: u32,
    pub running: bool,
}

// ============================================================================
// Live Event Types
// ============================================================================

/// Periodic tick update published to stopwatch.v1.live.tick at 5-10 Hz while running
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickUpdate {
    pub elapsed_ns: u64,
    pub server_time_ns: u64,
    pub lap_id: String,
}

/// Lap event type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LapEventType {
    Start,
    Finish,
}

/// Lap event published to stopwatch.v1.live.lap
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LapEvent {
    pub event_type: LapEventType,
    pub run_id: u64,
    pub lap: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lap_time_ns: Option<u64>,
    pub total_time_ns: u64,
    pub server_time_ns: u64,
}

// ============================================================================
// History Types (Request/Reply)
// ============================================================================

/// Request to list historical runs
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListRunsRequest {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

fn default_limit() -> u32 {
    100
}

/// Run summary for list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub run_id: u64,
    pub start_time_ns: u64,
    pub end_time_ns: Option<u64>,
    pub total_time_ns: u64,
    pub lap_count: u32,
}

/// Response for list runs query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRunsResponse {
    pub runs: Vec<RunSummary>,
    pub total: u32,
}

/// Request to get a specific run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRunRequest {
    pub run_id: u64,
}

/// Lap detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LapDetail {
    pub lap: String,
    pub lap_time_ns: u64,
    pub total_time_ns: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_ns: Option<u64>,
}

/// Response for get run query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRunResponse {
    pub run_id: u64,
    pub start_time_ns: u64,
    pub end_time_ns: Option<u64>,
    pub total_time_ns: u64,
    pub lap_count: u32,
    pub laps: Vec<LapDetail>,
}

/// Request to get laps for a run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetLapsRequest {
    pub run_id: u64,
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

/// Response for get laps query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetLapsResponse {
    pub run_id: u64,
    pub laps: Vec<LapDetail>,
    pub total: u32,
}

// ============================================================================
// Error Response
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
}

impl ErrorResponse {
    pub fn new(error: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: code.into(),
        }
    }

    pub fn invalid_transition(from: StopwatchState, action: &str) -> Self {
        Self::new(
            format!("Cannot {} from {:?} state", action, from),
            "INVALID_TRANSITION",
        )
    }
}

// ============================================================================
// GPIO Configuration Types
// ============================================================================

/// GPIO pin configuration from gpio.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpioPinConfig {
    pub pin: u32,
    pub name: String,
    /// If true, pin is active when high (rising edge = active). If false, pin is active when low (falling edge = active).
    #[serde(default = "default_active_high")]
    pub active_high: bool,
}

fn default_active_high() -> bool {
    true
}

/// Debounce configuration from gpio.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpioDebounceConfig {
    pub quiet_period_ms: u64,
    pub max_wait_ms: u64,
}

impl Default for GpioDebounceConfig {
    fn default() -> Self {
        Self {
            quiet_period_ms: 200,
            max_wait_ms: 3000,
        }
    }
}

/// GPIO configuration loaded from gpio.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpioConfig {
    pub chip: String,
    #[serde(default)]
    pub debounce: GpioDebounceConfig,
    pub pins: Vec<GpioPinConfig>,
}

impl Default for GpioConfig {
    fn default() -> Self {
        Self {
            chip: "/dev/gpiochip0".to_string(),
            debounce: GpioDebounceConfig::default(),
            pins: vec![GpioPinConfig {
                pin: 1,
                name: "Start/Finish Line".to_string(),
                active_high: true,
            }],
        }
    }
}

// ============================================================================
// Hardware Setup Types
// ============================================================================

/// State of a single GPIO pin (with name from config)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinState {
    pub pin: u32,
    pub name: String,
    pub active: bool, // true = beam broken/closed, false = beam clear/open
}

/// Hardware setup state for IR sensor alignment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareSetupState {
    pub armed: bool,
    pub pins: Vec<PinState>,
}

// ============================================================================
// Auth Response
// ============================================================================

/// Authentication response enum with jwt and error variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthResponse {
    Jwt {
        jwt: String,
    },
    Error {
        error: String,
        code: String,
    },
}

impl AuthResponse {
    pub fn jwt(jwt: impl Into<String>) -> Self {
        Self::Jwt { jwt: jwt.into() }
    }

    pub fn error(error: impl Into<String>, code: impl Into<String>) -> Self {
        Self::Error {
            error: error.into(),
            code: code.into(),
        }
    }

    pub fn from_error_response(err: ErrorResponse) -> Self {
        Self::Error {
            error: err.error,
            code: err.code,
        }
    }
}
