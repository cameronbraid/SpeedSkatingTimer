use crate::{ws, Clients, Result};
use warp::Reply;

pub async fn ws_handler(ws: warp::ws::Ws, clients: Clients) -> Result<impl Reply> {
    Ok(ws.on_upgrade(move |socket| ws::client_connection(socket, clients)))
}

pub async fn health_handler() -> Result<impl Reply> {
    Ok("OK")
}
