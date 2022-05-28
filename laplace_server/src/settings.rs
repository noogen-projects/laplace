use std::path::{Path, PathBuf};

use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct HttpSettings {
    pub host: String,
    pub port: u16,
    pub web_root: PathBuf,
    pub access_token: Option<String>,
    pub print_url: bool,
}

impl Default for HttpSettings {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 8080,
            web_root: PathBuf::new(),
            access_token: None,
            print_url: true,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SslSettings {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "private_key_path_default")]
    pub private_key_path: PathBuf,

    #[serde(default = "certificate_path_default")]
    pub certificate_path: PathBuf,
}

impl Default for SslSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            private_key_path: private_key_path_default(),
            certificate_path: certificate_path_default(),
        }
    }
}

fn private_key_path_default() -> PathBuf {
    PathBuf::from("key.pem")
}

fn certificate_path_default() -> PathBuf {
    PathBuf::from("cert.pem")
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct P2pSettings {
    pub mdns_discovery_enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct LoggerSettings {
    pub spec: String,
    pub dir: Option<PathBuf>,
    pub duplicate_to_stdout: bool,
}

impl Default for LoggerSettings {
    fn default() -> Self {
        Self {
            spec: "info".into(),
            dir: None,
            duplicate_to_stdout: false,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct LappsSettings {
    pub path: PathBuf,
}

impl Default for LappsSettings {
    fn default() -> Self {
        Self { path: "lapps".into() }
    }
}

#[derive(Default, Debug, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub http: HttpSettings,
    pub ssl: SslSettings,
    pub p2p: P2pSettings,
    pub log: LoggerSettings,
    pub lapps: LappsSettings,
}

impl Settings {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(File::from(path.as_ref()))
            // Add in settings from the environment (with a prefix of LAPLACE)
            // Eg.. `LAPLACE_HTTP.PORT=8090 laplace_server` would set the `http.newport` key
            .add_source(Environment::with_prefix("LAPLACE").separator("."))
            .build()?;
        config.try_deserialize()
    }
}
