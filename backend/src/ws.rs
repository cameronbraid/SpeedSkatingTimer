use crate::{ Client, Clients };
use futures::{ FutureExt, StreamExt };
use nanoid::nanoid;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::ws::{ Message, WebSocket };

pub async fn client_connection(ws: WebSocket, clients: Clients) {
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

    let client = Client {
        id: id.clone(),
        sender: client_sender,
    };
    clients.write().await.insert(id.clone(), client);

    println!("Client {} : connected", id);

    while let Some(result) = client_ws_rcv.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("Client {} : error receiving ws message: {}", id, e);
                break;
            }
        };
        client_msg(&id, msg, &clients).await;
    }

    clients.write().await.remove(&id);
    println!("Client {} : disconnected", id);
}

async fn client_msg(id: &str, msg: Message, _clients: &Clients) {
    println!("Client {} : ignoring message from : {:?}", id, msg);
}
