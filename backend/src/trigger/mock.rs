//! Mock trigger source for testing

use std::time::Duration;

use color_eyre::Result;
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::trigger::clock_monotonic_ns;
use crate::types::TriggerEvent;

/// Mock trigger source for testing
pub async fn run(
    interval: Duration,
    tx: mpsc::UnboundedSender<TriggerEvent>,
) -> Result<()> {
    info!("Mock trigger active with {}ms interval", interval.as_millis());

    loop {
        tokio::time::sleep(interval).await;

        let event = TriggerEvent {
            timestamp_ns: clock_monotonic_ns(),
            source: "mock".to_string(),
        };
        debug!("Mock trigger");
        if tx.send(event).is_err() {
            break;
        }
    }

    Ok(())
}


