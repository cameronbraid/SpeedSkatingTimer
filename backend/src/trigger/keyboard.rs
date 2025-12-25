//! Keyboard-based trigger source

use color_eyre::eyre::eyre;
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use tokio::sync::mpsc;
use tracing::info;

use crate::trigger::clock_monotonic_ns;
use crate::types::TriggerEvent;

/// Keyboard-based trigger source
pub async fn run(trigger_key: char, tx: mpsc::UnboundedSender<TriggerEvent>) -> Result<()> {
    info!("Keyboard trigger active. Press '{}' to trigger.", trigger_key);

    // Use crossterm's event polling in a blocking task
    loop {
        // Poll for events with a timeout
        let has_event = tokio::task::spawn_blocking(|| {
            event::poll(std::time::Duration::from_millis(100))
        })
        .await
        .map_err(|e| eyre!("Task join error: {}", e))?
        .map_err(|e| eyre!("Event poll error: {}", e))?;

        if !has_event {
            continue;
        }

        let evt = tokio::task::spawn_blocking(event::read)
            .await
            .map_err(|e| eyre!("Task join error: {}", e))?
            .map_err(|e| eyre!("Event read error: {}", e))?;

        if let Event::Key(key_event) = evt {
            // Only trigger on key press, not release
            if key_event.kind != KeyEventKind::Press {
                continue;
            }

            let matches = match key_event.code {
                KeyCode::Char(c) => c == trigger_key,
                KeyCode::Enter if trigger_key == '\n' => true,
                KeyCode::Tab if trigger_key == '\t' => true,
                _ => false,
            };

            if matches {
                let event = TriggerEvent {
                    timestamp_ns: clock_monotonic_ns(),
                    source: "keyboard".to_string(),
                };
                info!("Keyboard trigger");
                if tx.send(event).is_err() {
                    break;
                }
            }
        }
    }

    #[allow(unreachable_code)]
    Ok(())
}


