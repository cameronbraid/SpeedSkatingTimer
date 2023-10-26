use crate::{
    data::{DataMessage, ResetMessage},
    Client, Clients,
};
use futures::{FutureExt, StreamExt};
use nanoid::nanoid;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::ws::{Message, WebSocket};

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
        subscribed_to_setup: false,
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

async fn client_msg(id: &str, msg: Message, clients: &Clients) {
    let mut clients = clients.write().await;

    if let Some(client) = clients.get_mut(id) {
        if let Ok(text) = msg.to_str() {
            println!("Client {} : received message: {}", id, text);
            let msg: DataMessage = serde_json::from_str(text).unwrap();
            match msg {
                DataMessage::SubscribeSetup(..) => {
                    println!("Client {} : Subscribe to setup", client.id);
                    client.subscribed_to_setup = true;
                }
                DataMessage::UnSubscribeSetup(..) => {
                    println!("Client {} : UnSubscribe to setup", client.id);
                    client.subscribed_to_setup = false;
                }
                DataMessage::Reset(..) => {
                    println!("Client {} : Requested Reset", client.id);
                    // send a reset message to all clients
                    let response =
                        serde_json::to_string(&DataMessage::Reset(ResetMessage { id: Some(nanoid!()) }))
                            .expect("unable to serialise");
                    let m = warp::ws::Message::text(response);
                    for (_, client) in clients.iter() {
                        let _ = client.sender.send(Ok(m.clone()));
                    }
                }
                m => {
                    println!("Client {} : unknown received message: {:?}", id, m);
                }
            };
        }
    }
}
