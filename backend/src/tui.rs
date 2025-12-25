//! TUI (Text User Interface) for stopwatch control using Ratatui

use async_nats::Client;
use color_eyre::Result;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event as CrosstermEvent, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;
use ratatui::Terminal;
use std::io;
use std::sync::{Arc, Mutex, RwLock};
use std::collections::VecDeque;
use tokio::time::{Duration, Instant};
use tracing::{Level, Subscriber};
use tracing::Event as TracingEvent;
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

use crate::trigger::{clock_monotonic_ns, TRIGGER_SUBJECT};
use crate::types::{EmptyRequest, ErrorResponse, GetRunRequest, ListRunsRequest, StateSnapshot, LapEvent, TickUpdate};
use crate::stopwatch::subjects;

/// Service name and group (must match service registration)
const SERVICE_NAME: &str = "stopwatch";
const SERVICE_GROUP: &str = "v1";

/// Global app state for TUI (set by init_tui_tracing)
static APP_STATE: Mutex<Option<Arc<RwLock<AppState>>>> = Mutex::new(None);

/// Log entry for display
#[derive(Clone, Debug)]
struct LogEntry {
    level: String,
    message: String,
}

/// App state for TUI
#[derive(Clone)]
struct AppState {
    snapshot: Option<StateSnapshot>,
    current_lap_time_ns: Option<u64>, // Current lap time from tick events
    current_laps: Vec<LapDisplay>,
    recent_runs: Vec<RunDisplay>,
    last_update: Instant,
    error_message: Option<String>,
    logs: VecDeque<LogEntry>,
    tick_watching: bool,
}

#[derive(Clone)]
struct LapDisplay {
    lap: String,
    time_ns: u64,
    total_ns: u64,
}

#[derive(Clone)]
struct RunDisplay {
    run_id: u64,
    lap_count: u32,
    total_time_ns: u64,
    laps: Vec<LapDisplay>,
}

/// Custom tracing layer that captures logs for TUI display
struct TuiLogLayer {
    app_state: Arc<RwLock<AppState>>,
}

impl<S: Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>> Layer<S> for TuiLogLayer {
    fn on_event(&self, event: &TracingEvent<'_>, _ctx: Context<'_, S>) {
        let mut message = String::new();
        
        let level = match *event.metadata().level() {
            Level::ERROR => "ERROR",
            Level::WARN => "WARN",
            Level::INFO => "INFO",
            Level::DEBUG => "DEBUG",
            Level::TRACE => "TRACE",
        };

        // Format the event
        event.record(&mut LogVisitor {
            message: &mut message,
        });

        // If message is still empty, try to get it from metadata
        let final_message = if message.is_empty() {
            // Fallback: use the event's name or a default message
            event.metadata().name().to_string()
        } else {
            message
        };

        let log_entry = LogEntry {
            level: level.to_string(),
            message: final_message,
        };

        // Add to logs (keep last 100 entries)
        // Use blocking write lock - this is safe because tracing callbacks are quick
        // and the TUI read locks are also quick
        match self.app_state.write() {
            Ok(mut state) => {
                state.logs.push_back(log_entry);
                if state.logs.len() > 100 {
                    state.logs.pop_front();
                }
            }
            Err(_) => {
                // Lock is poisoned - this shouldn't happen, but handle gracefully
            }
        }
    }
}

struct LogVisitor<'a> {
    message: &'a mut String,
}

impl<'a> tracing::field::Visit for LogVisitor<'a> {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        // The "message" field is the main log message
        if field.name() == "message" {
            *self.message = value.to_string();
        } else {
            if !self.message.is_empty() {
                self.message.push_str(" ");
            }
            self.message.push_str(&format!("{}={}", field.name(), value));
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if !self.message.is_empty() {
            self.message.push_str(" ");
        }
        self.message.push_str(&format!("{}={:?}", field.name(), value));
    }
}

