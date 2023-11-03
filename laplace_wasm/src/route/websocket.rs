use borsh::{BorshDeserialize, BorshSerialize};
use derive_more::From;

#[derive(Debug, BorshSerialize, BorshDeserialize, From)]
pub enum MessageIn {
    #[from]
    Message(Message),
    Response {
        id: String,
        result: Result<(), String>,
    },
    Timeout,
    Error(String),
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct MessageOut {
    pub id: String,
    pub msg: Message,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    Close,
}

impl Message {
    pub fn new_text(msg: impl Into<String>) -> Self {
        Self::Text(msg.into())
    }
}
