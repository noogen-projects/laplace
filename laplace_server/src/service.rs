use derive_more::Display;

pub use self::gossipsub::GossipsubService;
pub use self::lapp::LappService;
pub use self::websocket::WebSocketService;

pub mod gossipsub;
pub mod lapp;
pub mod websocket;

#[derive(Debug, Hash, Clone, Eq, PartialEq, Display)]
pub enum Addr {
    #[display(fmt = "Lapp({})", _0)]
    Lapp(String),
}
