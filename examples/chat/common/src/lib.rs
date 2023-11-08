use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Peer {
    pub peer_id: Vec<u8>,
    pub keypair: Vec<u8>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChatWsMessage {
    pub peer_id: String,
    pub msg: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ChatWsRequest {
    AddPeer(String),
    AddAddress(String),
    UpdateName(String),
    SendMessage(ChatWsMessage),
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ChatWsResponse {
    AddPeerResult(String, Result<(), String>),
    AddAddressResult(String, Result<(), String>),
    SendMessageResult(String, Result<(), String>),
    ReceiveMessage(ChatWsMessage),
    InternalError(String),
}

impl ChatWsResponse {
    pub fn make_error_json_string<E: fmt::Debug>(err: E) -> String {
        format!(r#"{{"InternalError":"{err:?}"}}"#)
    }
}
