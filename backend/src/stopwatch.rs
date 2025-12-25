//! Stopwatch service implementation
//!
//! Implements the FSM-based stopwatch with NATS service API integration.

use std::sync::Arc;
use std::time::Duration;

use async_nats::service::ServiceExt;
use async_nats::jetstream::{self, Context};
use async_nats::Client;
use color_eyre::eyre::eyre;
use color_eyre::Result;
use futures::StreamExt;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::trigger::{clock_monotonic_ns, TRIGGER_SUBJECT};
use crate::types::*;

// ============================================================================
// NATS Subjects
// ============================================================================

pub mod subjects {
    pub const LIVE_STATE: &str = "stopwatch.v1.live.state";
    pub const LIVE_TICK: &str = "stopwatch.v1.live.tick";
    pub const LIVE_LAP: &str = "stopwatch.v1.live.lap";
}

/// Maximum lap duration in nanoseconds (99.999 seconds)
const MAX_LAP_DURATION_NS: u64 = 99_999_000_000;

// ============================================================================
// Stopwatch State Machine
// ============================================================================

/// Internal lap data
#[derive(Debug, Clone)]
struct Lap {
    lap: String,
    start_ns: u64,
    end_ns: Option<u64>,
}

impl Lap {
    fn lap_time_ns(&self) -> Option<u64> {
        self.end_ns.map(|end| end - self.start_ns)
    }
}

/// Internal run data
#[derive(Debug, Clone)]
struct Run {
    run_id: u64,
    start_ns: u64,
    last_trigger_ns: u64, // Last trigger timestamp - basis for elapsed time
    laps: Vec<Lap>,
}

impl Run {
    fn new(run_id: u64, start_ns: u64) -> Self {
        Self {
            run_id,
            start_ns,
            last_trigger_ns: start_ns, // Initialize with first trigger timestamp
            laps: vec![Lap {
                lap: nanoid::nanoid!(), // Lap ID is a nanoid string
                start_ns,
                end_ns: None,
            }],
        }
    }

    fn current_lap(&self) -> Option<&Lap> {
        self.laps.last()
    }

    fn lap_count(&self) -> u32 {
        self.laps.iter().filter(|lap| lap.end_ns.is_some()).count() as u32
    }

    fn total_elapsed_ns(&self) -> u64 {
        // Use last trigger timestamp as basis - don't compare with now
        // Ensure start_ns is valid (not 0 or uninitialized)
        if self.start_ns == 0 {
            return 0;
        }
        self.last_trigger_ns.saturating_sub(self.start_ns)
    }

    /// Record a trigger, finishing current lap and starting new one
    fn record_trigger(&mut self, timestamp_ns: u64) -> (Lap, Lap) {
        // Finish current lap
        let finished_lap = {
            let current = self.laps.last_mut().unwrap();
            current.end_ns = Some(timestamp_ns);
            current.clone()
        };

        // Start new lap with lap ID as nanoid string
        let new_lap = Lap {
            lap: nanoid::nanoid!(),
            start_ns: timestamp_ns,
            end_ns: None,
        };
        self.laps.push(new_lap.clone());

        (finished_lap, new_lap)
    }
}

/// Stopwatch state machine
#[derive(Debug)]
pub struct Stopwatch {
    state: StopwatchState,
    next_run_id: u64,
    current_run: Option<Run>,
    // History storage (in-memory for now, could be backed by SQL)
    history: Vec<Run>,
}

impl Default for Stopwatch {
    fn default() -> Self {
        Self::new()
    }
}

impl Stopwatch {
    pub fn new() -> Self {
        Self {
            state: StopwatchState::Disarmed,
            next_run_id: 1,
            current_run: None,
            history: Vec::new(),
        }
    }

