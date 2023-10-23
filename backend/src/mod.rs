use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use warp::{ws::Message, Rejection};

pub (crate) mod gpio;
pub (crate) mod handler;
pub (crate) mod ws;

pub type Result<T> = std::result::Result<T, Rejection>;
pub type Clients = Arc<RwLock<HashMap<String, Client>>>;

#[derive(Debug, Clone)]
pub struct Client {
    pub id: String,
    pub sender: mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>,
}

pub type Sample = u64;
