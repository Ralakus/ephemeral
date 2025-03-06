use std::time::{SystemTime, UNIX_EPOCH};

use maud::{html, Markup, PreEscaped};

/// History message HTMX component
pub fn message(
    icon: &'static str,
    index: usize,
    prefix: &str,
    sender: &str,
    content: &str,
) -> Markup {
    let start = SystemTime::now();
    let unix_time = start
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    html! {
        div id=(format!("message-{index}")) {
            span class="mr-1" { (PreEscaped(icon)) }
            (" ")
            span class=(prefix){ (sender) }
            (" ")
            span class="font-light text-neutral-400" x-text=(format!("new Date({unix_time} * 1000).toLocaleString()")) { (unix_time) }
            br;
            span class="pl-3 flex flex-row gap-4" {
                span class="font-light text-neutral-300" { "-" }
                (content)
            }
        }
    }
}
