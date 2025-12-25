//! Trigger sources for the stopwatch service
//!
//! Triggers are abstracted as logical pulse events. The stopwatch service
//! does not know about the physical source of triggers.

mod gpio;
mod keyboard;
mod mock;

use std::time::Duration;

use async_nats::Client;
use color_eyre::Result;
use tokio::sync::mpsc;
use tracing::{debug, error};

use crate::hardware_setup::SharedHardwareSetup;
use crate::types::{GpioConfig, TriggerEvent};

/// Subject for publishing trigger events
pub const TRIGGER_SUBJECT: &str = "stopwatch.v1.trigger";

/// Get current CLOCK_MONOTONIC time in nanoseconds (matches GPIO event timestamps)
pub fn clock_monotonic_ns() -> u64 {
    use nix::time::{clock_gettime, ClockId};
    let ts = clock_gettime(ClockId::CLOCK_MONOTONIC).expect("Failed to get CLOCK_MONOTONIC time");
    (ts.tv_sec() as u64) * 1_000_000_000 + (ts.tv_nsec() as u64)
}

/// Trigger source configuration
#[derive(Debug, Clone)]
pub enum TriggerConfig {
    /// Keyboard trigger with specified key
    Keyboard { key: char },
    /// GPIO trigger configured from gpio.yaml
    Gpio { config: GpioConfig },
    /// Mock trigger for testing (triggers at interval)
    Mock { interval_ms: u64 },
}

/// Run a trigger source that publishes to NATS
pub async fn run_trigger_source(
    config: TriggerConfig,
    nats: Client,
    hardware_setup: SharedHardwareSetup,
) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<TriggerEvent>();

    // Spawn the appropriate trigger source
    let mut source_handle = match config {
        TriggerConfig::Keyboard { key } => {
            tokio::spawn(keyboard::run(key, tx))
        }
        TriggerConfig::Gpio { config: gpio_config } => {
            tokio::spawn(gpio::run(gpio_config, tx, hardware_setup))
        }
        TriggerConfig::Mock { interval_ms } => {
            tokio::spawn(mock::run(Duration::from_millis(interval_ms), tx))
        }
    };

    // Forward trigger events to NATS, also watch for source task completion
    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Some(event) => {
                        let payload = serde_json::to_vec(&event)?;
                        if let Err(e) = nats.publish(TRIGGER_SUBJECT, payload.into()).await {
                            error!("Failed to publish trigger event: {}", e);
                        } else {
                            debug!("Published trigger event: {:?}", event);
                        }
                    }
                    None => {
                        // Channel closed, check if source task had an error
                        break;
                    }
                }
            }
            result = &mut source_handle => {
                // Source task completed
                match result {
                    Ok(Ok(())) => {
                        debug!("Trigger source completed successfully");
                        return Ok(());
                    }
                    Ok(Err(e)) => {
                        error!("Trigger source failed: {}", e);
                        return Err(e);
                    }
                    Err(e) => {
                        error!("Trigger source task panicked: {}", e);
                        return Err(color_eyre::eyre::eyre!("Trigger source task panicked: {}", e));
                    }
                }
            }
        }
    }

    // If we get here, channel closed - wait for task to see why
    match source_handle.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(color_eyre::eyre::eyre!("Trigger source task panicked: {}", e)),
    }
}

