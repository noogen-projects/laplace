use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Deserialize, Serialize)]
pub enum WsMessage {
    AddPeer(String),
    UpdateName(String),
    Text { peer_id: String, msg: String },
}

#[derive(Debug, Deserialize, Serialize)]
pub enum WsResponse {
    Success(WsMessage),
    Error(String),
}

impl WsResponse {
    pub fn make_error_json_string<E: fmt::Debug>(err: E) -> String {
        format!(r#"{{"Error":"{:?}"}}"#, err)
    }
}
