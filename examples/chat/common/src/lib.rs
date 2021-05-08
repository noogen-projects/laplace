use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub enum WsRequest {
    AddPeer(String),
    UpdateName(String),
    SendMessage { peer_id: String, msg: String },
}
