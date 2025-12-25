use std::sync::Arc;

use crate::{ws, App, WarpResult};
use warp::Reply;

pub async fn ws_handler(ws: warp::ws::Ws, app: Arc<App>) -> WarpResult<impl Reply> {
    Ok(ws.on_upgrade(move |socket| ws::client_connection(socket, app)))
}

pub async fn health_handler() -> WarpResult<impl Reply> {
    Ok("OK")
}
