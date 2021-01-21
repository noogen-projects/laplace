use std::{fs, io, path::Path};

pub use dapla_common::dap::{ApplicationSettings, DapSettings, PermissionsSettings};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DapSettingsError {
    #[error("Settings file operation error: {0}")]
    Io(#[from] io::Error),

    #[error("Settings deserialization error: {0}")]
    Deserialize(#[from] toml::de::Error),

    #[error("Settings serialization error: {0}")]
    Serialize(#[from] toml::ser::Error),
}

pub type DapSettingsResult<T> = Result<T, DapSettingsError>;

pub trait FileSettings {
    type Settings;

    fn load(path: impl AsRef<Path>) -> DapSettingsResult<Self::Settings>;
    fn save(&self, path: impl AsRef<Path>) -> DapSettingsResult<()>;
}

impl FileSettings for DapSettings {
    type Settings = Self;

    fn load(path: impl AsRef<Path>) -> DapSettingsResult<Self> {
        let buf = fs::read(path)?;
        toml::from_slice(&buf).map_err(Into::into)
    }

    fn save(&self, path: impl AsRef<Path>) -> DapSettingsResult<()> {
        let settings = toml::to_string(self)?;
        fs::write(path, settings).map_err(Into::into)
    }
}