/// Initialize TUI tracing subscriber early (before services start)
pub fn init_tui_tracing() -> Result<()> {
    // Initialize app state early
    let app_state = Arc::new(RwLock::new(AppState {
        snapshot: None,
        current_lap_time_ns: None,
        current_laps: Vec::new(),
        recent_runs: Vec::new(),
        last_update: Instant::now(),
        error_message: None,
        logs: VecDeque::new(),
        tick_watching: false,
    }));

    // Store in global static (thread-safe)
    let mut state = APP_STATE.lock().map_err(|e| color_eyre::eyre::eyre!("Mutex poisoned: {}", e))?;
    if state.is_some() {
        return Err(color_eyre::eyre::eyre!("App state already initialized"));
    }
    *state = Some(app_state.clone());

    // Install custom tracing layer that captures logs
    let layer = TuiLogLayer {
        app_state: app_state.clone(),
    };
    
    // Initialize tracing with our custom layer (disable default fmt)
    use tracing_subscriber::prelude::*;
    
    // Use try_init() which won't panic if a subscriber is already set
    // This should work since we call this before any other tracing initialization
    let _guard = tracing_subscriber::Registry::default()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .with(layer)
        .try_init()
        .map_err(|_| color_eyre::eyre::eyre!("Failed to initialize TUI tracing subscriber (subscriber may already be set)"))?;

    // Test log to verify the layer is working
    tracing::info!("TUI tracing initialized");

    Ok(())
}

