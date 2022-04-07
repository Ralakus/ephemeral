pub use json;
pub use serde;
pub use serde_json;

pub mod socket {
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ServerCall {
        Send { content: String },
        Connect { username: String },

        Error(String),
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
}
