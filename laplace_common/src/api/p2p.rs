use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Peer {
    pub peer_id: Vec<u8>,
    pub keypair: Vec<u8>,
}
