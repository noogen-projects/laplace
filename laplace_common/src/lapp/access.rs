use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString, IntoStaticStr};

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, AsRefStr, IntoStaticStr, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Permission {
    FileRead,
    FileWrite,
    ClientHttp,
    Http,
    Websocket,
    Tcp,
    Database,
    Sleep,
    LappsIncoming,
    LappsOutgoing,
}

impl Permission {
    pub fn as_str(&self) -> &'static str {
        self.into()
    }
}
