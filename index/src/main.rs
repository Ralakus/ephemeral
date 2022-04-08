#![warn(clippy::nursery, clippy::pedantic)]

use common::socket::{ClientCall, ServerCall};
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_agent::{Bridge, Bridged};

mod event;
mod socket;

use event::Bus;
use socket::WebSocketService;

enum Message {
    Send,
    Call(ClientCall),
}

struct Content {
    calls: Vec<ClientCall>,
    username: Option<String>,
    wss: WebSocketService,
    input: NodeRef,
    _producer: Box<dyn Bridge<Bus>>,
}

impl Component for Content {
    type Message = Message;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let calls = vec![
            ClientCall::Notification(String::from("Welcome!")),
            ClientCall::Notification(String::from(
                "Please type in a username in the box below and submit to join",
            )),
        ];

        Self {
            calls,
            username: None,
            wss: WebSocketService::new(),
            input: NodeRef::default(),
            _producer: Bus::bridge(ctx.link().callback(Message::Call)),
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Message::Send => {
                if let Some(input) = self.input.cast::<HtmlInputElement>() {
                    if input.value().is_empty() {
                        return false;
                    }

                    let call = if self.username.is_none() {
                        self.username = Some(input.value());

                        ServerCall::Connect {
                            username: input.value(),
                        }
                    } else {
                        ServerCall::Send {
                            content: input.value(),
                        }
                    };

                    if let Err(e) = self.wss.tx.try_send(call) {
                        let error = format!("Failed to send websocket message: {}", e);
                        log::error!("{}", error);
                        self.calls.push(ClientCall::Error(error));
                        return true;
                    }
                    input.set_value("");
                }
                false
            }
            Message::Call(call) => {
                let is_ok = matches!(call, ClientCall::Ok(_));
                if is_ok {
                    false
                } else {
                    self.calls.push(call);
                    true
                }
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let onclick = ctx.link().callback(|_| Message::Send);
        let onkeypress = ctx.link().batch_callback(|event: KeyboardEvent| {
            if event.key() == "Enter" {
                Some(Message::Send)
            } else {
                None
            }
        });

        html! {
            <>

            <main class="flex flex-grow mx-12 md:mx-32 my-8 flex-col-reverse overflow-auto font-mono">
            {
                self.calls.iter().rev().filter(|c| !matches!(c, ClientCall::Ok(_))).map(|c| {
                    let server_color = "text-orange-400";
                    let server_prefix = String::from("Server");
                    let display_pair = match c {
                        ClientCall::Connection { username } => (
                            server_color, server_prefix, format!("{} connected", username)
                        ),
                        ClientCall::Disconnection { username } => (
                            server_color, server_prefix, format!("{} disconnected", username)
                        ),
                        ClientCall::Error(error) => (
                            "text-red-500 font-bold", "ERROR".to_string(), error.clone()
                        ),
                        ClientCall::Notification(message) => (
                            server_color, server_prefix, message.clone()
                        ),
                        ClientCall::Ok(message) => (
                            "text-blue-400", server_prefix, message.clone()
                        ),
                        ClientCall::PushMessage { sender, content } => (
                            "text-emerald-700", sender.clone(), content.clone()
                        )
                    };
                    html! {
                        <div class="flex">
                            <span class={classes!(display_pair.0, "cursor-default", "flex-none", "mr-2")}>
                                {display_pair.1} {":"}
                            </span>
                            <span class="cursor-default flex-1">
                                {display_pair.2}
                            </span>
                        </div>
                    }
                }).collect::<Html>()
            }
            </main>

            <footer class="flex px-3 mb-6 w-full px-12 md:px-32">
                <input ref={self.input.clone()} {onkeypress} type="text"
                    placeholder={
                        self.username.as_ref().map_or_else(
                            || "Please enter a username...".to_string(),
                            |username| format!("Message as {}...", username)
                        )
                    }
                    class="font-mono block w-full rounded-tl rounded-bl border border-neutral-400 focus:outline-none focus:border-emerald-700 focus:border px-2"
                />
                <button {onclick} class="flex h-8 w-8 p-1 rounded-tr rounded-br transition ease-in-out bg-neutral-400 hover:bg-emerald-700">
                    <svg fill="#000000" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" class="fill-white">
                        <path d="M0 0h24v24H0z" fill="none"></path><path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z"></path>
                    </svg>
                </button>
            </footer>

            </>
        }
    }
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    yew::start_app::<Content>();
}
