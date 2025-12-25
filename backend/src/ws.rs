use std::sync::Arc;

use futures::{FutureExt, StreamExt};
use nanoid::nanoid;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::ws::WebSocket;

use crate::App;

pub async fn client_connection(ws: WebSocket, app: Arc<App>) {
    let (client_ws_sender, mut client_ws_rcv) = ws.split();
    let (client_sender, client_rcv) = mpsc::unbounded_channel();
    let id = nanoid!();
    let client_rcv = UnboundedReceiverStream::new(client_rcv);
    tokio::task::spawn({
        let id = id.clone();
        client_rcv.forward(client_ws_sender).map(move |result| {
            if let Err(e) = result {
                eprintln!("Client {}: error sending ws msg: {}", id, e);
            }
        })
    });

    app.new_client(id.clone(), client_sender).await;

    println!("Client {} : connected", id);

    while let Some(result) = client_ws_rcv.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("Client {} : error receiving ws message: {}", id, e);
                break;
            }
        };
        app.handle_msg_from_client(&id, msg).await;
    }

    app.client_disconnected(&id).await;
    
    println!("Client {} : disconnected", id);
}
