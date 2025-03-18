use maud::{html, Markup, DOCTYPE};

/// Not found page
pub fn not_found() -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width,initial-scale=1";

                title { "Ephemeral" }

                link rel="icon" type="image/ico" href="/favicon.ico";
                link rel="stylesheet" href="/css/index.css";
            }
            body class="flex flex-col h-screen bg-white antialiased" {
                div class="flex flex-col m-auto text-center justify-center space-y-2" {
                    h1 class="text-xl" {
                        "Page not found"
                    }
                }
            }
        }
    }
}
