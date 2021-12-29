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
}

impl Default for HttpSettings {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 8080,
            web_root: PathBuf::new(),
            access_token: None,
        }
    }
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
    pub p2p: P2pSettings,
    pub log: LoggerSettings,
    pub lapps: LappsSettings,
}

impl Settings {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let mut settings = Config::new();
        settings
            .merge(File::from(path.as_ref()))?
            // Add in settings from the environment (with a prefix of LAPLACE)
            // Eg.. `LAPLACE_HTTP.PORT=8090 laplace_server` would set the `http.port` key
            .merge(Environment::with_prefix("LAPLACE").separator("."))?;
        settings.try_into()
    }
}