    /// Get current state snapshot
    pub fn snapshot(&self) -> StateSnapshot {
        let now = clock_monotonic_ns();
        let (run_id, elapsed_ns, lap_count) = match &self.current_run {
            Some(run) => {
                // If running, use last trigger timestamp as basis; otherwise use the end time of the last lap
                let elapsed = if self.state == StopwatchState::Running {
                    run.total_elapsed_ns()
                } else {
                    // For non-running runs (shouldn't happen, but handle gracefully), use the end time of the last finished lap
                    run.laps.last()
                        .and_then(|lap| lap.end_ns)
                        .map(|end_ns| end_ns.saturating_sub(run.start_ns))
                        .unwrap_or(0)
                };
                (Some(run.run_id), elapsed, run.lap_count())
            }
            None => (None, 0, 0),
        };

        StateSnapshot {
            state: self.state,
            run_id,
            elapsed_ns,
            server_time_ns: now,
            lap_count,
            running: self.state == StopwatchState::Running,
        }
    }

    /// Get tick update for live feed
    pub fn tick_update(&self) -> Option<TickUpdate> {
        if self.state != StopwatchState::Running {
            return None;
        }

        let now = clock_monotonic_ns(); // Use CLOCK_MONOTONIC to match trigger timestamps
        self.current_run.as_ref().map(|run| {
            // Calculate current lap time: time since current lap started
            // Use CLOCK_MONOTONIC time minus the current lap's start time (both in CLOCK_MONOTONIC)
            let (current_lap_time, lap_id) = run.current_lap()
                .map(|lap| {
                    // Current lap time = current CLOCK_MONOTONIC - lap start time (both CLOCK_MONOTONIC)
                    // lap.start_ns is from trigger event, which uses CLOCK_MONOTONIC
                    (now.saturating_sub(lap.start_ns), lap.lap.clone())
                })
                .expect("Run should always have at least one lap"); // Run::new() always creates lap 1
            
            TickUpdate {
                elapsed_ns: current_lap_time,
                server_time_ns: now,
                lap_id,
            }
        })
    }

    /// Arm the stopwatch (Disarmed -> Armed)
    pub fn arm(&mut self) -> Result<StateSnapshot, ErrorResponse> {
        match self.state {
            StopwatchState::Disarmed => {
                self.state = StopwatchState::Armed;
                info!("Stopwatch armed");
                Ok(self.snapshot())
            }
            _ => Err(ErrorResponse::invalid_transition(self.state, "arm")),
        }
    }

    /// Unarm the stopwatch (Armed -> Disarmed, Running -> Disarmed)
    pub fn unarm(&mut self) -> Result<StateSnapshot, ErrorResponse> {
        match self.state {
            StopwatchState::Armed => {
                self.state = StopwatchState::Disarmed;
                info!("Stopwatch unarmed (was armed)");
                Ok(self.snapshot())
            }
            StopwatchState::Running => {
                // Archive current run and go to Disarmed
                if let Some(run) = self.current_run.take() {
                    self.history.push(run);
                }
                self.state = StopwatchState::Disarmed;
                info!("Stopwatch stopped");
                Ok(self.snapshot())
            }
            _ => Err(ErrorResponse::invalid_transition(self.state, "unarm")),
        }
    }

    /// Reset the stopwatch (!Disarmed -> Armed)
    pub fn reset(&mut self) -> Result<StateSnapshot, ErrorResponse> {
        match self.state {
            StopwatchState::Disarmed => {
                Err(ErrorResponse::invalid_transition(self.state, "reset"))
            }
            _ => {
                // Archive current run if exists and not already archived
                if let Some(run) = self.current_run.take() {
                    if !self.history.iter().any(|r| r.run_id == run.run_id) {
                        self.history.push(run);
                    }
                }
                self.state = StopwatchState::Armed;
                self.current_run = None;
                info!("Stopwatch reset to armed");
                Ok(self.snapshot())
            }
        }
    }

