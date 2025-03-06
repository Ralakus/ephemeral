use axum::{http::header, response::IntoResponse};

pub mod icons;
pub mod index;
pub mod message;

/// Static compile-time CSS data from tailwind
pub const CSS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/css/index.css"));
/// Static compile-time JS data for Alpine.js frontend library
pub const ALPINE_JS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/js/alpine.3.14.8.min.js"
));
/// Static compile-time JS data for HTMX core frontend library
pub const HTMX_JS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/js/htmx.2.0.4.min.js"
));
/// Static compile-time JS data for HTMX Websocket extension
pub const HTMX_WS_JS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/js/htmx-ws.2.0.3.min.js"
));
/// Static compile-time icon data for favicon
pub const FAVICON_ICO: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/favicon.ico"));

// Wrapper to automatically add CSS content-type header for data
pub struct Css(pub &'static str);
impl IntoResponse for Css {
    fn into_response(self) -> axum::response::Response {
        ([(header::CONTENT_TYPE, "text/css;charset=utf-8")], self.0).into_response()
    }
}

// Wrapper to automatically add JS content-type header for data
pub struct Js(pub &'static str);
impl IntoResponse for Js {
    fn into_response(self) -> axum::response::Response {
        (
            [(header::CONTENT_TYPE, "text/javascript;charset=utf-8")],
            self.0,
        )
            .into_response()
    }
}
