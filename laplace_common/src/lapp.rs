use std::path::Path;

use serde::{Deserialize, Serialize};

pub use self::access::*;
pub use self::settings::*;

pub mod access;
pub mod settings;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Lapp<PathT> {
    name: String,
    root_dir: PathT,
    settings: LappSettings,
}

impl<PathT> Lapp<PathT> {
    #[inline]
    pub fn new(name: impl Into<String>, root_dir: impl Into<PathT>, settings: LappSettings) -> Self {
        Self {
            name: name.into(),
            root_dir: root_dir.into(),
            settings,
        }
    }

    pub const fn static_dir_name() -> &'static str {
        "static"
    }

    pub const fn index_file_name() -> &'static str {
        "index.html"
    }

    pub const fn main_name() -> &'static str {
        "laplace"
    }

    pub fn main_static_uri() -> String {
        format!("/{}", Self::static_dir_name())
    }

    pub fn main_uri(tail: impl AsRef<str>) -> String {
        format!("/{}/{}", Self::main_name(), tail.as_ref())
    }

    pub fn main_uri2(first: impl AsRef<str>, second: impl AsRef<str>) -> String {
        format!("/{}/{}/{}", Self::main_name(), first.as_ref(), second.as_ref())
    }

    #[inline]
    pub fn is_main(&self) -> bool {
        self.name() == Self::main_name()
    }

    #[inline]
    pub fn enabled(&self) -> bool {
        self.settings.application.enabled
    }

    #[inline]
    pub fn set_enabled(&mut self, enabled: bool) {
        self.settings.application.enabled = enabled;
    }

    #[inline]
    pub fn switch_enabled(&mut self) {
        self.set_enabled(!self.enabled());
    }

    #[inline]
    pub fn title(&self) -> &str {
        &self.settings.application.title
    }

    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[inline]
    pub fn root_dir(&self) -> &PathT {
        &self.root_dir
    }

    #[inline]
    pub fn data_dir(&self) -> &Path {
        &self.settings.application.data_dir
    }

    #[inline]
    pub fn settings(&self) -> &LappSettings {
        &self.settings
    }

    #[inline]
    pub fn set_settings(&mut self, settings: LappSettings) {
        self.settings = settings;
    }

    pub fn root_uri(&self) -> String {
        format!("/{}", self.name())
    }

    pub fn static_uri(&self) -> String {
        format!("{}/{}", self.root_uri(), Self::static_dir_name())
    }

    pub fn uri(&self, tail: impl AsRef<str>) -> String {
        format!("/{}/{}", self.name(), tail.as_ref())
    }

    pub fn uri2(&self, first: impl AsRef<str>, second: impl AsRef<str>) -> String {
        format!("/{}/{}/{}", self.name(), first.as_ref(), second.as_ref())
    }

    pub fn required_permissions(&self) -> impl Iterator<Item = Permission> + '_ {
        self.settings.permissions.required.iter().copied()
    }

    pub fn allowed_permissions(&self) -> impl Iterator<Item = Permission> + '_ {
        self.settings.permissions.allowed.iter().copied()
    }

    pub fn is_allowed_permission(&self, permission: Permission) -> bool {
        self.settings.permissions.allowed.contains(&permission)
    }

    pub fn allow_permission(&mut self, permission: Permission) -> bool {
        if !self.is_allowed_permission(permission) {
            self.settings.permissions.allowed.push(permission);
            true
        } else {
            false
        }
    }

    pub fn deny_permission(&mut self, permission: Permission) -> bool {
        let index = self.allowed_permissions().position(|allowed| allowed == permission);
        if let Some(index) = index {
            self.settings.permissions.allowed.remove(index);
            true
        } else {
            false
        }
    }
}
