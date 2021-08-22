use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub enum Message {
    Dial(String),
    Text { peer_id: String, msg: String },
}
