use chrono::Utc;
use nanoid::nanoid;

use std::convert::Infallible;
use std::sync::Arc;
use std::{collections::HashMap, time::Instant};
use tokio::{
    spawn,
    sync::{mpsc, RwLock},
    task::JoinSet,
};
use warp::reject::Rejection;
use warp::{ws::Message, Filter};

mod gpio;
mod handler;
mod ws;

type WarpResult<T> = std::result::Result<T, Rejection>;

struct App {
    clients: RwLock<HashMap<String, Client>>,
    lap_controller: RwLock<LapController>,
}

struct StartedLap {
    id: String,
    last_sent: Instant,
    event_ts: u64,
}
struct LapController {
    last_switch_state: Option<SwitchState>,

    started_lap: Option<StartedLap>,
}
impl LapController {
    pub fn new() -> Self {
        Self {
            last_switch_state: None,
            started_lap: None,
        }
    }
    pub fn reset_lap(&mut self) -> Option<Vec<LapTimerEvent>> {
        if let Some(last_lap) = &self.started_lap {
            let now_ms = Utc::now().timestamp_millis() as u64;
            let event = LapTimerEvent::Aborted(LapAborted {
                id: last_lap.id.to_string(),
                timestamp_ms: now_ms,
            });
            self.started_lap = None;
            Some(vec![event])
        } else {
            self.started_lap = None;
            None
        }
    }

    pub fn update_switch_state(&mut self, switch_state: SwitchState) -> Option<Vec<LapTimerEvent>> {
        let value = switch_state.value;
        let new_ts = switch_state.timestamp_ms;

        // trigger the next lap event
        self.last_switch_state = Some(switch_state);

        if value {
            // only consider falling edges for lap timing
            return None;
        }

        let now_ms = Utc::now().timestamp_millis() as u64;
        let now_instant = Instant::now();
        const DEBOUNCE_PERIOD: std::time::Duration = std::time::Duration::from_secs(3);

        if let Some(last_lap) = &self.started_lap {
            if last_lap.last_sent.elapsed() < DEBOUNCE_PERIOD {
                return None;
            }
        }

        let mut timer_events = Vec::with_capacity(2);
        match &self.started_lap {
            Some(started_lap) => {
                // the end of the previous lap
                // the start of the next lap
                timer_events.push(LapTimerEvent::Finished(LapFinished {
                    id: started_lap.id.clone(),
                    timestamp_ms: now_ms,
                    duration_ms: new_ts - started_lap.event_ts,
                }));

                let new_id = nanoid!();
                self.started_lap.replace(StartedLap {
                    id: new_id.clone(),
                    last_sent: now_instant,
                    event_ts: new_ts,
                });
                timer_events.push(LapTimerEvent::Started(LapStarted {
                    id: new_id,
                    timestamp_ms: now_ms,
                }));
            }
            None => {
                // the start of the first lap
                let new_id = nanoid!();

                timer_events.push(LapTimerEvent::Started(LapStarted {
                    id: new_id.clone(),
                    timestamp_ms: now_ms,
                }));

                self.started_lap = Some(StartedLap {
                    id: new_id,
                    last_sent: now_instant,
                    event_ts: new_ts,
                });
            }
        }

        Some(timer_events)
    }
}

impl App {
    pub async fn start() -> Arc<Self> {
        let app = Arc::new(Self {
            clients: RwLock::new(HashMap::new()),
            lap_controller: RwLock::new(LapController::new()),
        });

        spawn(gpio::run(app.clone()));

        app
    }

    pub async fn new_client(
        self: &Arc<Self>,
        id: String,
        sender: mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>,
    ) {
        let mut clients = self.clients.write().await;
        let client = Client {
            id: id.clone(),
            monitor_subscribed: false,
            sender,
        };
        clients.insert(id, client);
    }

    pub async fn client_disconnected(self: &Arc<Self>, id: &str) {
        let mut clients = self.clients.write().await;
        clients.remove(id);
    }

