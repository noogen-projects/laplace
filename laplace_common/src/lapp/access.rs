use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString, IntoStaticStr};

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, AsRefStr, IntoStaticStr, EnumString)]
#[serde(try_from = "&str")]
#[serde(into = "&str")]
pub enum Permission {
    #[strum(serialize = "file_read")]
    FileRead,

    #[strum(serialize = "file_write")]
    FileWrite,

    #[strum(serialize = "client_http")]
    ClientHttp,

    #[strum(serialize = "http")]
    Http,

    #[strum(serialize = "websocket")]
    Websocket,

    #[strum(serialize = "tcp")]
    Tcp,

    #[strum(serialize = "database")]
    Database,

    #[strum(serialize = "sleep")]
    Sleep,

    #[strum(serialize = "lapps_incoming")]
    LappsIncoming,

    #[strum(serialize = "lapps_outgoing")]
    LappsOutgoing,
}

impl Permission {
    pub fn as_str(&self) -> &'static str {
        self.into()
    }
}
