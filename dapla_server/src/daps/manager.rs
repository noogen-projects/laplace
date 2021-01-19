use std::{collections::HashMap, fs, io, path::Path};

use log::{error, info};
use wasmer::Instance;

use super::{Dap, DapError, DapResult};

pub struct DapsManager {
    daps: HashMap<String, Dap>,
    instances: HashMap<String, Instance>,
}

impl DapsManager {
    pub const MAIN_CLIENT_APP_NAME: &'static str = "dapla";

    pub fn new(daps_path: impl AsRef<Path>) -> io::Result<Self> {
        fs::read_dir(daps_path)?
            .map(|entry| {
                entry.and_then(|dir| {
                    let name = dir.file_name().into_string().map_err(|invalid_name| {
                        error!("Dap name '{:?}' is not valid UTF-8", invalid_name);
                        io::Error::from(io::ErrorKind::InvalidData)
                    })?;
                    Ok((name.clone(), Dap::new(name, dir.path())))
                })
            })
            .collect::<io::Result<_>>()
            .map(|daps| Self {
                daps,
                instances: Default::default(),
            })
    }

    pub fn load(&mut self, dap_name: impl AsRef<str> + Into<String>) -> DapResult<()> {
        if let Some(dap) = self.daps.get(dap_name.as_ref()) {
            self.instances.insert(dap_name.into(), dap.instantiate()?);
            Ok(())
        } else {
            Err(DapError::NotFound(dap_name.into()))
        }
    }

    pub fn load_daps(&mut self) {
        for (name, dap) in &self.daps {
            if !dap.is_main_client() && dap.enabled() && !self.is_loaded(&name) {
                info!("Load dap '{}'", name);
                let instance = dap.instantiate().expect("Dap should be loaded");
                self.instances.insert(name.into(), instance);
            }
        }
    }

    pub fn is_loaded(&self, dap_name: impl AsRef<str>) -> bool {
        self.instances.contains_key(dap_name.as_ref())
    }

    pub fn daps_iter(&self) -> impl Iterator<Item = &Dap> {
        self.daps.values()
    }

    pub fn daps_iter_mut(&mut self) -> impl Iterator<Item = &mut Dap> {
        self.daps.values_mut()
    }

    pub fn instance(&self, dap_name: impl AsRef<str>) -> Option<&Instance> {
        self.instances.get(dap_name.as_ref())
    }
}
