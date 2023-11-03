use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Peer {
    pub peer_id: Vec<u8>,
    pub keypair: Vec<u8>,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ChatWsMessage {
    AddPeer(String),
    AddAddress(String),
    UpdateName(String),
    Text { peer_id: String, msg: String },
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ChatWsResponse {
    Success(ChatWsMessage),
    Error(String),
}

impl ChatWsResponse {
    pub fn make_error_json_string<E: fmt::Debug>(err: E) -> String {
        format!(r#"{{"Error":"{err:?}"}}"#)
    }
}
