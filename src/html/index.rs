use maud::{html, Markup, PreEscaped, DOCTYPE};

use crate::html::{icons, message::message};

/// Messages to be shown to all users upon first load
pub const WELCOME_MESSAGES: [&str; 2] = [
    "Welcome!",
    "Please type in a username in the box below and submit to join",
];

/// Application web root
pub async fn index() -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width,initial-scale=1";

                title { "Ephemeral" }

                link rel="icon" type="image/ico" href="/favicon.ico";
                link rel="stylesheet" href="/css/index.css";

                script src="/js/alpine.3.14.8.min.js" defer {}
                script src="/js/htmx.2.0.4.min.js" {}
                script src="/js/htmx-ws.2.0.3.min.js" {}
            }
            body
                class="flex flex-col h-screen bg-white antialiased"
                hx-ext="ws"
                ws-connect="/ws"
                x-data=(format!("{{ historyLength: {}, username: null }}", WELCOME_MESSAGES.len()))
                x-on:htmx:ws-after-message="historyLength += 1; let div = document.createElement('div'); div.id = `message-${historyLength}`; $refs.history.prepend(div);"
                x-on:htmx:ws-open="username=null"
                x-bind:hx-target="`#message-${historyLength}`" {

                div x-ref="history" class="flex flex-grow mx-12 md:mx-32 my-8 flex-col-reverse overflow-auto font-mono" {
                    div id=(format!("message-{}", WELCOME_MESSAGES.len())) {}
                    @for (i, welcome_message) in WELCOME_MESSAGES.iter().rev().enumerate() {
                        (message(icons::BELL_ALERT, i, "text-orange-400", "Server", welcome_message))
                    }
                }

                form
                    class="flex px-3 mb-6 w-full px-12 md:px-32"
                    autocomplete="off"
                    ws-send x-on:submit="if (!username) {username = $refs.input.value;}  $nextTick(() => $refs.input.value = '')" {

                    input
                        x-ref="input"
                        type="text"
                        name="message"
                        x-bind:placeholder="username ? `Message as ${username}` : `Please enter a username...`"
                        class="font-mono block w-full rounded-tl rounded-bl border border-neutral-400 focus:outline-none focus:border-emerald-700 focus:border px-2";

                    button
                        type="submit"
                        class="flex h-8 w-8 p-1 rounded-tr rounded-br transition ease-in-out bg-neutral-400 hover:bg-emerald-700 text-white" {
                        (PreEscaped(icons::PAPER_AIRPLANE))
                    }
                }
            }
        }
    }
}
