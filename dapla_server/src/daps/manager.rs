use std::{collections::HashMap, convert::TryFrom, fs, io, path::Path};

use log::{error, info};
use wasmer::Instance;

use crate::{
    daps::{service, ExpectedInstance},
    error::{ServerError, ServerResult},
    Dap,
};

pub struct DapsManager {
    daps: HashMap<String, Dap>,
    service_senders: HashMap<String, service::Sender>,
}

impl DapsManager {
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
                service_senders: Default::default(),
            })
    }

    pub fn load(&mut self, dap_name: impl AsRef<str>) -> ServerResult<()> {
        let dap_name = dap_name.as_ref();
        self.daps
            .get_mut(dap_name)
            .ok_or_else(|| ServerError::DapNotFound(dap_name.to_string()))?
            .instantiate()
    }

    pub fn unload(&mut self, dap_name: impl AsRef<str>) -> bool {
        self.daps
            .get_mut(dap_name.as_ref())
            .map(|dap| dap.instance.take().is_some())
            .unwrap_or(false)
    }

    pub fn load_daps(&mut self) {
        for (name, dap) in &mut self.daps {
            if !dap.is_main() && dap.enabled() && !dap.is_loaded() {
                info!("Load dap '{}'", name);
                dap.instantiate().expect("Dap should be loaded");
            }
        }
    }

    pub fn is_loaded(&self, dap_name: impl AsRef<str>) -> bool {
        self.daps
            .get(dap_name.as_ref())
            .map(|dap| dap.is_loaded())
            .unwrap_or(false)
    }

    pub fn dap(&self, dap_name: impl AsRef<str>) -> ServerResult<&Dap> {
        let dap_name = dap_name.as_ref();
        self.daps
            .get(dap_name)
            .ok_or_else(|| ServerError::DapNotFound(dap_name.to_string()))
    }

    pub fn dap_mut(&mut self, dap_name: impl AsRef<str>) -> ServerResult<&mut Dap> {
        let dap_name = dap_name.as_ref();
        self.daps
            .get_mut(dap_name)
            .ok_or_else(|| ServerError::DapNotFound(dap_name.to_string()))
    }

    pub fn daps_iter(&self) -> impl Iterator<Item = &Dap> {
        self.daps.values()
    }

    pub fn daps_iter_mut(&mut self) -> impl Iterator<Item = &mut Dap> {
        self.daps.values_mut()
    }

    pub fn instance(&self, dap_name: impl AsRef<str>) -> ServerResult<Instance> {
        let dap_name = dap_name.as_ref();
        self.daps
            .get(dap_name)
            .and_then(|dap| dap.instance.clone())
            .ok_or_else(|| ServerError::DapNotLoaded(dap_name.to_string()))
    }

    pub fn service_sender(&mut self, dap_name: impl AsRef<str>) -> ServerResult<service::Sender> {
        let dap_name = dap_name.as_ref();
        if let Some(sender) = self.service_senders.get(dap_name) {
            Ok(sender.clone())
        } else {
            let dap = self
                .daps
                .get(dap_name)
                .ok_or_else(|| ServerError::DapNotFound(dap_name.to_string()))?;
            let instance = dap
                .instance
                .clone()
                .ok_or_else(|| ServerError::DapNotLoaded(dap_name.to_string()))?;

            let (service, sender) = service::DapService::new(ExpectedInstance::try_from(instance)?);
            actix::spawn(service.run());

            self.service_senders.insert(dap_name.to_string(), sender.clone());
            Ok(sender)
        }
    }
}
