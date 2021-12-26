use std::{fs, io, path::Path};

pub use laplace_common::lapp::{ApplicationSettings, LappSettings, PermissionsSettings};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LappSettingsError {
    #[error("Settings file operation error: {0}")]
    Io(#[from] io::Error),

    #[error("Settings deserialization error: {0}")]
    Deserialize(#[from] toml::de::Error),

    #[error("Settings serialization error: {0}")]
    Serialize(#[from] toml::ser::Error),
}

pub type LappSettingsResult<T> = Result<T, LappSettingsError>;

pub trait FileSettings {
    type Settings;

    fn load(path: impl AsRef<Path>) -> LappSettingsResult<Self::Settings>;
    fn save(&self, path: impl AsRef<Path>) -> LappSettingsResult<()>;
}

impl FileSettings for LappSettings {
    type Settings = Self;

    fn load(path: impl AsRef<Path>) -> LappSettingsResult<Self> {
        let buf = fs::read(path)?;
        toml::from_slice(&buf).map_err(Into::into)
    }

    fn save(&self, path: impl AsRef<Path>) -> LappSettingsResult<()> {
        log::debug!("Save settings to file {}\n{:#?}", path.as_ref().display(), self);

        let settings = toml::to_string(self)?;
        fs::write(path, settings).map_err(Into::into)
    }
}
