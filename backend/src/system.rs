//! System service implementation
//!
//! Provides the ping service for clients to measure network latency via RTT.

use async_nats::service::ServiceExt;
use async_nats::Client;
use color_eyre::Result;
use tracing::{debug, info, warn};
use futures::StreamExt;

use crate::trigger::clock_monotonic_ns;
use crate::types::{PingRequest, PingResponse};

/// Run the system ping service
pub async fn run_ping_service(nats: Client) -> Result<()> {
    // Create NATS service
    let service = nats
        .service_builder()
        .description("System ping service for latency measurement")
        .start("system", "1.0.0")
        .await
        .map_err(|e| color_eyre::eyre::eyre!("Failed to create NATS service: {}", e))?;

    info!("System ping service started");

    // Create service group
    let system_group = service.group("system.v1");

    // Create ping endpoint
    let mut ping_endpoint = system_group
        .endpoint("ping")
        .await
        .map_err(|e| color_eyre::eyre::eyre!("Failed to create ping endpoint: {}", e))?;

    // Handle ping requests
    while let Some(request) = ping_endpoint.next().await {
        // Parse request (can be empty or PingRequest)
        let _request: PingRequest = if request.message.payload.is_empty() {
            PingRequest::default()
        } else {
            match serde_json::from_slice(&request.message.payload) {
                Ok(req) => req,
                Err(e) => {
                    warn!("Failed to parse ping request: {}, using default", e);
                    PingRequest::default()
                }
            }
        };

        // Get current server time
        let server_time_ns = clock_monotonic_ns();
        let response = PingResponse {
            server_time_ns,
        };

        // Send response
        let payload = serde_json::to_vec(&response)?;
        if let Err(e) = request.respond(Ok(payload.into())).await {
            warn!("Failed to respond to ping: {}", e);
        } else {
            debug!("Ping response: {}ns", response.server_time_ns);
        }
    }

    Ok(())
}

