use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::{
    sync::{mpsc, RwLock},
    task::JoinSet,
};
use warp::{ws::Message, Filter, Rejection};

mod data;
mod gpio;
mod handler;
mod ws;

type Result<T> = std::result::Result<T, Rejection>;
type Clients = Arc<RwLock<HashMap<String, Client>>>;

#[derive(Debug, Clone)]
pub struct Client {
    pub id: String,
    pub subscribed_to_setup: bool,
    pub sender: mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>,
}

pub struct Sample {
    timestamp: u64,
    duration: Option<u64>,
}

#[tokio::main]
async fn main() {
    let clients: Clients = Arc::new(RwLock::new(HashMap::new()));

    let mut join_set = JoinSet::new();

    join_set.spawn(gpio::run(clients.clone(), false));
    join_set.spawn(warp_run(clients.clone()));

    while join_set.join_next().await.is_some() {}
}

async fn warp_run(clients: Clients) {
    let frontend = warp::get().and(warp::fs::dir("frontend"));

    let health_route = warp::path!("health").and_then(handler::health_handler);

    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(with_clients(clients.clone()))
        .and_then(handler::ws_handler);

    let routes = health_route
        .or(frontend)
        .or(ws_route)
        .with(warp::cors().allow_any_origin());

    warp::serve(routes).run(([0, 0, 0, 0], 8001)).await;
}

fn with_clients(clients: Clients) -> impl Filter<Extract = (Clients,), Error = Infallible> + Clone {
    warp::any().map(move || clients.clone())
}