    /// Abort current lap and reset to Armed (lap not recorded)
    /// Used when lap hits max duration (MAX_LAP_DURATION_NS) - treated as timeout/abort
    /// Returns: snapshot or None if not running or no current lap
    fn timeout_reset(&mut self) -> Option<StateSnapshot> {
        if self.state != StopwatchState::Running {
            return None;
        }

        // Get the current lap ID - if there's no current lap, do nothing
        let lap_id = self.current_run.as_ref()
            .and_then(|r| r.current_lap())
            .map(|l| l.lap.clone())?;

        info!("Lap {} aborted at max duration (99.999s), resetting to armed", lap_id);

        // Archive run (current lap remains unfinished/aborted)
        if let Some(run) = self.current_run.take() {
            self.history.push(run);
        }
        self.state = StopwatchState::Armed;

        Some(self.snapshot())
    }

    /// Handle a trigger event
    /// Returns: (new_snapshot, lap_events) where lap_events contains:
    /// - First trigger: just lap 1 start
    /// - Subsequent triggers: lap N finish + lap N+1 start
    pub fn handle_trigger(
        &mut self,
        event: &TriggerEvent,
    ) -> Option<(StateSnapshot, Vec<LapEvent>)> {
        match self.state {
            StopwatchState::Disarmed => {
                debug!("Trigger ignored in {:?} state", self.state);
                None
            }
            StopwatchState::Armed => {
                // Start new run
                let run_id = self.next_run_id;
                self.next_run_id += 1;
                let run = Run::new(run_id, event.timestamp_ns);
                let lap_id = run.current_lap().unwrap().lap.clone(); // Get the lap ID from the newly created run
                self.current_run = Some(run);
                self.state = StopwatchState::Running;

                info!("Run {} started at trigger", run_id);

                // Emit lap start event with epoch-based lap ID
                let lap_start = LapEvent {
                    event_type: LapEventType::Start,
                    run_id,
                    lap: lap_id,
                    lap_time_ns: None,
                    total_time_ns: 0,
                    server_time_ns: event.timestamp_ns,
                };

                Some((self.snapshot(), vec![lap_start]))
            }
            StopwatchState::Running => {
                let run = self.current_run.as_mut()?;
                let run_id = run.run_id;

                // Update last trigger timestamp before calculating elapsed time
                run.last_trigger_ns = event.timestamp_ns;
                let (finished, new) = run.record_trigger(event.timestamp_ns);
                let total_time = run.total_elapsed_ns();

                let finished_lap_id = finished.lap.clone();
                let new_lap_id = new.lap.clone();

                info!(
                    "Lap {} finished ({}ns), lap {} started",
                    finished_lap_id,
                    finished.lap_time_ns().unwrap_or(0),
                    new_lap_id
                );

                let lap_finish = LapEvent {
                    event_type: LapEventType::Finish,
                    run_id,
                    lap: finished_lap_id,
                    lap_time_ns: finished.lap_time_ns().map(|ns| ns.min(MAX_LAP_DURATION_NS)),
                    total_time_ns: total_time,
                    server_time_ns: event.timestamp_ns,
                };

                let lap_start = LapEvent {
                    event_type: LapEventType::Start,
                    run_id,
                    lap: new_lap_id,
                    lap_time_ns: None,
                    total_time_ns: total_time,
                    server_time_ns: event.timestamp_ns,
                };

                Some((self.snapshot(), vec![lap_finish, lap_start]))
            }
        }
    }

    // History queries
    pub fn list_runs(&self, limit: u32, offset: u32) -> ListRunsResponse {
        let total = self.history.len() as u32;
        let runs: Vec<RunSummary> = self
            .history
            .iter()
            .skip(offset as usize)
            .take(limit as usize)
            .map(|run| {
                // Find the last finished lap (one with end_ns)
                let end_ns = run.laps.iter()
                    .rev()
                    .find_map(|lap| lap.end_ns);
                // Only calculate total_time from finished laps
                let total_time = end_ns.map(|end| end - run.start_ns).unwrap_or(0);
                RunSummary {
                    run_id: run.run_id,
                    start_time_ns: run.start_ns,
                    end_time_ns: end_ns,
                    total_time_ns: total_time,
                    lap_count: run.lap_count(),
                }
            })
            .collect();

        ListRunsResponse { runs, total }
    }

