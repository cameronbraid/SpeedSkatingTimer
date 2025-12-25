//! Simulator service for the stopwatch
//!
//! Runs a static sequence of commands and triggers, repeating indefinitely.
//! Can send commands (arm, unarm, reset) and trigger events.

use std::time::Duration;

use async_nats::Client;
use color_eyre::eyre::eyre;
use color_eyre::Result;
use tracing::{debug, error, info, warn};

use crate::types::{EmptyRequest, StateSnapshot, TriggerEvent};
use crate::trigger::{clock_monotonic_ns, TRIGGER_SUBJECT};

/// Send a command to the stopwatch service using NATS service API
async fn send_command<T: serde::Serialize, R: for<'de> serde::Deserialize<'de>>(
    nats: &Client,
    endpoint: &str,
    request: &T,
) -> Result<R> {
    let subject = format!("stopwatch.v1.{}", endpoint);
    let payload = serde_json::to_vec(request)?;
    
    // Use NATS request/reply pattern for service calls
    let response = nats
        .request(subject, payload.into())
        .await
        .map_err(|e| color_eyre::eyre::eyre!("Service call failed: {}", e))?;
    
    // Try to parse as the expected response type
    if let Ok(result) = serde_json::from_slice::<R>(&response.payload) {
        Ok(result)
    } else if let Ok(error) = serde_json::from_slice::<crate::types::ErrorResponse>(&response.payload) {
        Err(color_eyre::eyre::eyre!("{}: {}", error.code, error.error))
    } else {
        // Try to get error message from response
        let response_str = String::from_utf8_lossy(&response.payload);
        Err(color_eyre::eyre::eyre!("Invalid response format: {}", response_str))
    }
}

/// Send a command with retry logic - retries indefinitely until success
async fn send_command_with_retry(
    nats: &Client,
    command_name: &str,
) -> Result<StateSnapshot> {
    const RETRY_DELAY: Duration = Duration::from_millis(500);
    let mut attempt = 0;
    
    loop {
        attempt += 1;
        match send_command::<EmptyRequest, StateSnapshot>(nats, command_name, &EmptyRequest {}).await {
            Ok(snapshot) => {
                if attempt > 1 {
                    info!("Simulator: {} command succeeded on attempt {}", command_name, attempt);
                }
                return Ok(snapshot);
            }
            Err(e) => {
                warn!(
                    "Simulator: {} command failed on attempt {}: {}. Retrying in {:?}...",
                    command_name, attempt, e, RETRY_DELAY
                );
                tokio::time::sleep(RETRY_DELAY).await;
            }
        }
    }
}

/// Run the simulator service with a static sequence
///
/// Sequence format: space-separated tokens like "arm 1.4s trigger 8.4s trigger 10.2s trigger reset 5s"
/// Tokens:
/// - "arm" - send arm command
/// - "unarm" - send unarm command
/// - "reset" - send reset command
/// - "<duration>s" - wait for duration in seconds (supports decimals like 8.4s)
/// - "trigger" - send a trigger event
pub async fn run_simulator(sequence: String, nats: Client) -> Result<()> {
    info!("Simulator service starting with sequence: {}", sequence);

    let tokens: Vec<&str> = sequence.split_whitespace().collect();
    
    if tokens.is_empty() {
        return Err(eyre!("Simulator sequence is empty"));
    }

    loop {
        for token in &tokens {
            if token.ends_with('s') && token.len() > 1 {
                // Parse duration: "1.4s" or "8.4s"
                let duration_str = &token[..token.len() - 1]; // Remove "s" suffix
                let duration_secs: f64 = duration_str
                    .parse()
                    .map_err(|e| eyre!("Invalid duration '{}': {}", duration_str, e))?;
                
                let duration = Duration::from_secs_f64(duration_secs);
                debug!("Simulator: waiting for {:.3}s", duration_secs);
                tokio::time::sleep(duration).await;
            } else if *token == "trigger" {
                let event = TriggerEvent {
                    timestamp_ns: clock_monotonic_ns(),
                    source: "simulator".to_string(),
                };
                info!("Simulator: sending trigger");
                let payload = serde_json::to_vec(&event)?;
                if let Err(e) = nats.publish(TRIGGER_SUBJECT, payload.into()).await {
                    error!("Failed to publish trigger event: {}", e);
                }
            } else if *token == "arm" {
                info!("Simulator: sending arm command");
                match send_command_with_retry(&nats, "arm").await {
                    Ok(snapshot) => {
                        debug!("Simulator: armed successfully, state: {:?}", snapshot.state);
                    }
                    Err(e) => {
                        // This should never happen since retry loops indefinitely
                        error!("Simulator: arm command failed unexpectedly: {}", e);
                    }
                }
            } else if *token == "unarm" {
                info!("Simulator: sending unarm command");
                match send_command_with_retry(&nats, "unarm").await {
                    Ok(snapshot) => {
                        debug!("Simulator: unarmed successfully, state: {:?}", snapshot.state);
                    }
                    Err(e) => {
                        // This should never happen since retry loops indefinitely
                        error!("Simulator: unarm command failed unexpectedly: {}", e);
                    }
                }
            } else if *token == "reset" {
                info!("Simulator: sending reset command");
                match send_command_with_retry(&nats, "reset").await {
                    Ok(snapshot) => {
                        debug!("Simulator: reset successfully, state: {:?}", snapshot.state);
                    }
                    Err(e) => {
                        // This should never happen since retry loops indefinitely
                        error!("Simulator: reset command failed unexpectedly: {}", e);
                    }
                }
            } else {
                error!("Simulator: unknown token '{}', ignoring", token);
            }
        }
    }
}

