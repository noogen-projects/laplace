use std::{fs, io, path::PathBuf};

use derive_more::From;
use serde::Deserialize;

use super::Permission;

#[derive(Debug, From)]
pub enum DapSettingsError {
    Io(io::Error),
    Deserialize(toml::de::Error),
}

pub type DapSettingsResult<T> = Result<T, DapSettingsError>;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ApplicationSettings {
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PermissionsSettings {
    pub required: Vec<Permission>,
    pub allowed: Vec<Permission>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct DapSettings {
    pub application: ApplicationSettings,
    pub permissions: PermissionsSettings,
}

impl DapSettings {
    pub fn load(path: impl Into<PathBuf>) -> DapSettingsResult<Self> {
        let buf = fs::read(path.into())?;
        toml::from_slice(&buf).map_err(Into::into)
    }
}