    pub fn get_run(&self, run_id: u64) -> Option<GetRunResponse> {
        self.history.iter().find(|r| r.run_id == run_id).map(|run| {
            // Find the last finished lap (one with end_ns)
            let end_ns = run.laps.iter()
                .rev()
                .find_map(|lap| lap.end_ns);
            // Only calculate total_time from finished laps
            let total_time = end_ns.map(|end| end - run.start_ns).unwrap_or(0);
            let mut cumulative = 0u64;

            GetRunResponse {
                run_id: run.run_id,
                start_time_ns: run.start_ns,
                end_time_ns: end_ns,
                total_time_ns: total_time,
                lap_count: run.lap_count(),
                laps: run
                    .laps
                    .iter()
                    .filter_map(|lap| {
                        lap.lap_time_ns().map(|lt| {
                            cumulative += lt;
                            LapDetail {
                                lap: lap.lap.clone(),
                                lap_time_ns: lt,
                                total_time_ns: cumulative,
                                timestamp_ns: lap.end_ns,
                            }
                        })
                    })
                    .collect(),
            }
        })
    }

    pub fn get_laps(&self, run_id: u64, limit: u32, offset: u32) -> Option<GetLapsResponse> {
        self.history.iter().find(|r| r.run_id == run_id).map(|run| {
            let mut cumulative = 0u64;
            let all_laps: Vec<LapDetail> = run
                .laps
                .iter()
                .filter_map(|lap| {
                    lap.lap_time_ns().map(|lt| {
                        cumulative += lt;
                        LapDetail {
                            lap: lap.lap.clone(),
                            lap_time_ns: lt,
                            total_time_ns: cumulative,
                            timestamp_ns: lap.end_ns,
                        }
                    })
                })
                .collect();

            let total = all_laps.len() as u32;
            let laps = all_laps
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect();

            GetLapsResponse { run_id, laps, total }
        })
    }
}

// ============================================================================
// Stopwatch Service (NATS)
// ============================================================================

pub type SharedStopwatch = Arc<RwLock<Stopwatch>>;

/// Configure JetStream stream for lap events
pub async fn configure_jetstream(nats: &Client) -> Result<Context> {
    let js = jetstream::new(nats.clone());
    
    // Create or get stream for lap events
    // Store messages for 1 hour (3600 seconds) as per plan: "server records X minutes of data"
    let stream_config = jetstream::stream::Config {
        name: "STOPWATCH_LAPS".to_string(),
        subjects: vec![subjects::LIVE_LAP.to_string()],
        max_age: std::time::Duration::from_secs(3600), // 1 hour retention
        storage: jetstream::stream::StorageType::File,
        ..Default::default()
    };
    
    match js.get_or_create_stream(stream_config).await {
        Ok(_) => {
            info!("JetStream stream 'STOPWATCH_LAPS' configured for subject '{}'", subjects::LIVE_LAP);
            Ok(js)
        }
        Err(e) => {
            warn!("Failed to configure JetStream stream: {}. Lap events will use regular pub/sub.", e);
            Ok(js)
        }
    }
}