    pub async fn handle_msg_from_client(self: &Arc<Self>, id: &str, msg: Message) {
        let mut clients = self.clients.write().await;

        if let Some(client) = clients.get_mut(id) {
            if let Ok(text) = msg.to_str() {
                eprintln!("Client {} : received message: {}", id, text);
                let Ok(msg) = serde_json::from_str::<DataMessage>(text) else {
                    println!("Client {} : unable to parse message: {}", id, text);
                    return;
                };
                match msg {
                    DataMessage::MonitorSubscribe => {
                        eprintln!("Client {} : Subscribe to setup", client.id);
                        client.monitor_subscribed = true;
                        // send the current state to the client
                        let lock = self.lap_controller.read().await;
                        if let Some(state) = lock.last_switch_state.clone() {
                            let _ = client
                                .send(DataMessage::Monitor(MonitorEvent::Initial(state.clone())));
                        }
                    }
                    DataMessage::MonitorUnSubscribe => {
                        eprintln!("Client {} : UnSubscribe to setup", client.id);
                        client.monitor_subscribed = false;
                    }
                    DataMessage::ResetLap => {
                        eprintln!("Client {} : Requested Reset", client.id);
                        // todo! add security and only allow permitted users to reset

                        // tell the LapController to reset - this will Abort the current lap and inform clients
                        // and send all clients any new events
                        if let Some(events) = self.lap_controller.write().await.reset_lap() {
                            for event in events.iter() {
                                for (_, client) in clients.iter() {
                                    let _ = client.send(DataMessage::Lap(event.clone()));
                                }
                            }
                        }
                    }
                    m => {
                        eprintln!("Client {} : unknown received message: {:?}", id, m);
                    }
                };
            }
        }
    }

    pub async fn send_heartbeat(self: &Arc<Self>) {
        let clients = self.clients.read().await;
        for (_, client) in clients.iter() {
            let _ = client.send(DataMessage::Heartbeat(HeartbeatMessage {
                timestamp_ms: Utc::now().timestamp_millis() as u64,
            }));
        }
    }

    pub async fn update_switch_state(self: &Arc<Self>, switch_state: SwitchState) {
        let lap_events = self
            .lap_controller
            .write()
            .await
            .update_switch_state(switch_state.clone());

        // send the switch state to all monitoring clients
        let clients = self.clients.read().await;
        for (_, client) in clients
            .iter()
            .filter(|(_, client)| client.monitor_subscribed)
        {
            println!("Client {} : sending monitor message", client.id);
            let _ = client.send(DataMessage::Monitor(MonitorEvent::Change(
                switch_state.clone(),
            )));
        }

        // send the lap events to all clients
        if let Some(events) = lap_events {
            let clients = self.clients.read().await;
            for (_, client) in clients.iter() {
                for event in events.iter() {
                    let _ = client.send(DataMessage::Lap(event.clone()));
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    pub id: String,
    pub monitor_subscribed: bool,
    pub sender: mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>,
}

impl Client {
    pub fn send(&self, msg: DataMessage) {
        let response = serde_json::to_string(&msg).expect("unable to serialise");
        let _ = self.sender.send(Ok(warp::ws::Message::text(response)));
    }
}

pub struct LapTimer {
    pub debounce_ms: u64,
    pub monitor: mpsc::UnboundedSender<MonitorEvent>,
    pub laps: mpsc::UnboundedSender<LapTimerEvent>,
}
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum LapTimerEvent {
    Started(LapStarted),
    Updated(LapUpdated),
    Finished(LapFinished),
    Aborted(LapAborted),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct LapStarted {
    pub id: String,
    pub timestamp_ms: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct LapUpdated {
    pub id: String,
    pub timestamp_ms: u64,
    pub duration_ms: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct LapFinished {
    pub id: String,
    pub timestamp_ms: u64,
    pub duration_ms: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct LapAborted {
    pub id: String,
    pub timestamp_ms: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum MonitorEvent {
    Initial(SwitchState),
    Change(SwitchState),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct SwitchState {
    pub timestamp_ms: u64,
    pub value: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(tag = "type")]
pub enum DataMessage {
    #[serde(rename = "reset")]
    ResetLap,

    #[serde(rename = "lap")]
    Lap(LapTimerEvent),

    #[serde(rename = "setup")]
    Monitor(MonitorEvent),

    #[serde(rename = "monitor-subscribe")]
    MonitorSubscribe,

    #[serde(rename = "monitor-unsubscribe")]
    MonitorUnSubscribe,

    #[serde(rename = "timestamp")]
    Heartbeat(HeartbeatMessage),
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct HeartbeatMessage {
    pub timestamp_ms: u64,
}

#[tokio::main]
async fn main() {
    let app = App::start().await;

    let mut join_set = JoinSet::new();
    join_set.spawn(warp_run(app));

    while join_set.join_next().await.is_some() {}
}

async fn warp_run(app: Arc<App>) {
    let frontend = warp::get().and(warp::fs::dir("frontend"));

    let health_route = warp::path!("health").and_then(handler::health_handler);

    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(with_app(app.clone()))
        .and_then(handler::ws_handler);

    let routes = health_route
        .or(frontend)
        .or(ws_route)
        .with(warp::cors().allow_any_origin());

    warp::serve(routes).run(([0, 0, 0, 0], 8001)).await;
}

fn with_app(app: Arc<App>) -> impl Filter<Extract = (Arc<App>,), Error = Infallible> + Clone {
    warp::any().map(move || app.clone())
}
