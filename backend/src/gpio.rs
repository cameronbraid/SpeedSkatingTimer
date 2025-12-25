use core::panic;
use std::sync::Arc;

use crate::{App, SwitchState};
use chrono::Utc;
use color_eyre::Result;
use futures::StreamExt;
use gpiocdev::tokio::AsyncRequest;
use tokio::sync::mpsc;

pub async fn run(
    app: Arc<App>
) {
    let (minitor_sender, mut monitor_recv) = mpsc::unbounded_channel();

    tokio::spawn(gpio_sender(minitor_sender, 14)); // pin 40 = GPIO 21, pin 8 = GPIO 14

    loop {
        tokio::select! {
            _tick = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
              app.send_heartbeat().await;
            },
            event = monitor_recv.recv() => {
                match event {
                    Some(event) => {
                        app.update_switch_state(event).await;
                    }
                    None => {
                        break;
                    }
                }
            },
        }
    }
}

// async fn mock_gpio_sender(
//     sender: mpsc::UnboundedSender<Sample>,
//     setup_sender: mpsc::UnboundedSender<bool>,
//     duration: tokio::time::Duration,
// ) {
//     let mut connected = false;

//     loop {
//         connected = !connected;

//         setup_sender.send(connected).unwrap();

//         let _ = sender.send(Sample {
//             timestamp: Utc::now().timestamp_millis() as u64,
//             duration: Some(duration.as_millis() as u64),
//         });
//         tokio::time::sleep(duration).await;
//     }
// }


async fn gpio_sender(monitor_tx: mpsc::UnboundedSender<SwitchState>, line: u32) -> Result<()> {
    let req = gpiocdev::Request::builder()
        .on_chip("/dev/gpiochip0")
        .with_line(line)
        .as_input()
        .with_edge_detection(gpiocdev::line::EdgeDetection::BothEdges)
        .request()?;

    let initial_value = req.value(line)?;

    let areq = AsyncRequest::new(req);
    let mut evt_stream = areq.new_edge_event_stream(1000);

    monitor_tx.send(SwitchState {
        value: initial_value.into(),
        timestamp_ms: Utc::now().timestamp_millis() as u64,
    })?;

    while let Some(Ok(event)) = evt_stream.next().await {
        let now_ms = Utc::now().timestamp_millis() as u64;

        let value = match event.kind {
            gpiocdev::line::EdgeKind::Rising => true,
            gpiocdev::line::EdgeKind::Falling => false,
        };

        // send all events to the monitor
        let _ = monitor_tx.send(SwitchState {
            value,
            timestamp_ms: now_ms,
        });
    }

    Ok(())
}
