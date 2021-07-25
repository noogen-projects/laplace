use borsh::{BorshDeserialize, BorshSerialize};
use derive_more::From;

#[derive(From, BorshSerialize, BorshDeserialize)]
pub enum Route {
    Http(Http),
    Websocket(Websocket),
    P2p(P2p),
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Http {
    pub body: String,
}

impl Http {
    pub fn new(body: impl Into<String>) -> Self {
        Self { body: body.into() }
    }
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub enum Websocket {
    Text(String),
}

impl Websocket {
    pub fn new_text(msg: impl Into<String>) -> Self {
        Self::Text(msg.into())
    }
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct P2p;
