use futures::{channel::mpsc::Sender, SinkExt, StreamExt};
use reqwasm::websocket::{futures::WebSocket, Message, WebSocketError};
use wasm_bindgen_futures::spawn_local;
use yew_agent::Dispatched;

use common::socket::{ClientCall, ServerCall};

use crate::event::EventBus;

pub struct WebSocketService {
    pub tx: Sender<ServerCall>,
}

impl WebSocketService {
    pub fn new() -> Self {
        let host = web_sys::window()
            .expect("Failed to get html window")
            .document()
            .expect("Failed to get html document")
            .location()
            .expect("Failed to get url location")
            .host()
            .expect("Failed to get host");
        let ws = WebSocket::open(&format!("ws://{}/", host))
            .expect("Failed to connect to server websocket");

        log::debug!("Websocket connected: {}", host);

        let (mut ws_tx, mut ws_rx) = ws.split();
        let (tx, mut rx) = futures::channel::mpsc::channel::<ServerCall>(512);
        let mut event_bus = EventBus::dispatcher();

        // Event bus reader
        spawn_local(async move {
            while let Some(call) = rx.next().await {
                let message = match common::serde_json::to_string(&call) {
                    Ok(m) => m,
                    Err(e) => {
                        log::error!("Failed to convert call to json: {}", e);
                        continue;
                    }
                };

                if let Err(e) = ws_tx.send(Message::Text(message)).await {
                    log::error!("Failed to send message via websocket: {}", e);
                }
            }
        });

        // Websocket reader
        spawn_local(async move {
            while let Some(message) = ws_rx.next().await {
                let message = match message {
                    Ok(Message::Text(m)) => m,
                    Err(WebSocketError::ConnectionClose(_)) => {
                        log::debug!("Closing websocket");
                        break;
                    }
                    _ => {
                        log::error!("Recieved non-text response from websocket: {:?}", message);
                        continue;
                    }
                };

                let call: ClientCall = match common::serde_json::from_str(&message) {
                    Ok(c) => c,
                    Err(e) => {
                        log::error!("Failed to parse json payload from server: {}", e);
                        continue;
                    }
                };

                log::debug!("{:?}", call);

                event_bus.send(call);
            }

            let close_error = ClientCall::Error(
                "Websocket closed. Please reload page to attempt reconnect".to_string(),
            );
            event_bus.send(close_error);

            log::debug!("Websocket disconnected");
        });

        Self { tx }
    }
}
