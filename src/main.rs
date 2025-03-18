#![warn(clippy::nursery, clippy::pedantic)]

use std::{
    net::{IpAddr, Ipv6Addr, SocketAddr},
    str::FromStr,
    sync::Arc,
};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        WebSocketUpgrade,
    },
    http::{HeaderValue, Method},
    routing::{any, get},
    Extension, Router,
};
use clap::Parser;
use futures::{stream::SplitStream, SinkExt, StreamExt};
use maud::Markup;
use serde::Deserialize;
use tokio::sync::{broadcast, RwLock};
use tower_http::{
    compression::{CompressionLayer, DefaultPredicate},
    cors::CorsLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod html;

use html::{icons, index::WELCOME_MESSAGES, message::message};

/// Command line arguments for server.
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Sets the port of the embedded webserver.
    #[clap()]
    port: u16,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // Axum logs rejections from built-in extractors with the `axum::rejection`
                // target, at `TRACE` level. `axum::rejection=trace` enables showing those events.
                format!(
                    "{}=debug,tower_http=debug,axum::rejection=trace",
                    env!("CARGO_CRATE_NAME")
                )
                .into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Command line parsing.
    let args = Args::parse();

    // Create broadcast channel and wrap the transmit for the Axum state.
    let (broadcast_tx, _broadcast_rx) = broadcast::channel::<BroadcastEvent>(128);
    let state = Arc::new(RwLock::new(broadcast_tx));

    let comression_layer: CompressionLayer = CompressionLayer::new()
        .br(true)
        .deflate(true)
        .gzip(true)
        .zstd(true)
        .compress_when(DefaultPredicate::new());

    let cors_layer = CorsLayer::new()
        // allow `GET` when accessing the resource
        .allow_methods([Method::GET])
        // allow requests from one origin
        .allow_origin("http://licas.dev".parse::<HeaderValue>().unwrap());

    let routes =
        Router::new()
            .route(
                "/ws",
                any(
                    |ws: WebSocketUpgrade,
                     Extension(state): Extension<
                        Arc<RwLock<broadcast::Sender<BroadcastEvent>>>,
                    >| async move {
                        ws.on_upgrade(|socket| handle_socket(socket, state))
                    },
                ),
            )
            .route("/", get(html::index::index))
            .fallback(html::static_handler)
            .layer(Extension(state))
            .layer(comression_layer)
            .layer(cors_layer);

    // Binding and running the HTTP server.
    tracing::info!("Running server on http://localhost:{}", args.port);
    let socket = SocketAddr::new(IpAddr::V6(Ipv6Addr::from_str("::").unwrap()), args.port);
    let listener = tokio::net::TcpListener::bind(socket).await.unwrap();
    axum::serve(listener, routes.into_make_service())
        .await
        .unwrap();
}

/// Server-wide broadcast event type
#[derive(Clone, Debug)]
pub struct BroadcastEvent {
    pub event: BroadcastEventType,
    pub sender: Option<String>,
    pub content: Option<String>,
}

/// Broadcast event type
#[derive(Clone, Debug)]
pub enum BroadcastEventType {
    #[allow(dead_code)]
    Alert,
    Join,
    Leave,
    #[allow(dead_code)]
    Ok,
    #[allow(dead_code)]
    Error,
    Message,
}

impl BroadcastEvent {
    /// Convert broadcast data into valid HTMX data block
    #[must_use]
    pub fn into_htmx(self, index: usize) -> Markup {
        let sender = self
            .sender
            .map_or_else(|| "Server".to_string(), |sender| sender);

        let content = &self.content.unwrap_or_default();

        match self.event {
            BroadcastEventType::Alert => message(
                icons::BELL_ALERT,
                index,
                "text-orange-400",
                &sender,
                content,
            ),
            BroadcastEventType::Join => message(
                icons::USER_PLUS,
                index,
                "text-orange-400",
                &sender,
                &format!("{content} joined"),
            ),
            BroadcastEventType::Leave => message(
                icons::USER_MINUS,
                index,
                "text-orange-400",
                &sender,
                &format!("{content} left"),
            ),
            BroadcastEventType::Ok => {
                message(icons::CHECK, index, "text-blue-400", &sender, content)
            }
            BroadcastEventType::Error => message(
                icons::X_MARK,
                index,
                "text-red-500 font-bold",
                &sender,
                content,
            ),
            BroadcastEventType::Message => message(
                icons::CHAT_BUBBLE_BOTTOM_CENTER_TEXT,
                index,
                "text-emerald-700",
                &sender,
                content,
            ),
        }
    }
}