/// Run the TUI
pub async fn run_tui(nats: Client) -> Result<()> {
    // Get app state from global static
    let app_state = {
        let state = APP_STATE.lock().map_err(|e| color_eyre::eyre::eyre!("Mutex poisoned: {}", e))?;
        state.as_ref().ok_or_else(|| color_eyre::eyre::eyre!("TUI tracing not initialized"))?.clone()
    };

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Fetch history on startup
    fetch_run_history(&nats, &app_state).await;

    // Spawn update tasks
    let state_updater = tokio::spawn(update_state(nats.clone(), app_state.clone()));
    let tick_subscriber = tokio::spawn(subscribe_ticks(nats.clone(), app_state.clone()));
    let lap_subscriber = tokio::spawn(subscribe_laps(nats.clone(), app_state.clone()));
    let state_event_subscriber = tokio::spawn(subscribe_state_events(nats.clone(), app_state.clone()));

    // Main event loop
    let mut should_quit = false;
    while !should_quit {
        // Clone state for rendering (std::sync::RwLock is sync)
        let state = app_state.read().map_err(|e| color_eyre::eyre::eyre!("RwLock poisoned: {}", e))?.clone();
        terminal.draw(|f| ui(f, &state))?;

        // Handle input or Ctrl-C
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                should_quit = true;
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                // Check for keyboard input
                if crossterm::event::poll(Duration::from_millis(0))? {
                    if let CrosstermEvent::Key(key) = event::read()? {
                        if key.kind == KeyEventKind::Press {
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Esc => {
                                    should_quit = true;
                                }
                                KeyCode::Char('t') | KeyCode::Char(' ') => {
                                    // Trigger
                                    let event = crate::types::TriggerEvent {
                                        timestamp_ns: clock_monotonic_ns(),
                                        source: "tui".to_string(),
                                    };
                                    let payload = serde_json::to_vec(&event)?;
                                    if let Err(e) = nats.publish(TRIGGER_SUBJECT, payload.into()).await {
                                        if let Ok(mut state) = app_state.write() {
                                            let error_msg = format!("Failed to send trigger: {}", e);
                                            state.error_message = Some(error_msg.clone());
                                            // Also add to logs
                                            state.logs.push_back(LogEntry {
                                                level: "ERROR".to_string(),
                                                message: error_msg,
                                            });
                                            if state.logs.len() > 100 {
                                                state.logs.pop_front();
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char('a') => {
                                    // Toggle arm/unarm based on current state
                                    let current_state = {
                                        if let Ok(state) = app_state.read() {
                                            state.snapshot.as_ref().map(|s| s.state)
                                        } else {
                                            None
                                        }
                                    };
                                    
                                    let command = match current_state {
                                        Some(crate::types::StopwatchState::Armed) | Some(crate::types::StopwatchState::Running) => "unarm",
                                        _ => "arm",
                                    };
                                    
                                    if let Err(e) = send_command::<EmptyRequest, StateSnapshot>(&nats, command, &EmptyRequest {}).await {
                                        if let Ok(mut state) = app_state.write() {
                                            let error_msg = format!("{} failed: {}", if command == "arm" { "Arm" } else { "Unarm" }, e);
                                            state.error_message = Some(error_msg.clone());
                                            // Also add to logs
                                            state.logs.push_back(LogEntry {
                                                level: "ERROR".to_string(),
                                                message: error_msg,
                                            });
                                            if state.logs.len() > 100 {
                                                state.logs.pop_front();
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char('r') => {
                                    // Reset
                                    if let Err(e) = send_command::<EmptyRequest, StateSnapshot>(&nats, "reset", &EmptyRequest {}).await {
                                        if let Ok(mut state) = app_state.write() {
                                            let error_msg = format!("Reset failed: {}", e);
                                            state.error_message = Some(error_msg.clone());
                                            // Also add to logs
                                            state.logs.push_back(LogEntry {
                                                level: "ERROR".to_string(),
                                                message: error_msg,
                                            });
                                            if state.logs.len() > 100 {
                                                state.logs.pop_front();
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char('c') => {
                                    // Clear error
                                    if let Ok(mut state) = app_state.write() {
                                        state.error_message = None;
                                    }
                                }
                                KeyCode::Char('x') => {
                                    // Toggle tick watching
                                    if let Ok(mut state) = app_state.write() {
                                        state.tick_watching = !state.tick_watching;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    // Cleanup
    state_updater.abort();
    tick_subscriber.abort();
    lap_subscriber.abort();
    state_event_subscriber.abort();

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

/// Update state periodically
async fn update_state(nats: Client, app_state: Arc<RwLock<AppState>>) {
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    loop {
        interval.tick().await;
        match send_command::<EmptyRequest, StateSnapshot>(&nats, "get_state", &EmptyRequest {}).await {
            Ok(snapshot) => {
                let run_id = snapshot.run_id;
                
                // Update snapshot first
                {
                    if let Ok(mut state) = app_state.write() {
                        state.snapshot = Some(snapshot.clone());
                        state.last_update = Instant::now();
                        if run_id.is_none() {
                            state.current_laps.clear();
                            state.current_lap_time_ns = None; // Clear lap time when run ends
                        }
                    }
                }
                
                // Fetch current run's laps if there's an active run (drop lock before await)
                if let Some(run_id) = run_id {
                    let get_run_req = GetRunRequest { run_id };
                    if let Ok(get_run_resp) = send_command::<GetRunRequest, crate::types::GetRunResponse>(&nats, "get_run", &get_run_req).await {
                        if let Ok(mut state) = app_state.write() {
                            state.current_laps = get_run_resp
                                .laps
                                .iter()
                                .map(|lap| LapDisplay {
                                    lap: lap.lap.clone(),
                                    time_ns: lap.lap_time_ns,
                                    total_ns: lap.total_time_ns,
                                })
                                .collect();
                        }
                    }
                }
            }
            Err(e) => {
                    if let Ok(mut state) = app_state.write() {
                        let error_msg = format!("State update failed: {}", e);
                        state.error_message = Some(error_msg.clone());
                        // Also add to logs
                        state.logs.push_back(LogEntry {
                            level: "ERROR".to_string(),
                            message: error_msg,
                        });
                        if state.logs.len() > 100 {
                            state.logs.pop_front();
                        }
                    }
            }
        }
    }
}

/// Subscribe to tick updates
async fn subscribe_ticks(nats: Client, app_state: Arc<RwLock<AppState>>) {
    use std::time::{Duration, Instant};
    
    let mut subscriber = match nats.subscribe(subjects::LIVE_TICK).await {
        Ok(sub) => sub,
        Err(e) => {
            if let Ok(mut state) = app_state.write() {
                let error_msg = format!("Failed to subscribe to ticks: {}", e);
                state.error_message = Some(error_msg.clone());
                // Also add to logs
                state.logs.push_back(LogEntry {
                    level: "ERROR".to_string(),
                    message: error_msg,
                });
                if state.logs.len() > 100 {
                    state.logs.pop_front();
                }
            }
            return;
        }
    };

    // Debounce: only log ticks every 500ms
    let debounce_interval = Duration::from_millis(500);
    let mut last_log = Instant::now();

    while let Some(msg) = subscriber.next().await {
        // Check if tick watching is enabled
        let should_log = {
            if let Ok(state) = app_state.read() {
                state.tick_watching
            } else {
                false
            }
        };

        match serde_json::from_slice::<TickUpdate>(&msg.payload) {
            Ok(tick) => {
                // Always update current lap time from tick events
                if let Ok(mut state) = app_state.write() {
                    state.current_lap_time_ns = Some(tick.elapsed_ns);
                }
                
                // Only log if tick watching is enabled and debounce interval has passed
                if should_log && last_log.elapsed() >= debounce_interval {
                    // Add log entry
                    if let Ok(mut state) = app_state.write() {
                        let elapsed_secs = tick.elapsed_ns as f64 / 1_000_000_000.0;
                        let log_entry = LogEntry {
                            level: "INFO".to_string(),
                            message: format!("[TICK] elapsed={:.3}s ({}ns)", elapsed_secs, tick.elapsed_ns),
                        };
                        state.logs.push_back(log_entry);
                        if state.logs.len() > 100 {
                            state.logs.pop_front();
                        }
                    }
                    last_log = Instant::now();
                }
            }
            Err(_) => {}
        }
    }
}

/// Subscribe to lap events for immediate lap updates
async fn subscribe_laps(nats: Client, app_state: Arc<RwLock<AppState>>) {
    let mut subscriber = match nats.subscribe(subjects::LIVE_LAP).await {
        Ok(sub) => sub,
        Err(e) => {
            if let Ok(mut state) = app_state.write() {
                let error_msg = format!("Failed to subscribe to laps: {}", e);
                state.error_message = Some(error_msg.clone());
                // Also add to logs
                state.logs.push_back(LogEntry {
                    level: "ERROR".to_string(),
                    message: error_msg,
                });
                if state.logs.len() > 100 {
                    state.logs.pop_front();
                }
            }
            return;
        }
    };

    while let Some(msg) = subscriber.next().await {
        match serde_json::from_slice::<LapEvent>(&msg.payload) {
            Ok(lap_event) => {
                // Only process finish events (they have lap_time_ns)
                if lap_event.event_type == crate::types::LapEventType::Finish {
                    if let Some(lap_time_ns) = lap_event.lap_time_ns {
                        // Update current laps immediately
                        if let Ok(mut state) = app_state.write() {
                            // Check if this is for the current run
                            if let Some(snapshot) = &state.snapshot {
                                if snapshot.run_id == Some(lap_event.run_id) {
                                    // Add or update this lap
                                    let lap_display = LapDisplay {
                                        lap: lap_event.lap.clone(),
                                        time_ns: lap_time_ns,
                                        total_ns: lap_event.total_time_ns,
                                    };
                                    
                                    // Find if this lap already exists and update it, or add it
                                    let existing_index = state.current_laps.iter().position(|l| l.lap == lap_event.lap);
                                    if let Some(idx) = existing_index {
                                        state.current_laps[idx] = lap_display;
                                    } else {
                                        state.current_laps.push(lap_display);
                                        // Sort by total time (since lap IDs are now strings)
                                        state.current_laps.sort_by_key(|l| l.total_ns);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(_) => {}
        }
    }
}

/// Fetch run history
async fn fetch_run_history(nats: &Client, app_state: &Arc<RwLock<AppState>>) {
    let req = ListRunsRequest { limit: 5, offset: 0 };
    match send_command::<ListRunsRequest, crate::types::ListRunsResponse>(nats, "list_runs", &req).await {
        Ok(response) => {
            let mut runs = Vec::new();
            for run_summary in &response.runs {
                // Fetch lap details for each run
                let get_run_req = GetRunRequest { run_id: run_summary.run_id };
                if let Ok(get_run_resp) = send_command::<GetRunRequest, crate::types::GetRunResponse>(nats, "get_run", &get_run_req).await {
                    let laps: Vec<LapDisplay> = get_run_resp
                        .laps
                        .iter()
                        .map(|lap| LapDisplay {
                            lap: lap.lap.clone(),
                            time_ns: lap.lap_time_ns,
                            total_ns: lap.total_time_ns,
                        })
                        .collect();
                    runs.push(RunDisplay {
                        run_id: run_summary.run_id,
                        lap_count: run_summary.lap_count,
                        total_time_ns: run_summary.total_time_ns,
                        laps,
                    });
                }
            }
            if let Ok(mut state) = app_state.write() {
                state.recent_runs = runs;
            }
        }
        Err(_) => {}
    }
}

/// Subscribe to state events and update history when runs finish
async fn subscribe_state_events(nats: Client, app_state: Arc<RwLock<AppState>>) {
    let mut subscriber = match nats.subscribe(subjects::LIVE_STATE).await {
        Ok(sub) => sub,
        Err(e) => {
            if let Ok(mut state) = app_state.write() {
                let error_msg = format!("Failed to subscribe to state events: {}", e);
                state.error_message = Some(error_msg.clone());
                // Also add to logs
                state.logs.push_back(LogEntry {
                    level: "ERROR".to_string(),
                    message: error_msg,
                });
                if state.logs.len() > 100 {
                    state.logs.pop_front();
                }
            }
            return;
        }
    };

    let mut previous_state: Option<crate::types::StopwatchState> = None;
    let mut previous_run_id: Option<u64> = None;

    while let Some(msg) = subscriber.next().await {
        match serde_json::from_slice::<StateSnapshot>(&msg.payload) {
            Ok(snapshot) => {
                let current_state = snapshot.state;
                let current_run_id = snapshot.run_id;

                // Update history when:
                // 1. Run finishes (Running -> Disarmed)
                // 2. Run is reset (any -> Armed with different run_id)
                let should_update = match (previous_state, current_state) {
                    (Some(crate::types::StopwatchState::Running), crate::types::StopwatchState::Disarmed) => {
                        // Run finished (stopped)
                        true
                    }
                    (Some(_), crate::types::StopwatchState::Armed) if previous_run_id.is_some() && current_run_id.is_none() => {
                        // Run was reset (had a run, now no run)
                        true
                    }
                    _ => false,
                };

                if should_update {
                    fetch_run_history(&nats, &app_state).await;
                }

                previous_state = Some(current_state);
                previous_run_id = current_run_id;
            }
            Err(_) => {}
        }
    }
}

/// Build service subject for an endpoint
fn service_subject(endpoint: &str) -> String {
    format!("{}.{}.{}", SERVICE_NAME, SERVICE_GROUP, endpoint)
}

/// Send a command to the stopwatch service
async fn send_command<T: serde::Serialize, R: for<'de> serde::Deserialize<'de>>(
    nats: &Client,
    endpoint: &str,
    request: &T,
) -> Result<R> {
    let subject = service_subject(endpoint);
    let payload = serde_json::to_vec(request)?;

    let response = nats
        .request(subject, payload.into())
        .await
        .map_err(|e| color_eyre::eyre::eyre!("Service call failed: {}", e))?;

    if let Ok(result) = serde_json::from_slice::<R>(&response.payload) {
        Ok(result)
    } else if let Ok(error) = serde_json::from_slice::<ErrorResponse>(&response.payload) {
        Err(color_eyre::eyre::eyre!("{}: {}", error.code, error.error))
    } else {
        let response_str = String::from_utf8_lossy(&response.payload);
        Err(color_eyre::eyre::eyre!("Invalid response format: {}", response_str))
    }
}

/// Render the UI
fn ui(f: &mut Frame, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // Header with help (increased from 3)
            Constraint::Min(0),     // Main content
            Constraint::Length(12), // Logs panel (increased from 8)
        ])
        .split(f.area());

    // Header with help/commands
    render_header(f, chunks[0], state);

    // Main content
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)]) // Recent runs wider
        .split(chunks[1]);

    // Left panel: Current state and laps
    render_current_state(f, main_chunks[0], state);

    // Right panel: Recent runs
    render_recent_runs(f, main_chunks[1], state);

    // Logs panel
    render_logs(f, chunks[2], state);
}

/// Render current state panel
fn render_current_state(f: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),  // Status info
            Constraint::Min(0),      // Current laps
        ])
        .split(area);

    // Status block
    let status_text = if let Some(snapshot) = &state.snapshot {
        let state_color = match snapshot.state {
            crate::types::StopwatchState::Disarmed => Color::Gray,
            crate::types::StopwatchState::Armed => Color::Yellow,
            crate::types::StopwatchState::Running => Color::Green,
        };

        // Use current lap time from tick events if available and running, otherwise use snapshot
        let elapsed_ns = if snapshot.running {
            state.current_lap_time_ns.unwrap_or(snapshot.elapsed_ns)
        } else {
            snapshot.elapsed_ns
        };
        let elapsed_secs = elapsed_ns as f64 / 1_000_000_000.0;
        let elapsed_str = format!("{:.3}s", elapsed_secs);

        vec![
            Line::from(vec![
                Span::styled("State: ", Style::default().fg(Color::White)),
                Span::styled(
                    format!("{:?}", snapshot.state),
                    Style::default().fg(state_color).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Run ID: ", Style::default().fg(Color::White)),
                Span::styled(
                    snapshot.run_id.map(|id| id.to_string()).unwrap_or_else(|| "None".to_string()),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(vec![
                Span::styled("Elapsed: ", Style::default().fg(Color::White)),
                Span::styled(elapsed_str, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("Laps: ", Style::default().fg(Color::White)),
                Span::styled(snapshot.lap_count.to_string(), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("Running: ", Style::default().fg(Color::White)),
                Span::styled(
                    snapshot.running.to_string(),
                    Style::default().fg(if snapshot.running { Color::Green } else { Color::Red }),
                ),
            ]),
        ]
    } else {
        vec![Line::from(Span::styled("Loading...", Style::default().fg(Color::Yellow)))]
    };

    let status_block = Block::default()
        .title("Current State")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let status_paragraph = Paragraph::new(status_text)
        .block(status_block)
        .wrap(Wrap { trim: true });
    f.render_widget(status_paragraph, chunks[0]);

    // Current laps
    let lap_items: Vec<ListItem> = state
        .current_laps
        .iter()
        .map(|lap| {
            let lap_secs = lap.time_ns as f64 / 1_000_000_000.0;
            let total_secs = lap.total_ns as f64 / 1_000_000_000.0;
            ListItem::new(format!(
                "Lap {}: {:.3}s (total: {:.3}s)",
                lap.lap, lap_secs, total_secs
            ))
        })
        .collect();

    let laps_list = List::new(lap_items)
        .block(Block::default().title("Current Laps").borders(Borders::ALL))
        .style(Style::default().fg(Color::White));
    f.render_widget(laps_list, chunks[1]);
}

/// Render recent runs panel
fn render_recent_runs(f: &mut Frame, area: Rect, state: &AppState) {
    let run_items: Vec<ListItem> = state
        .recent_runs
        .iter()
        .rev() // Show newest first
        .map(|run| {
            let total_secs = run.total_time_ns as f64 / 1_000_000_000.0;
            let mut text = vec![Span::styled(
                format!("Run {}: {} laps, {:.3}s", run.run_id, run.lap_count, total_secs),
                Style::default().fg(Color::Cyan),
            )];
            
            // Show lap times as comma-separated list
            if !run.laps.is_empty() {
                let lap_times: Vec<String> = run.laps
                    .iter()
                    .map(|lap| {
                        let lap_secs = lap.time_ns as f64 / 1_000_000_000.0;
                        format!("{:.3}s", lap_secs)
                    })
                    .collect();
                text.push(Span::raw("\n  "));
                text.push(Span::styled(
                    lap_times.join(", "),
                    Style::default().fg(Color::White),
                ));
            }
            ListItem::new(Line::from(text))
        })
        .collect();

    let runs_list = List::new(run_items)
        .block(Block::default().title("Recent Runs").borders(Borders::ALL))
        .style(Style::default().fg(Color::White));
    f.render_widget(runs_list, area);
}

/// Render logs panel
fn render_logs(f: &mut Frame, area: Rect, state: &AppState) {
    let log_items: Vec<ListItem> = state
        .logs
        .iter()
        .rev() // Show newest first
        .take(10) // Show last 10 log entries (increased from 6)
        .map(|log| {
            let level_color = match log.level.as_str() {
                "ERROR" => Color::Red,
                "WARN" => Color::Yellow,
                "INFO" => Color::Cyan,
                "DEBUG" => Color::Gray,
                "TRACE" => Color::DarkGray,
                _ => Color::White,
            };

            // Truncate long messages
            let message = if log.message.len() > 60 {
                format!("{}...", &log.message[..57])
            } else {
                log.message.clone()
            };

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("[{}] ", log.level),
                    Style::default().fg(level_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(message, Style::default().fg(Color::White)),
            ]))
        })
        .collect();

    let logs_list = List::new(log_items)
        .block(Block::default().title("Logs").borders(Borders::ALL))
        .style(Style::default().fg(Color::White));
    f.render_widget(logs_list, area);
}

/// Render header with title and help/commands
fn render_header(f: &mut Frame, area: Rect, state: &AppState) {
    let mut help_parts = vec!["[T]rigger", "[A]rm/Unarm", "[R]eset"];
    
    if state.tick_watching {
        help_parts.push("[X]tick:ON");
    } else {
        help_parts.push("[X]tick:OFF");
    }
    
    help_parts.push("[Q]uit");
    
    let help_text = help_parts.join(" ");
    
    let header_lines = vec![
        Line::from(Span::styled(
            "Speed Skating Timer",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            help_text,
            Style::default().fg(Color::Gray),
        )),
    ];

    let header = Paragraph::new(header_lines)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    f.render_widget(header, area);
}

