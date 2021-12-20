use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub enum Message {
    Text(String),
}

impl Message {
    pub fn new_text(msg: impl Into<String>) -> Self {
        Self::Text(msg.into())
    }
}
