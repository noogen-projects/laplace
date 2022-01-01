use std::{collections::HashMap, convert::TryFrom, fs, io, path::Path};

use futures::executor;
use log::{error, info};
use wasmer::Instance;

use crate::{
    error::{ServerError, ServerResult},
    lapps::{service, ExpectedInstance},
    Lapp,
};

pub struct LappsManager {
    lapps: HashMap<String, Lapp>,
    service_senders: HashMap<String, service::Sender>,
    http_client: reqwest::blocking::Client,
}

impl LappsManager {
    pub fn new(lapps_path: impl AsRef<Path>) -> io::Result<Self> {
        fs::read_dir(lapps_path)?
            .map(|entry| {
                entry.and_then(|dir| {
                    let name = dir.file_name().into_string().map_err(|invalid_name| {
                        error!("Lapp name '{:?}' is not valid UTF-8", invalid_name);
                        io::Error::from(io::ErrorKind::InvalidData)
                    })?;
                    Ok((name.clone(), Lapp::new(name, dir.path())))
                })
            })
            .collect::<io::Result<_>>()
            .map(|lapps| {
                let http_client = reqwest::blocking::Client::new();
                Self {
                    lapps,
                    service_senders: Default::default(),
                    http_client,
                }
            })
    }

    pub fn load(&mut self, lapp_name: impl AsRef<str>) -> ServerResult<()> {
        let lapp_name = lapp_name.as_ref();
        let http_client = self.http_client.clone();
        self.lapps
            .get_mut(lapp_name)
            .ok_or_else(|| ServerError::LappNotFound(lapp_name.to_string()))?
            .instantiate(http_client)
    }

    pub fn unload(&mut self, lapp_name: impl AsRef<str>) -> bool {
        executor::block_on(self.service_stop(lapp_name.as_ref())); // todo: use async
        self.lapps
            .get_mut(lapp_name.as_ref())
            .map(|lapp| lapp.take_instance().is_some())
            .unwrap_or(false)
    }

    pub fn load_lapps(&mut self) {
        let http_client = self.http_client.clone();
        for (name, lapp) in &mut self.lapps {
            if !lapp.is_main() && lapp.enabled() && !lapp.is_loaded() {
                info!("Load lapp '{}'", name);
                lapp.instantiate(http_client.clone()).expect("Lapp should be loaded");
            }
        }
    }

    pub fn is_loaded(&self, lapp_name: impl AsRef<str>) -> bool {
        self.lapps
            .get(lapp_name.as_ref())
            .map(|lapp| lapp.is_loaded())
            .unwrap_or(false)
    }

    pub fn loaded_lapp(&self, lapp_name: impl AsRef<str>) -> ServerResult<(&Lapp, Instance)> {
        let lapp_name = lapp_name.as_ref();
        self.lapps
            .get(lapp_name)
            .ok_or_else(|| ServerError::LappNotFound(lapp_name.to_string()))
            .and_then(|lapp| {
                lapp.instance()
                    .ok_or_else(|| ServerError::LappNotLoaded(lapp_name.to_string()))
                    .map(|instance| (lapp, instance))
            })
    }

    pub fn lapp(&self, lapp_name: impl AsRef<str>) -> ServerResult<&Lapp> {
        let lapp_name = lapp_name.as_ref();
        self.lapps
            .get(lapp_name)
            .ok_or_else(|| ServerError::LappNotFound(lapp_name.to_string()))
    }

    pub fn lapp_mut(&mut self, lapp_name: impl AsRef<str>) -> ServerResult<&mut Lapp> {
        let lapp_name = lapp_name.as_ref();
        self.lapps
            .get_mut(lapp_name)
            .ok_or_else(|| ServerError::LappNotFound(lapp_name.to_string()))
    }

    pub fn lapps_iter(&self) -> impl Iterator<Item = &Lapp> {
        self.lapps.values()
    }

    pub fn instance(&self, lapp_name: impl AsRef<str>) -> ServerResult<Instance> {
        let lapp_name = lapp_name.as_ref();
        self.lapps
            .get(lapp_name)
            .and_then(|lapp| lapp.instance())
            .ok_or_else(|| ServerError::LappNotLoaded(lapp_name.to_string()))
    }

    pub fn service_sender(&mut self, lapp_name: impl AsRef<str>) -> ServerResult<service::Sender> {
        let lapp_name = lapp_name.as_ref();
        if let Some(sender) = self.service_senders.get(lapp_name) {
            Ok(sender.clone())
        } else {
            let lapp = self
                .lapps
                .get(lapp_name)
                .ok_or_else(|| ServerError::LappNotFound(lapp_name.to_string()))?;
            let instance = lapp
                .instance()
                .ok_or_else(|| ServerError::LappNotLoaded(lapp_name.to_string()))?;

            let (service, sender) = service::LappService::new(ExpectedInstance::try_from(instance)?);
            actix::spawn(service.run());

            self.service_senders.insert(lapp_name.to_string(), sender.clone());
            Ok(sender)
        }
    }

    pub async fn service_stop(&mut self, lapp_name: impl AsRef<str>) -> bool {
        if let Some(sender) = self.service_senders.remove(lapp_name.as_ref()) {
            sender
                .send(service::Message::Stop)
                .await
                .map_err(|err| log::error!("Error occurs when send to lapp service: {:?}", err))
                .is_ok()
        } else {
            false
        }
    }
}
