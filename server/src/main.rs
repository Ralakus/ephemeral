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

/// Type alias for the commonly used Arc<RwLock<T>> type
type ArcLock<T> = Arc<RwLock<T>>;

/// Arc and Rwlock proection around the client map
type Clients = ArcLock<ClientsInner>;

/// Inner client map for storing all clients
type ClientsInner = HashMap<u128, ArcLock<Client>>;

/// Data for each client that is connected
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Client {
    uuid: u128,
    username: Option<String>,
    tx: mpsc::UnboundedSender<Result<Message, warp::Error>>,
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

    let client = Arc::new(RwLock::new(Client {
        uuid,
        username: None,
        tx: tx.clone(),
    }));

    {
        let mut clients = clients.write().await;
        clients.insert(uuid, client.clone());
    }

    eprintln!("Client {:x} connected", uuid);
    let uuid_payload = ClientCall::Uuid(uuid);
    match serde_json::to_string(&uuid_payload) {
        Ok(message) => {
            if let Err(e) = tx.send(Ok(Message::text(message))) {
                eprintln!("Failed to send uuid to client {:x} : {}", uuid, e);
            }
        }
        Err(e) => {
            eprintln!(
                "Failed to create uuid payload for client {:x} : {}",
                uuid, e
            );
        }
    }

    // Transmits messages to the client's websocket
    let mut client_transmit = tokio::spawn(rx.forward(sender).map(move |result| {
        if let Err(e) = result {
            eprintln!("Failed to write to client {:x}'s websocket : {}", uuid, e);
        }
    }));

    // Forwards broadcasted message to client's reciever
    let broadcast_recieve_tx = tx.clone();
    let mut broadcast_recieve = tokio::spawn(async move {
        let tx = broadcast_recieve_tx;
        while let Some(Ok(message)) = broadcast_rx.next().await {
            if let Err(e) = tx.send(Ok(Message::text(message))) {
                eprintln!("Failed to write to client {:x}'s rx : {}", uuid, e);
            }
        }
    });

    // Processes client's message from websocket
    let client_recieve_clients = clients.clone();
    let client_recieve_client = client.clone();
    let client_recieve_broadcast_tx = broadcast_tx.clone();
    let mut client_recieve = tokio::spawn(async move {
        let clients = client_recieve_clients;
        let client = client_recieve_client;
        let broadcast_tx = client_recieve_broadcast_tx;
        process_client_messages(reciever, uuid, clients, client, broadcast_tx, tx).await;
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
    if let Some(username) = client.write().await.username.clone() {
        let payload = ClientCall::Disconnection { username };

        match serde_json::to_string(&payload) {
            Ok(p) => {
                if let Err(e) = broadcast_tx.send(p) {
                    eprintln!(
                        "Failed to send user disconnect payload for client {:x} : {}",
                        uuid, e
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "Failed to create json payload for user disconnect for client {:x} : {}",
                    uuid, e
                );
            }
        };
    }

    let mut clients = clients.write().await;
    clients.remove(&uuid);
    eprintln!("Client {:x} disconnected", uuid);
}

/// Processes messages recieved from client's websocket
async fn process_client_messages(
    mut reciever: futures::stream::SplitStream<warp::ws::WebSocket>,
    uuid: u128,
    clients: Clients,
    client: ArcLock<Client>,
    broadcast_tx: broadcast::Sender<String>,
    tx: mpsc::UnboundedSender<Result<warp::ws::Message, warp::Error>>,
) {
    while let Some(Ok(message)) = reciever.next().await {
        let message = if let Ok(m) = message.to_str() {
            m.into()
        } else if message.is_ping() {
            format!("{{\"ok\":\"Pong\"}}")
        } else if message.is_close() {
            format!("{{\"notification\":\"Goodbye\"}}")
        } else {
            format!("{{\"notification\":\"Non-text call. Ignoring processing.\"}}",)
        };

        let call = match serde_json::from_str(&message) {
            Ok(c) => c,
            Err(e) => ServerCall::Error(format!(
                "Failed to parse json {} into ServerCall from client {:x} : {}",
                message, uuid, e
            )),
        };

        println!("Client {:x} : {}", uuid, call);

        let response = match process_call(&clients, client.clone(), &broadcast_tx, call).await {
            Ok(r) => r,
            Err(e) => format!("{{\"error\":\"{}\"}}", e),
        };

        if let Err(e) = tx.send(Ok(Message::text(response))) {
            eprintln!("Failed to send response to client {:x} : {}", uuid, e);
            continue;
        }
    }
}

/// Processes server call and returns a json payload or error
async fn process_call(
    clients: &Clients,
    client: ArcLock<Client>,
    broadcast_tx: &broadcast::Sender<String>,
    call: ServerCall,
) -> Result<String, Box<dyn std::error::Error>> {
    let payload = match call {
        ServerCall::Connect { username } => {
            let mut client = client.write().await;
            client.username = Some(username.clone());
            let payload = ClientCall::Connection {
                username: username.clone(),
            };
            let message = serde_json::to_string(&payload)?;
            broadcast_tx.send(message)?;
            ClientCall::Ok(format!("Successfully joined as {}", username))
        }
        ServerCall::Send { content } => {
            let client = client.read().await;
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
        ServerCall::Command { command, args } => {
            hanndle_command(clients, client, command, args).await
        }
        ServerCall::Notification(notification) => ClientCall::Notification(notification),
        ServerCall::Ok(message) => ClientCall::Ok(message),
        ServerCall::Error(error) => ClientCall::Error(error),
    };

    Ok(serde_json::to_string(&payload)?)
}

async fn hanndle_command(
    clients: &Clients,
    client: ArcLock<Client>,
    command: String,
    _args: Vec<String>,
) -> ClientCall {
    match command.to_lowercase().as_str() {
        "help" => ClientCall::Notification("Available commands: [connected] [uuid]".into()),
        "uuid" => {
            let uuid = client.read().await.uuid;
            ClientCall::Notification(format!("Your uuid is {:x}", uuid))
        }
        "connected" => {
            let clients_lock = clients.read().await;
            let mut usernames = String::default();
            let mut connected = 0;
            for (_uuid, c) in clients_lock.iter() {
                if let Some(ref username) = c.read().await.username {
                    usernames.push_str(&format!("[{}] ", username));
                    connected += 1;
                }
            }
            ClientCall::Notification(format!("{} users connected: {}", connected, usernames))
        }
        _ => ClientCall::Notification(
            "Invalid command, try the `help` command to see available commands".into(),
        ),
    }
}
