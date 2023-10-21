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

impl Addr {
    pub fn as_lapp_name(&self) -> &str {
        match self {
            Addr::Lapp(name) => name.as_str(),
        }
    }

    pub fn into_lapp_name(self) -> String {
        self.into()
    }
}

impl From<Addr> for String {
    fn from(addr: Addr) -> Self {
        match addr {
            Addr::Lapp(value) => value,
        }
    }
}