/// Run the stopwatch NATS service
pub async fn run_stopwatch_service(nats: Client, stopwatch: SharedStopwatch) -> Result<()> {
    // Configure JetStream for lap events
    let js = configure_jetstream(&nats).await?;

    // Create NATS service
    let service = nats
        .service_builder()
        .description("Speed skating stopwatch timing service")
        .start("stopwatch", "1.0.0")
        .await
        .map_err(|e| eyre!("Failed to create NATS service: {}", e))?;

    info!("Stopwatch service started");

    // Spawn command handlers
    let cmd_handle = tokio::spawn(handle_commands(service, nats.clone(), stopwatch.clone(), js.clone()));

    // Spawn trigger subscriber
    let trigger_handle = tokio::spawn(handle_triggers(nats.clone(), stopwatch.clone(), js.clone()));

    // Spawn tick publisher
    let tick_handle = tokio::spawn(publish_ticks(nats.clone(), stopwatch.clone()));

    // Wait for all tasks
    tokio::select! {
        r = cmd_handle => { error!("Command handler exited: {:?}", r); }
        r = trigger_handle => { error!("Trigger handler exited: {:?}", r); }
        r = tick_handle => { error!("Tick publisher exited: {:?}", r); }
    }

    Ok(())
}

/// Handle NATS service commands
async fn handle_commands(
    service: async_nats::service::Service,
    nats: Client,
    stopwatch: SharedStopwatch,
    _js: Context,
) -> Result<()> {
    // Create service groups for different command categories
    let state_group = service.group("stopwatch.v1");

    // Add endpoints
    let mut arm_endpoint = state_group
        .endpoint("arm")
        .await
        .map_err(|e| eyre!("Failed to create arm endpoint: {}", e))?;
    let mut unarm_endpoint = state_group
        .endpoint("unarm")
        .await
        .map_err(|e| eyre!("Failed to create unarm endpoint: {}", e))?;
    let mut reset_endpoint = state_group
        .endpoint("reset")
        .await
        .map_err(|e| eyre!("Failed to create reset endpoint: {}", e))?;
    let mut get_state_endpoint = state_group
        .endpoint("get_state")
        .await
        .map_err(|e| eyre!("Failed to create get_state endpoint: {}", e))?;
    let mut list_runs_endpoint = state_group
        .endpoint("list_runs")
        .await
        .map_err(|e| eyre!("Failed to create list_runs endpoint: {}", e))?;
    let mut get_run_endpoint = state_group
        .endpoint("get_run")
        .await
        .map_err(|e| eyre!("Failed to create get_run endpoint: {}", e))?;
    let mut get_laps_endpoint = state_group
        .endpoint("get_laps")
        .await
        .map_err(|e| eyre!("Failed to create get_laps endpoint: {}", e))?;

    loop {
        tokio::select! {
            Some(request) = arm_endpoint.next() => {
                let mut sw = stopwatch.write().await;
                let response = match sw.arm() {
                    Ok(snapshot) => {
                        // Publish state change
                        let _ = publish_state(&nats, &snapshot).await;
                        serde_json::to_vec(&snapshot).unwrap()
                    }
                    Err(e) => serde_json::to_vec(&e).unwrap(),
                };
                let _ = request.respond(Ok(response.into())).await;
            }

            Some(request) = unarm_endpoint.next() => {
                let mut sw = stopwatch.write().await;
                let response = match sw.unarm() {
                    Ok(snapshot) => {
                        let _ = publish_state(&nats, &snapshot).await;
                        serde_json::to_vec(&snapshot).unwrap()
                    }
                    Err(e) => serde_json::to_vec(&e).unwrap(),
                };
                let _ = request.respond(Ok(response.into())).await;
            }

            Some(request) = reset_endpoint.next() => {
                let mut sw = stopwatch.write().await;
                let response = match sw.reset() {
                    Ok(snapshot) => {
                        let _ = publish_state(&nats, &snapshot).await;
                        serde_json::to_vec(&snapshot).unwrap()
                    }
                    Err(e) => serde_json::to_vec(&e).unwrap(),
                };
                let _ = request.respond(Ok(response.into())).await;
            }

            Some(request) = get_state_endpoint.next() => {
                let sw = stopwatch.read().await;
                let snapshot = sw.snapshot();
                let response = serde_json::to_vec(&snapshot).unwrap();
                let _ = request.respond(Ok(response.into())).await;
            }

            Some(request) = list_runs_endpoint.next() => {
                let payload = &request.message.payload;
                let req: ListRunsRequest = if payload.is_empty() {
                    ListRunsRequest::default()
                } else {
                    serde_json::from_slice(payload).unwrap_or_default()
                };
                let sw = stopwatch.read().await;
                let response = sw.list_runs(req.limit, req.offset);
                let _ = request.respond(Ok(serde_json::to_vec(&response).unwrap().into())).await;
            }

            Some(request) = get_run_endpoint.next() => {
                let payload = &request.message.payload;
                let req: Option<GetRunRequest> = if payload.is_empty() {
                    None
                } else {
                    serde_json::from_slice(payload).ok()
                };
                let sw = stopwatch.read().await;
                let response = match req {
                    Some(r) => sw.get_run(r.run_id)
                        .map(|r| serde_json::to_vec(&r).unwrap())
                        .unwrap_or_else(|| serde_json::to_vec(&ErrorResponse::new("Run not found", "NOT_FOUND")).unwrap()),
                    None => serde_json::to_vec(&ErrorResponse::new("Invalid request", "BAD_REQUEST")).unwrap(),
                };
                let _ = request.respond(Ok(response.into())).await;
            }

            Some(request) = get_laps_endpoint.next() => {
                let payload = &request.message.payload;
                let req: Option<GetLapsRequest> = if payload.is_empty() {
                    None
                } else {
                    serde_json::from_slice(payload).ok()
                };
                let sw = stopwatch.read().await;
                let response = match req {
                    Some(r) => sw.get_laps(r.run_id, r.limit, r.offset)
                        .map(|r| serde_json::to_vec(&r).unwrap())
                        .unwrap_or_else(|| serde_json::to_vec(&ErrorResponse::new("Run not found", "NOT_FOUND")).unwrap()),
                    None => serde_json::to_vec(&ErrorResponse::new("Invalid request", "BAD_REQUEST")).unwrap(),
                };
                let _ = request.respond(Ok(response.into())).await;
            }
        }
    }
}

