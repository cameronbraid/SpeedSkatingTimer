use std::time::Instant;

use crate::{
    data::{DataMessage, SetupMessage, TimestampMessage},
    Clients, Sample,
};
use chrono::Utc;
use color_eyre::Result;
use futures::stream::StreamExt;
use gpio_cdev::{
    AsyncLineEventHandle, Chip, EventRequestFlags, EventType, LineEvent, LineRequestFlags,
};
use tokio::sync::mpsc;

pub async fn run(clients: Clients, mock: bool) {
    let (sample_sender, mut sample_recv) = mpsc::unbounded_channel();
    let (setup_sender, mut setup_recv) = mpsc::unbounded_channel();

    if mock {
        tokio::spawn(mock_gpio_sender(
            sample_sender,
            setup_sender,
            tokio::time::Duration::from_secs(1),
        ));
    } else {
        tokio::spawn(gpio_sender(sample_sender, setup_sender, 14)); // pin 40 = GPIO 21, pin 8 = GPIO 14
    }

    loop {
        tokio::select! {
            sample = sample_recv.recv() => {
                match sample {
                    Some(sample) => {
                        let  clients = clients.read().await;
                        for (_, client) in clients.iter() {
                            let response =
                                serde_json::to_string(&DataMessage::Timestamp(TimestampMessage {
                                    timestamp: sample.timestamp,
                                    duration: sample.duration,
                                }))
                                .expect("unable to serialise");
                            let _ = client.sender.send(Ok(warp::ws::Message::text(response)));
                        }
                    }
                    None => {
                        break;
                    }

                }
            },
            setup = setup_recv.recv() => {
                match setup {
                    Some(connected) => {
                        let  clients = clients.read().await;
                        for (_, client) in clients.iter().filter(|(_, client)|client.subscribed_to_setup) {
                            println!("Client {} : sending setup message", client.id);
                            let response =
                                serde_json::to_string(&DataMessage::Setup(SetupMessage {
                                    connected
                                }))
                                .expect("unable to serialise");
                            let _ = client.sender.send(Ok(warp::ws::Message::text(response)));
                        }
                    }
                    None => {
                        break;
                    }
                }
            },
        }
    }
}

async fn mock_gpio_sender(
    sender: mpsc::UnboundedSender<Sample>,
    setup_sender: mpsc::UnboundedSender<bool>,
    duration: tokio::time::Duration,
) {
    let mut connected = false;

    loop {
        connected = !connected;

        setup_sender.send(connected).unwrap();

        let _ = sender.send(Sample {
            timestamp: Utc::now().timestamp_millis() as u64,
            duration: Some(duration.as_millis() as u64),
        });
        tokio::time::sleep(duration).await;
    }
}

async fn gpio_sender(
    sample_sender: mpsc::UnboundedSender<Sample>,
    setup_sender: mpsc::UnboundedSender<bool>,
    line: u32,
) -> Result<()> {
    let mut chip = Chip::new("/dev/gpiochip0")?;

    let input = chip.get_line(line)?;

    let mut events = AsyncLineEventHandle::new(input.events(
        LineRequestFlags::INPUT,
        EventRequestFlags::BOTH_EDGES,
        "read-gpio",
    )?)?;

    let debounce = std::time::Duration::from_secs(3);
    let mut debounce_last_sent: Option<Instant> = None;
    let mut last_event: Option<LineEvent> = None;
    loop {
        match events.next().await {
            Some(Ok(evt)) => {
                setup_sender.send(matches!(evt.event_type(), EventType::RisingEdge))?;

                if let Some(last) = debounce_last_sent {
                    if last.elapsed() < debounce {
                        continue;
                    }
                }
                debounce_last_sent = Some(Instant::now());

                let duration = if let Some(last) = last_event.as_ref() {
                    Some(evt.timestamp() - last.timestamp())
                } else {
                    None
                };
                sample_sender.send(Sample {
                    timestamp: Utc::now().timestamp_millis() as u64,
                    duration,
                })?;

                last_event = Some(evt);
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
