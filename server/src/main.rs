#![warn(clippy::nursery, clippy::pedantic)]

use clap::Parser;
use futures::{FutureExt, StreamExt};
use uuid::Uuid;
use warp::{ws::Message, Filter};

use std::{collections::HashMap, sync::Arc};

use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;

use common::serde_json;
use common::socket::{ClientCall, ServerCall};

#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Sets the port of the embedded webserver
    #[clap(parse(try_from_str))]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Command line parsing
    let args = Args::parse();

    // Health check route for service managers
    let health = warp::path!("health").map(|| "Ephemeral OK");

    // Application data for clients
    let clients: Clients = Arc::new(RwLock::new(HashMap::new()));

    // Warp filter for clients
    let clients_filter = warp::any().map(move || clients.clone());

    // Websocket route handler
    let socket =
        warp::any()
            .and(clients_filter)
            .and(warp::ws())
            .map(|clients, ws: warp::ws::Ws| {
                ws.on_upgrade(move |socket| handle_socket(clients, socket))
            });

    // Static file warp routes
    let static_route = warp::fs::dir("www");

    // Warp routes for the server
    let routes = health.or(socket).or(static_route);

    // Running the server with routes
    warp::serve(routes).run(([0, 0, 0, 0], args.port)).await;

    Ok(())
}

#[derive(Clone, Debug)]
struct ServerError {
    message: String,
}

impl std::error::Error for ServerError {}
impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

type Clients = Arc<RwLock<ClientsInner>>;
type ClientsInner = HashMap<u128, Client>;
struct Client {
    username: Option<String>,
    sender: Option<mpsc::UnboundedSender<Result<Message, warp::Error>>>,
}

// Websocket handler
async fn handle_socket(clients: Clients, socket: warp::ws::WebSocket) {
    let (ws_tx, mut ws_rx) = socket.split();
    let (tx, rx) = mpsc::unbounded_channel();

    let rx = UnboundedReceiverStream::new(rx);

    tokio::task::spawn(rx.forward(ws_tx).map(|result| {
        if let Err(e) = result {
            eprintln!("Error sending websocket message : {}", e);
        }
    }));

    let uuid = Uuid::new_v4().as_u128();

    let client = Client {
        username: None,
        sender: Some(tx),
    };

    clients.write().await.insert(uuid, client);

    println!("Client {} connected", uuid);

    while let Some(result) = ws_rx.next().await {
        let message = match result {
            Ok(m) => m,
            Err(e) => {
                eprintln!(
                    "Failed to get websocket message from client {} : {}",
                    uuid, e
                );
                break;
            }
        };

        handle_client_msg(uuid, &clients, message).await;
    }

    {
        let mut clients = clients.write().await;
        if let Some(client) = clients.get_mut(&uuid) {
            if let Some(username) = client.username.clone() {
                if let Err(e) =
                    broadcast_to_clients(uuid, &clients, &ClientCall::Disconnection { username })
                {
                    eprintln!(
                        "Failed to broadcast client's {} disconnection : {}",
                        uuid, e
                    );
                }
            }
        }
    }

    clients.write().await.remove(&uuid);
    println!("Client {} disconnected", uuid);
}

async fn handle_client_msg(uuid: u128, clients: &Clients, message: Message) {
    println!("Call from client {} : {:?}", uuid, message);
    let message = if let Ok(m) = message.to_str() {
        m.to_string()
    } else {
        let response = format!("Call from client {} : Non text. Invalid call", uuid);
        format!("{{\"error\":\"{}\"}}", response)
    };

    let message = match serde_json::from_str(&message) {
        Ok(m) => m,
        Err(e) => {
            let response = format!(
                "Failed to parse json into ServerCall from client {} : {}",
                uuid, e
            );
            ServerCall::Error(response)
        }
    };

    let response = {
        let mut clients = clients.write().await;
        let client = if let Some(c) = clients.get_mut(&uuid) {
            c
        } else {
            println!("Client {} does not exist", uuid);
            return;
        };

        match message {
            ServerCall::Connect { username } => {
                client.username = Some(username.clone());
                match broadcast_to_clients(
                    uuid,
                    &clients,
                    &ClientCall::Connection {
                        username: username.clone(),
                    },
                ) {
                    Ok(_) => ClientCall::Ok(format!(
                        "Successfully connected with username : {}",
                        username
                    )),
                    Err(e) => ClientCall::Error(format!(
                        "Failed to send connection notification to other clients : {}",
                        e
                    )),
                }
            }
            ServerCall::Send { content } => {
                let username = match &client.username {
                    Some(u) => u,
                    None => {
                        println!("Client {} is not connected with a username", uuid);
                        return;
                    }
                };
                let payload = ClientCall::PushMessage {
                    sender: username.to_string(),
                    content,
                };

                match broadcast_to_clients(uuid, &clients, &payload) {
                    Ok(_) => ClientCall::Ok("Successfully sent message".to_string()),
                    Err(e) => ClientCall::Error(format!("Failed to send message: {}", e)),
                }
            }
            ServerCall::Error(content) => ClientCall::Error(content),
        }
    };

    if let Err(e) = send_to_client(uuid, clients, response).await {
        eprintln!("Failed to send response to client {} : {}", uuid, e);
    }
}

async fn send_to_client(
    uuid: u128,
    clients: &Clients,
    payload: ClientCall,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut client_lock = clients.write().await;

    let client = client_lock.get_mut(&uuid).ok_or(ServerError {
        message: format!("Client {} does not exist", uuid),
    })?;

    match &client.sender {
        Some(s) => {
            let payload = serde_json::to_string(&payload)?;
            s.send(Ok(Message::text(payload)))?;
            Ok(())
        }
        None => Err(ServerError {
            message: format!("Client {} does not have an mspc sender", uuid),
        }
        .into()),
    }
}

fn broadcast_to_clients(
    uuid: u128,
    clients: &ClientsInner,
    payload: &ClientCall,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Recieved broadcast event from {} : {:?}", uuid, payload);
    let json = serde_json::to_string(&payload)?;
    for (c_uuid, c) in clients.iter() {
        if let Some(sender) = &c.sender {
            if let Err(e) = sender.send(Ok(Message::text(&json))) {
                eprintln!(
                    "Failed to send message to client {} from client {} : {}",
                    c_uuid, uuid, e
                );
            }
        }
    }

    Ok(())
}
