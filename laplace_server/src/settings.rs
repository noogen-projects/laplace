use std::path::{Path, PathBuf};

use config::{Config, Environment, File};
use serde::{Deserialize, Serialize};

pub use config::ConfigError;

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct HttpSettings {
    pub host: String,
    pub port: u16,
    pub web_root: PathBuf,
    pub access_token: Option<String>,
    pub upload_file_limit: usize,
    pub print_url: bool,
}

impl Default for HttpSettings {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 8080,
            web_root: PathBuf::new(),
            access_token: None,
            upload_file_limit: 2 * 1024 * 1024 * 1024,
            print_url: true,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
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

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct P2pSettings {
    pub mdns_discovery_enabled: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct LoggerSettings {
    #[serde(default = "default_spec")]
    pub spec: String,

    pub path: Option<PathBuf>,

    pub duplicate_to_stdout: bool,

    #[serde(default = "default_keep_log_for_days")]
    pub keep_log_for_days: usize,
}

impl Default for LoggerSettings {
    fn default() -> Self {
        Self {
            spec: default_spec(),
            path: None,
            duplicate_to_stdout: false,
            keep_log_for_days: default_keep_log_for_days(),
        }
    }
}

fn default_spec() -> String {
    "info".into()
}

const fn default_keep_log_for_days() -> usize {
    7
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct LappsSettings {
    pub path: PathBuf,
}

impl Default for LappsSettings {
    fn default() -> Self {
        Self { path: "lapps".into() }
    }
}

#[derive(Default, Debug, Deserialize, Serialize)]
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
