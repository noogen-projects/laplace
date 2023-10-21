use std::path::Path;
use std::{fs, io};

use laplace_common::api::UpdateQuery;
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

    fn load(lapp_name: impl Into<String>, path: impl AsRef<Path>) -> LappSettingsResult<Self::Settings>;
    fn save(&self, path: impl AsRef<Path>) -> LappSettingsResult<()>;
    fn update(&mut self, query: UpdateQuery, path: impl AsRef<Path>) -> LappSettingsResult<UpdateQuery>;
}

impl FileSettings for LappSettings {
    type Settings = Self;

    fn load(lapp_name: impl Into<String>, path: impl AsRef<Path>) -> LappSettingsResult<Self> {
        let content = fs::read_to_string(path)?;
        let mut settings: LappSettings = toml::from_str(&content)?;
        settings.lapp_name = lapp_name.into();

        Ok(settings)
    }

    fn save(&self, path: impl AsRef<Path>) -> LappSettingsResult<()> {
        log::debug!("Save settings to file {}\n{:#?}", path.as_ref().display(), self);

        let settings = toml::to_string(self)?;
        fs::write(path, settings).map_err(Into::into)
    }

    fn update(&mut self, mut query: UpdateQuery, path: impl AsRef<Path>) -> LappSettingsResult<UpdateQuery> {
        if let Some(enabled) = query.enabled {
            if self.enabled() != enabled {
                self.set_enabled(enabled);
            } else {
                query.enabled = None;
            }
        }

        if let Some(permission) = query.allow_permission {
            if !self.permissions.allow(permission) {
                query.allow_permission = None;
            }
        }

        if let Some(permission) = query.deny_permission {
            if !self.permissions.deny(permission) {
                query.deny_permission = None;
            }
        }

        self.save(path)?;
        Ok(query)
    }
}
