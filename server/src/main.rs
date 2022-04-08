#![warn(clippy::nursery, clippy::pedantic)]

use clap::Parser;
use futures::{FutureExt, StreamExt};
use uuid::Uuid;
use warp::{ws::Message, Filter};

use std::{collections::HashMap, sync::Arc};

use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_stream::wrappers::{BroadcastStream, UnboundedReceiverStream};

use common::serde_json;
use common::socket::{ClientCall, ServerCall};

/// Command line arguments for server
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

    // Broadcast for sending messages to all clients
    let (broadcast_tx, _broadcast_rx) = broadcast::channel::<String>(64);
    let broadcast_filter =
        warp::any().map(move || (broadcast_tx.clone(), broadcast_tx.subscribe()));

    // Client data for passing to socket
    let clients: Clients = Arc::new(RwLock::new(HashMap::new()));
    let clients_filter = warp::any().map(move || clients.clone());

    // Websocket route handler
    let socket = warp::any()
        .and(broadcast_filter)
        .and(clients_filter)
        .and(warp::ws())
        .map(|broadcast, clients, ws: warp::ws::Ws| {
            ws.on_upgrade(move |socket| handle_socket(broadcast, clients, socket))
        });

    // Health check route for service managers
    let health = warp::path!("health").map(|| "Ephemeral OK");

    // Static file warp routes
    let static_route = warp::fs::dir("www");

    // Warp routes for the server
    let routes = health.or(socket).or(static_route);

    // Running the server with routes
    warp::serve(routes).run(([0, 0, 0, 0], args.port)).await;

    Ok(())
}

/// Arc and Rwlock proection around the client map
type Clients = Arc<RwLock<ClientsInner>>;

/// Inner client map for storing all clients
type ClientsInner = HashMap<u128, Client>;

/// Each connected client
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Client {
    username: Option<String>,
}

/// Websocket handler. Handles incomming connections and creates clients
async fn handle_socket(
    broadcast: (broadcast::Sender<String>, broadcast::Receiver<String>),
    clients: Clients,
    socket: warp::ws::WebSocket,
) {
    let (sender, reciever) = socket.split();
    let (broadcast_tx, broadcast_rx) = broadcast;
    let mut broadcast_rx = BroadcastStream::new(broadcast_rx);
    let uuid = Uuid::new_v4().as_u128();

    let (tx, rx) = mpsc::unbounded_channel();
    let rx = UnboundedReceiverStream::new(rx);

    {
        let mut clients = clients.write().await;
        clients.insert(uuid, Client { username: None });
    }

    // Transmits messages to the client's websocket
    let mut client_transmit = tokio::spawn(rx.forward(sender).map(move |result| {
        if let Err(e) = result {
            eprintln!("Failed to write to client's {} websocket : {}", uuid, e);
        }
    }));

    // Forwards broadcasted message to client's reciever
    let broadcast_recieve_tx = tx.clone();
    let mut broadcast_recieve = tokio::spawn(async move {
        let tx = broadcast_recieve_tx;
        while let Some(Ok(message)) = broadcast_rx.next().await {
            if let Err(e) = tx.send(Ok(Message::text(message))) {
                eprintln!("Failed to write to client's {} rx : {}", uuid, e);
            }
        }
    });

    // Processes client's message from websocket
    let client_recieve_clients = clients.clone();
    let client_recieve_broadcast_tx = broadcast_tx.clone();
    let mut client_recieve = tokio::spawn(async move {
        let clients = client_recieve_clients;
        let broadcast_tx = client_recieve_broadcast_tx;
        process_client_messages(reciever, uuid, clients, broadcast_tx, tx).await;
    });

    // If one tasks ends, kill all the other tasks
    tokio::select! {
        _ = (&mut client_transmit) => {
            broadcast_recieve.abort();
            client_recieve.abort();
        }
        _ = (&mut broadcast_recieve) => {
            client_transmit.abort();
            client_recieve.abort();
        }
        _ = (&mut client_recieve) => {
            broadcast_recieve.abort();
            client_transmit.abort();
        }
    };

    // Send user disconnect message if the user was connected with a username
    if let Some(client) = clients.read().await.get(&uuid) {
        if let Some(username) = client.username.clone() {
            let payload = ClientCall::Disconnection { username };

            match serde_json::to_string(&payload) {
                Ok(p) => {
                    if let Err(e) = broadcast_tx.send(p) {
                        eprintln!(
                            "Failed to send user disconnect payload for client {} : {}",
                            uuid, e
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Failed to create json payload for user disconnect for client {} : {}",
                        uuid, e
                    );
                }
            };
        }
    }

    let mut clients = clients.write().await;
    clients.remove(&uuid);
    eprintln!("Client {} disconnected", uuid);
}

/// Processes messages recieved from client's websocket
async fn process_client_messages(
    mut reciever: futures::stream::SplitStream<warp::ws::WebSocket>,
    uuid: u128,
    clients: Clients,
    broadcast_tx: broadcast::Sender<String>,
    tx: mpsc::UnboundedSender<Result<warp::ws::Message, warp::Error>>,
) {
    while let Some(Ok(message)) = reciever.next().await {
        let message = if let Ok(m) = message.to_str() {
            m.into()
        } else if message.is_ping() {
            format!("{{\"ok\":\"Pong\"}}")
        } else {
            format!("{{\"notification\":\"Non-text call. Ignoring processing.\"}}",)
        };

        let call = match serde_json::from_str(&message) {
            Ok(c) => c,
            Err(e) => ServerCall::Error(format!(
                "Failed to parse json {} into ServerCall from client {} : {}",
                message, uuid, e
            )),
        };

        println!("Client {} : {}", uuid, call);

        let mut clients = clients.write().await;
        let client = if let Some(c) = clients.get_mut(&uuid) {
            c
        } else {
            eprintln!("Client {} does not exist", uuid);
            continue;
        };

        let response = match process_call(client, &broadcast_tx, call) {
            Ok(r) => r,
            Err(e) => format!("{{\"error\":\"{}\"}}", e),
        };

        if let Err(e) = tx.send(Ok(Message::text(response))) {
            eprintln!("Failed to send response to client {} : {}", uuid, e);
            continue;
        }
    }
}

/// Processes server call and returns a json payload or error
fn process_call(
    client: &mut Client,
    broadcast_tx: &broadcast::Sender<String>,
    call: ServerCall,
) -> Result<String, Box<dyn std::error::Error>> {
    let payload = match call {
        ServerCall::Connect { username } => {
            client.username = Some(username.clone());
            let payload = ClientCall::Connection {
                username: username.clone(),
            };
            let message = serde_json::to_string(&payload)?;
            broadcast_tx.send(message)?;
            ClientCall::Ok(format!("Successfully joined as {}", username))
        }
        ServerCall::Send { content } => {
            if let Some(ref username) = client.username {
                let payload = ClientCall::PushMessage {
                    sender: username.clone(),
                    content,
                };

                let message = serde_json::to_string(&payload)?;
                broadcast_tx.send(message)?;
                ClientCall::Ok("Message successfully sent".into())
            } else {
                ClientCall::Error("Client not connected with username".into())
            }
        }
        ServerCall::Notification(notification) => ClientCall::Notification(notification),
        ServerCall::Ok(message) => ClientCall::Ok(message),
        ServerCall::Error(error) => ClientCall::Error(error),
    };

    Ok(serde_json::to_string(&payload)?)
}