/// Message type expected from websocket client
#[derive(Clone, Deserialize)]
struct WebsocketMessage {
    message: String,
}

/// Websocket setup and handler
async fn handle_socket(socket: WebSocket, state: Arc<RwLock<broadcast::Sender<BroadcastEvent>>>) {
    let (mut socket_tx, socket_rx) = socket.split();
    let broadcast_tx = state.read().await.clone();
    let mut broadcast_rx = broadcast_tx.subscribe();
    let uuid: u128 = uuid::Uuid::new_v4().as_u128();
    let username: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(None));

    tracing::info!("Client {uuid:x} connected");

    // Task for listening to websocket message from client
    // client -> broadcast
    let mut socket_recieve_task = {
        let username = username.clone();
        tokio::spawn(async move {
            socket_listen(uuid, username, socket_rx, broadcast_tx).await;
        })
    };

    // Task for listening to broadcast messages from server
    // broadcast -> client
    let mut broadcast_recieve_task = {
        let username = username.clone();
        tokio::spawn(async move {
            let mut counter: usize = WELCOME_MESSAGES.len();
            while let Ok(message) = broadcast_rx.recv().await {
                // Do not forward broadcast messages unless user has chosen a username
                let username_read = username.read().await.clone();
                if username_read.is_some() {
                    if let Err(e) = socket_tx
                        .send(Message::text(message.into_htmx(counter).into_string()))
                        .await
                    {
                        tracing::error!(
                            "Failed to send user htmx payload for client {uuid:x} : {e}"
                        );
                    }

                    counter += 1;
                }
            }
        })
    };

    // Allowed due to bug within clippy/tokio
    // https://github.com/tokio-rs/tokio/issues/5616
    #[allow(clippy::redundant_pub_crate)]
    {
        // If one tasks ends, kill all the other tasks
        tokio::select! {
            _ = (&mut socket_recieve_task) => {
                broadcast_recieve_task.abort();
            }
            _ = (&mut broadcast_recieve_task) => {
                socket_recieve_task.abort();
            }
        };
    };

    tracing::info!("Client {uuid:x} disconnected");

    // Once all tasks end, client has disconnected
    // Broadcast disconnect message
    let username_read = username.read().await.clone();
    if let Some(name) = username_read {
        let payload = BroadcastEvent {
            event: BroadcastEventType::Leave,
            sender: None,
            content: Some(name),
        };

        let broadcast_tx = state.read().await.clone();
        if let Err(e) = broadcast_tx.send(payload) {
            tracing::error!("Failed to send user disconnect payload for client {uuid:x} : {e}");
        }
    }
}

async fn socket_listen(
    uuid: u128,
    username: Arc<RwLock<Option<String>>>,
    mut socket_rx: SplitStream<WebSocket>,
    broadcast_tx: broadcast::Sender<BroadcastEvent>,
) {
    while let Some(Ok(Message::Text(message))) = socket_rx.next().await {
        let message: String = match serde_json::from_str::<WebsocketMessage>(&message) {
            Ok(m) => m.message,
            Err(e) => {
                tracing::error!("Failed to parse input payload from client {uuid:x} : {e}");
                continue;
            }
        };

        if message.is_empty() {
            continue;
        }

        // If user has joined with username, broadcast their message
        // otherwise broadcast their join.
        let username_read = username.read().await.clone();
        let payload = if let Some(name) = username_read {
            tracing::info!("{uuid:x} [{name}] : {message}");

            BroadcastEvent {
                event: BroadcastEventType::Message,
                sender: Some(name),
                content: Some(message),
            }
        } else {
            tracing::info!("{uuid:x} [{message}] joined");

            username.write().await.replace(message.clone());

            BroadcastEvent {
                event: BroadcastEventType::Join,
                sender: None,
                content: Some(message),
            }
        };

        if let Err(e) = broadcast_tx.send(payload) {
            tracing::error!("Failed to send user disconnect payload for client {uuid:x} : {e}");
        }
    }
}
