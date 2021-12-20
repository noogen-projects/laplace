use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Message {
    pub body: String,
}

impl Message {
    pub fn new(body: impl Into<String>) -> Self {
        Self { body: body.into() }
    }
}
