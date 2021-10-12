use std::{convert::TryFrom, str::FromStr};

use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString, IntoStaticStr, ParseError};

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
}

impl Permission {
    pub fn as_str(&self) -> &'static str {
        self.into()
    }
}

impl TryFrom<&str> for Permission {
    type Error = ParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        FromStr::from_str(value)
    }
}