/// Handle trigger events from NATS
/// Also auto-triggers when a lap exceeds max duration (99.999s)
async fn handle_triggers(nats: Client, stopwatch: SharedStopwatch, js: Context) -> Result<()> {
    let mut subscriber = nats.subscribe(TRIGGER_SUBJECT).await?;
    info!("Subscribed to trigger events on {}", TRIGGER_SUBJECT);

    loop {
        // Calculate time until auto-trigger (if running)
        let auto_trigger_deadline = {
            let sw = stopwatch.read().await;
            if sw.state == StopwatchState::Running {
                sw.current_run.as_ref().and_then(|run| {
                    run.current_lap().map(|lap| lap.start_ns + MAX_LAP_DURATION_NS)
                })
            } else {
                None
            }
        };

        // Calculate sleep duration for auto-trigger
        let auto_trigger_sleep = auto_trigger_deadline.map(|deadline| {
            let now = clock_monotonic_ns();
            if deadline > now {
                Duration::from_nanos(deadline - now)
            } else {
                Duration::ZERO // Already past deadline, trigger immediately
            }
        });

        tokio::select! {
            // Wait for real trigger from NATS
            msg = subscriber.next() => {
                match msg {
                    Some(msg) => {
                        let event: TriggerEvent = match serde_json::from_slice(&msg.payload) {
                            Ok(e) => e,
                            Err(e) => {
                                warn!("Invalid trigger event: {}", e);
                                continue;
                            }
                        };
                        debug!("Processing trigger event: {:?}", event);

                        let mut sw = stopwatch.write().await;
                        if let Some((snapshot, lap_events)) = sw.handle_trigger(&event) {
                            let _ = publish_state(&nats, &snapshot).await;
                            for lap_event in lap_events {
                                let _ = publish_lap(&js, &lap_event).await;
                            }
                        }
                    }
                    None => {
                        warn!("Trigger subscription ended");
                        break;
                    }
                }
            }
            // Wait for auto-trigger deadline (only if running with a deadline)
            _ = async {
                match auto_trigger_sleep {
                    Some(duration) => tokio::time::sleep(duration).await,
                    None => std::future::pending::<()>().await, // Never resolves if no deadline
                }
            } => {
                // Max lap duration reached - abort lap and reset to Armed
                info!("Max lap duration (99.999s) reached, aborting lap and resetting");

                let mut sw = stopwatch.write().await;
                if let Some(snapshot) = sw.timeout_reset() {
                    let _ = publish_state(&nats, &snapshot).await;
                }
            }
        }
    }

    Ok(())
}

