//! GPIO trigger module - monitors GPIO pins for edge events and emits trigger events.
//! Also updates shared hardware setup state for frontend display (e.g., IR sensor alignment).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use color_eyre::Result;
use color_eyre::eyre::eyre;
use futures::{StreamExt, stream::select_all};
use gpio_cdev::{AsyncLineEventHandle, Chip, EventRequestFlags, EventType, LineRequestFlags};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::hardware_setup::SharedHardwareSetup;
use crate::types::{GpioConfig, PinState, TriggerEvent};

/// Pin info with number, name, and active_high configuration
#[derive(Debug, Clone)]
struct PinInfo {
    pin: u32,
    name: String,
    active_high: bool,
}

/// Monitors multiple GPIO pins and emits trigger events.
///
/// On every edge event:
/// - Updates hardware setup state (pin active/inactive) for frontend display
///
/// Debounce strategy for triggers:
/// - Emit trigger immediately on the first edge
/// - Then filter noise: ignore subsequent edges until quiet period with no edges, or max_wait expires
/// - Pin state updates still happen during filtering (for accurate frontend display)
pub async fn run(
    config: GpioConfig,
    tx: mpsc::UnboundedSender<TriggerEvent>,
    hardware_setup: SharedHardwareSetup,
) -> Result<()> {
    let chip_path = &config.chip;
    let quiet_period = Duration::from_millis(config.debounce.quiet_period_ms);
    let max_wait = Duration::from_millis(config.debounce.max_wait_ms);

    // Build pin info with names and active_high configuration
    let pin_infos: Vec<PinInfo> = config
        .pins
        .iter()
        .map(|p| PinInfo {
            pin: p.pin,
            name: p.name.clone(),
            active_high: p.active_high,
        })
        .collect();
    let pins: Vec<u32> = pin_infos.iter().map(|p| p.pin).collect();
    
    // Create a lookup map for pin info by pin number
    let pin_info_map: HashMap<u32, PinInfo> = pin_infos
        .iter()
        .map(|p| (p.pin, p.clone()))
        .collect();

    info!(
        "GPIO trigger active on {} pins {:?} (quiet: {}ms, max: {}ms)",
        chip_path,
        pins,
        quiet_period.as_millis(),
        max_wait.as_millis()
    );

    if pins.is_empty() {
        return Err(eyre!("No GPIO pins specified for trigger"));
    }

    info!("Opening GPIO chip at {}...", chip_path);
    let mut chip = match Chip::new(&chip_path) {
        Ok(c) => {
            info!("GPIO chip opened successfully");
            c
        }
        Err(e) => {
            error!("Failed to open GPIO chip: {:?}", e);
            return Err(eyre!("Failed to open GPIO chip: {}", e));
        }
    };

    // Initialize the shared hardware setup state with pin names
    info!("Acquiring hardware_setup write lock...");
    {
        let mut setup = hardware_setup.write().await;
        info!("Got hardware_setup write lock");
        setup.gpio_available = true;
        setup.pins = pin_infos
            .iter()
            .map(|p| PinState {
                pin: p.pin,
                name: p.name.clone(),
                active: false,
            })
            .collect();
        info!("Hardware setup: GPIO available with {} pins", pins.len());
    }

    // Create async event handles for all pins (for trigger detection)
    info!("Setting up event handlers for {} pins...", pins.len());
    let mut event_streams = Vec::new();
    for &pin in &pins {
        info!("  Requesting events for pin {}...", pin);
        let line = chip.get_line(pin)?;
        let line_events = line.events(
            LineRequestFlags::INPUT,
            EventRequestFlags::BOTH_EDGES,
            "speedskating-trigger",
        )?;
        info!("  Creating async handle for pin {}...", pin);
        let events = AsyncLineEventHandle::new(line_events)?;
        event_streams.push((pin, events));
        info!("  Pin {} ready", pin);
    }

    info!(
        "Created {} event streams for GPIO pins",
        event_streams.len()
    );

    // Combine all event streams into one, tracking which pin each event came from
    let pin_event_streams: Vec<_> = event_streams
        .into_iter()
        .map(|(pin, stream)| stream.map(move |evt| (pin, evt)))
        .collect();
    let mut combined_events = select_all(pin_event_streams);

    info!("GPIO trigger ready, waiting for events...");

    // State for noise filtering (shared across all pins)
    let mut filter_start: Option<Instant> = None; // When noise filter period started
    let mut filtered_count: u32 = 0;

    loop {
        if let Some(start_time) = filter_start {
            // In noise filtering mode - ignore edges until quiet period or max_wait
            let elapsed = start_time.elapsed();
            let remaining_max = max_wait.saturating_sub(elapsed);

            if remaining_max.is_zero() {
                // Max wait exceeded - filtering period over
                debug!(
                    "GPIO noise filter ended (max-wait, {} edges filtered)",
                    filtered_count
                );
                filter_start = None;
                filtered_count = 0;
                continue;
            }

            // Wait for: quiet period, max wait, or another edge
            let wait_time = quiet_period.min(remaining_max);

            tokio::select! {
                result = combined_events.next() => {
                    match result {
                        Some((pin, Ok(evt))) => {
                            // Update pin state from edge event
                            if let Some(pin_info) = pin_info_map.get(&pin) {
                                update_pin_state(&hardware_setup, pin, &evt, pin_info.active_high).await;
                            }
                            // Edge during filter period - ignore it (noise)
                            filtered_count += 1;
                            debug!("GPIO filtered edge #{} on pin {} (noise)", filtered_count, pin);
                            continue;
                        }
                        Some((pin, Err(e))) => {
                            warn!("GPIO event error on pin {}: {}", pin, e);
                            continue;
                        }
                        None => {
                            warn!("All GPIO event streams ended unexpectedly (during filter)");
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(wait_time) => {
                    let reason = if wait_time == remaining_max { "max-wait" } else { "quiet" };
                    debug!("GPIO noise filter ended ({}, {} edges filtered)", reason, filtered_count);
                    filter_start = None;
                    filtered_count = 0;
                }
            }
        } else {
            // Ready for trigger - wait for edge
            match combined_events.next().await {
                Some((pin, Ok(evt))) => {
                    // Emit trigger immediately
                    let timestamp = evt.timestamp();
                    debug!("GPIO trigger emitted on pin {} at {}ns", pin, timestamp);
                    emit_trigger(&tx, &pins, timestamp)?;

                    // Update pin state from edge event
                    if let Some(pin_info) = pin_info_map.get(&pin) {
                        update_pin_state(&hardware_setup, pin, &evt, pin_info.active_high).await;
                    }

                    // Start noise filtering period
                    filter_start = Some(Instant::now());
                    filtered_count = 0;
                }
                Some((pin, Err(e))) => {
                    warn!("GPIO event error on pin {}: {}", pin, e);
                }
                None => {
                    warn!("All GPIO event streams ended unexpectedly");
                    break;
                }
            }
        }
    }

    // Mark GPIO as unavailable when shutting down
    {
        let mut setup = hardware_setup.write().await;
        setup.gpio_available = false;
    }

    Ok(())
}

/// Updates the pin's active state in hardware setup based on edge type and active_high configuration.
/// If active_high is true: rising edge = active, falling edge = inactive
/// If active_high is false: rising edge = inactive, falling edge = active
async fn update_pin_state(
    hardware_setup: &SharedHardwareSetup,
    pin: u32,
    evt: &gpio_cdev::LineEvent,
    active_high: bool,
) {
    let new_state = if active_high {
        // Active high: rising edge means active, falling edge means inactive
        matches!(evt.event_type(), EventType::RisingEdge)
    } else {
        // Active low: falling edge means active, rising edge means inactive
        matches!(evt.event_type(), EventType::FallingEdge)
    };

    let mut setup = hardware_setup.write().await;
    if let Some(pin_state) = setup.pins.iter_mut().find(|p| p.pin == pin) {
        pin_state.active = new_state;
    }

    debug!("Pin {} state updated to {} (active_high: {})", pin, new_state, active_high);
}

/// Sends a trigger event through the channel to the stopwatch.
fn emit_trigger(
    tx: &mpsc::UnboundedSender<TriggerEvent>,
    pins: &[u32],
    timestamp_ns: u64,
) -> Result<()> {
    let event = TriggerEvent {
        timestamp_ns,
        source: "gpio".to_string(),
    };
    info!("GPIO trigger on pins {:?} at {}ns", pins, timestamp_ns);
    tx.send(event)
        .map_err(|_| color_eyre::eyre::eyre!("Trigger channel closed"))
}
