use borsh::{BorshDeserialize, BorshSerialize};
use derive_more::From;

pub mod gossipsub;
pub mod http;
pub mod websocket;

#[derive(Debug, From, BorshSerialize, BorshDeserialize)]
pub enum Route {
    Http(http::Message),
    Websocket(websocket::Message),
    Gossipsub(gossipsub::MessageOut),
}
