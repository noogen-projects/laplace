pub use self::gossipsub::GossipsubService;
pub use self::lapp::LappService;
pub use self::websocket::WebSocketService;

pub mod gossipsub;
pub mod lapp;
pub mod websocket;

#[derive(Debug, Hash, Clone, Eq, PartialEq)]
pub enum Addr {
    Lapp(String),
}
