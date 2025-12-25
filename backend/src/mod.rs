use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use warp::{ws::Message, Rejection};

pub (crate) mod gpio;
pub (crate) mod handler;
pub (crate) mod ws;

pub type Result<T> = std::result::Result<T, Rejection>;
