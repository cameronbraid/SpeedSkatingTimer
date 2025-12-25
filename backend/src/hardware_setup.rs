//! Hardware Setup Service
//!
//! Handles hardware setup mode for IR sensor alignment.
//! Runs independently of the trigger source.

use std::sync::Arc;

use async_nats::service::ServiceExt;
use async_nats::Client;
use color_eyre::eyre::eyre;
use color_eyre::Result;
use futures::StreamExt;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::types::{HardwareSetupState, PinState};

/// NATS subjects for hardware setup
pub mod subjects {
    pub const ARM: &str = "hardware-setup.v1.arm";
    pub const DISARM: &str = "hardware-setup.v1.disarm";
    pub const LIVE_STATE: &str = "hardware-setup.v1.live.state";
}

/// Shared hardware setup state that can be updated by the GPIO trigger
#[derive(Debug, Default)]
pub struct HardwareSetupService {
    pub armed: bool,
    pub pins: Vec<PinState>,
    pub gpio_available: bool,
}

pub type SharedHardwareSetup = Arc<RwLock<HardwareSetupService>>;

/// Create a new shared hardware setup service
pub fn new_hardware_setup_shared() -> SharedHardwareSetup {
    Arc::new(RwLock::new(HardwareSetupService::default()))
}

/// Run the hardware setup NATS service
pub async fn run_hardware_setup_service(
    nats: Client,
    setup: SharedHardwareSetup,
) -> Result<()> {
    // Create NATS service
    let service = nats
        .service_builder()
        .description("Hardware setup service for IR sensor alignment")
        .start("hardware-setup", "1.0.0")
        .await
        .map_err(|e| eyre!("Failed to create hardware setup service: {}", e))?;

    info!("Hardware setup service started");

    // Create service group
    let setup_group = service.group("hardware-setup.v1");

    // Create endpoints
    let mut arm_endpoint = setup_group
        .endpoint("arm")
        .await
        .map_err(|e| eyre!("Failed to create arm endpoint: {}", e))?;

    let mut disarm_endpoint = setup_group
        .endpoint("disarm")
        .await
        .map_err(|e| eyre!("Failed to create disarm endpoint: {}", e))?;

    info!("Hardware setup service listening on 'hardware-setup.v1.arm' and 'hardware-setup.v1.disarm'");

    loop {
        tokio::select! {
            Some(request) = arm_endpoint.next() => {
                let mut state = setup.write().await;
                
                if !state.gpio_available {
                    // No GPIO configured - return error state
                    let response = HardwareSetupState {
                        armed: false,
                        pins: vec![],
                    };
                    let payload = serde_json::to_vec(&response).unwrap_or_default();
                    let _ = request.respond(Ok(payload.into())).await;
                    continue;
                }
                
                state.armed = true;
                info!("Hardware setup mode ARMED");
                
                let response = HardwareSetupState {
                    armed: true,
                    pins: state.pins.clone(),
                };
                let payload = serde_json::to_vec(&response).unwrap_or_default();
                let _ = request.respond(Ok(payload.into())).await;
            }

            Some(request) = disarm_endpoint.next() => {
                let mut state = setup.write().await;
                state.armed = false;
                info!("Hardware setup mode DISARMED");
                
                let response = HardwareSetupState {
                    armed: false,
                    pins: state.pins.clone(),
                };
                let payload = serde_json::to_vec(&response).unwrap_or_default();
                let _ = request.respond(Ok(payload.into())).await;
            }
        }
    }
}

/// Background task to publish setup state at ~10Hz when armed
pub async fn run_hardware_setup_publisher(
    nats: Client,
    setup: SharedHardwareSetup,
) -> Result<()> {
    let interval = std::time::Duration::from_millis(100); // 10Hz

    loop {
        tokio::time::sleep(interval).await;

        let state = setup.read().await;
        if !state.armed || !state.gpio_available {
            continue;
        }

        let setup_state = HardwareSetupState {
            armed: true,
            pins: state.pins.clone(),
        };

        let payload = match serde_json::to_vec(&setup_state) {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to serialize setup state: {}", e);
                continue;
            }
        };

        if let Err(e) = nats.publish(subjects::LIVE_STATE, payload.into()).await {
            error!("Failed to publish setup state: {}", e);
        }
    }
}
