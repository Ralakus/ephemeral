pub use json;
pub use serde;
pub use serde_json;

pub mod socket {
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ServerCall {
        Connect { username: String },
        Send { content: String },
        Notification(String),

        Ok(String),
        Error(String),
    }

    impl std::fmt::Display for ServerCall {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Connect { username } => write!(f, "connect: {}", username),
                Self::Send { content } => write!(f, "send: {}", content),
                Self::Notification(notification) => write!(f, "notification: {}", notification),
                Self::Ok(message) => write!(f, "ok: {}", message),
                Self::Error(error) => write!(f, "error: {}", error),
            }
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ClientCall {
        PushMessage { sender: String, content: String },
        Connection { username: String },
        Disconnection { username: String },
        Notification(String),

        Ok(String),
        Error(String),
    }

    impl std::fmt::Display for ClientCall {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::PushMessage { sender, content } => {
                    write!(f, "push message: {} | {}", sender, content)
                }
                Self::Connection { username } => write!(f, "connection: {}", username),
                Self::Disconnection { username } => write!(f, "disconnection: {}", username),
                Self::Notification(noticiation) => write!(f, "notification: {}", noticiation),
                Self::Ok(message) => write!(f, "ok: {}", message),
                Self::Error(error) => write!(f, "error: {}", error),
            }
        }
    }
}
