use std::{fs, io, path::PathBuf};

pub use dapla_common::dap::{ApplicationSettings, DapSettings, PermissionsSettings};
use derive_more::From;

#[derive(Debug, From)]
pub enum DapSettingsError {
    Io(io::Error),
    Deserialize(toml::de::Error),
}

pub type DapSettingsResult<T> = Result<T, DapSettingsError>;

pub trait FileSettings {
    type Settings;

    fn load(path: impl Into<PathBuf>) -> DapSettingsResult<Self::Settings>;
}

impl FileSettings for DapSettings {
    type Settings = Self;

    fn load(path: impl Into<PathBuf>) -> DapSettingsResult<Self> {
        let buf = fs::read(path.into())?;
        toml::from_slice(&buf).map_err(Into::into)
    }
}