/// Publish periodic tick updates while running
async fn publish_ticks(nats: Client, stopwatch: SharedStopwatch) -> Result<()> {
    let tick_interval = Duration::from_millis(100); // 10 Hz

    loop {
        tokio::time::sleep(tick_interval).await;

        let sw = stopwatch.read().await;
        if let Some(tick) = sw.tick_update() {
            let payload = serde_json::to_vec(&tick)?;
            if let Err(e) = nats.publish(subjects::LIVE_TICK, payload.into()).await {
                warn!("Failed to publish tick: {}", e);
            }
        }
    }
}

/// Publish state snapshot
async fn publish_state(nats: &Client, snapshot: &StateSnapshot) -> Result<()> {
    let payload = serde_json::to_vec(snapshot)?;
    nats.publish(subjects::LIVE_STATE, payload.into()).await?;
    debug!("Published state: {:?}", snapshot.state);
    Ok(())
}

/// Publish lap event to JetStream
async fn publish_lap(js: &Context, event: &LapEvent) -> Result<()> {
    let payload = serde_json::to_vec(event)?;
    
    // Publish to JetStream (messages are automatically available to subscribers)
    if let Err(e) = js.publish(subjects::LIVE_LAP, payload.into()).await {
        warn!("Failed to publish lap event to JetStream: {}", e);
    }
    
    debug!("Published lap event: {:?}", event);
    Ok(())
}

/// Run an event logger that subscribes to all live events and logs them
pub async fn run_event_logger(nats: Client) -> Result<()> {
    // Subscribe to state changes
    let mut state_sub = nats.subscribe(subjects::LIVE_STATE).await?;
    // Subscribe to lap events
    let mut lap_sub = nats.subscribe(subjects::LIVE_LAP).await?;

    info!(
        "Event logger started - listening on {} and {}",
        subjects::LIVE_STATE,
        subjects::LIVE_LAP
    );

    loop {
        tokio::select! {
            Some(msg) = state_sub.next() => {
                match serde_json::from_slice::<StateSnapshot>(&msg.payload) {
                    Ok(snapshot) => {
                        let run_id_str = snapshot.run_id.map(|id| id.to_string()).unwrap_or_else(|| "none".to_string());
                        info!(
                            "[STATE] {:?} | run_id={} elapsed={}ns laps={} running={}",
                            snapshot.state,
                            run_id_str,
                            snapshot.elapsed_ns,
                            snapshot.lap_count,
                            snapshot.running
                        );
                    }
                    Err(e) => {
                        warn!("Failed to parse state event: {}", e);
                    }
                }
            }
            Some(msg) = lap_sub.next() => {
                match serde_json::from_slice::<LapEvent>(&msg.payload) {
                    Ok(event) => {
                        match event.event_type {
                            LapEventType::Start => {
                                info!(
                                    "[LAP] START | run_id={} lap={} total={}ns",
                                    event.run_id,
                                    event.lap,
                                    event.total_time_ns
                                );
                            }
                            LapEventType::Finish => {
                                info!(
                                    "[LAP] FINISH | run_id={} lap={} lap_time={}ns total={}ns",
                                    event.run_id,
                                    event.lap,
                                    event.lap_time_ns.unwrap_or(0),
                                    event.total_time_ns
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse lap event: {}", e);
                    }
                }
            }
        }
    }
}

