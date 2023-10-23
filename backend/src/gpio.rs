use std::time::Instant;

use chrono::Utc;
use futures::stream::StreamExt;
use gpio_cdev::{ AsyncLineEventHandle, Chip, EventRequestFlags, LineRequestFlags };
use tokio::sync::mpsc;
use color_eyre::Result;
use crate::{ Clients, Sample };

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
enum Message {
    #[serde(rename = "timestamp")] Timestamp(TimestampMessage),
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TimestampMessage {
    pub timestamp: u64,
}

pub async fn run(clients: Clients) {
    let (input_sender, mut input_recv) = mpsc::unbounded_channel();

    // tokio::spawn(mock_gpio_sender(input_sender, tokio::time::Duration::from_secs(1)));
    tokio::spawn(gpio_sender(input_sender, 14)); // pin 40 = GPIO 21, pin 8 = GPIO 14

    loop {
        let sample = input_recv.recv().await;
        match sample {
            Some(timestamp) => {
                let mut clients = clients.write().await;
                for (_, client) in clients.iter_mut() {
                    let response = serde_json
                        ::to_string(
                            &Message::Timestamp(TimestampMessage {
                                timestamp,
                            })
                        )
                        .expect("unable to serialise");
                    let _ = client.sender.send(Ok(warp::ws::Message::text(response)));
                }
            }
            None => {
                break;
            }
        }
    }
}

async fn mock_gpio_sender(sender: mpsc::UnboundedSender<Sample>, duration: tokio::time::Duration) {
    loop {
        let _ = sender.send(Utc::now().timestamp_millis() as u64);
        tokio::time::sleep(duration).await;
    }
}

async fn gpio_sender(sender: mpsc::UnboundedSender<Sample>, line: u32) -> Result<()> {
    let mut chip = Chip::new("/dev/gpiochip0")?;

    let input = chip.get_line(line)?;

    let mut events = AsyncLineEventHandle::new(
        input.events(LineRequestFlags::INPUT, EventRequestFlags::BOTH_EDGES, "read-gpio")?
    )?;

    let debounce = std::time::Duration::from_secs(3);
    let mut last_sent: Option<Instant> = None;
    loop {
        match events.next().await {
            Some(Ok(evt)) => {
                if let Some(last) = last_sent {
                    if last.elapsed() < debounce {
                        continue;
                    }
                }
                last_sent = Some(Instant::now());
                // sender.send(evt.timestamp() / 1000000)?; // round half up ns to ms
                sender.send(Utc::now().timestamp_millis() as u64)?; // round half up ns to ms
            }
            Some(Err(e)) => {
                eprintln!("Error: {}", e);
            }
            None => {
                break;
            }
        }
    }

    Ok(())
}
