use serde::{Deserialize, Serialize};

pub use self::{access::*, settings::*};

pub mod access;
pub mod settings;

#[derive(Debug, Deserialize, Serialize)]
pub struct Dap<P> {
    name: String,
    root_dir: P,
    settings: DapSettings,
}

impl<P> Dap<P> {
    #[inline]
    pub fn new(name: impl Into<String>, root_dir: impl Into<P>, settings: DapSettings) -> Self {
        Self {
            name: name.into(),
            root_dir: root_dir.into(),
            settings,
        }
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
    pub fn root_dir(&self) -> &P {
        &self.root_dir
    }

    #[inline]
    pub fn settings(&self) -> &DapSettings {
        &self.settings
    }

    #[inline]
    pub fn set_settings(&mut self, settings: DapSettings) {
        self.settings = settings;
    }
}
