use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

use reqwest::blocking::Client;
use tokio::fs;
use tokio::sync::RwLockWriteGuard;

use crate::error::{ServerError, ServerResult};
use crate::lapps::SharedLapp;
use crate::settings::LappsSettings;
use crate::Lapp;

pub struct LappsManager {
    lapps: HashMap<String, SharedLapp>,
    lapps_path: PathBuf,
    http_client: Client,
}

impl LappsManager {
    pub async fn new(settings: &LappsSettings) -> io::Result<Self> {
        let mut lapps = HashMap::new();
        let mut read_dir = fs::read_dir(&settings.path).await?;

        while let Some(dir) = read_dir.next_entry().await? {
            let name = dir.file_name().into_string().map_err(|invalid_name| {
                log::error!("Lapp name '{invalid_name:?}' is not valid UTF-8");
                io::Error::from(io::ErrorKind::InvalidData)
            })?;

            if let Some(allowed_lapps) = &settings.allowed {
                if !allowed_lapps.contains(&name) {
                    continue;
                }
            }

            lapps.insert(name.clone(), SharedLapp::new(Lapp::new(name, dir.path())));
        }

        let http_client = tokio::task::spawn_blocking(Client::new).await?;
        Ok(Self {
            lapps,
            lapps_path: settings.path.clone(),
            http_client,
        })
    }

    pub fn insert_lapp(&mut self, lapp_name: impl Into<String>) {
        let lapp_name = lapp_name.into();
        let root_dir = self.lapps_path.join(&lapp_name);
        self.lapps
            .insert(lapp_name.clone(), SharedLapp::new(Lapp::new(lapp_name, root_dir)));
    }

    pub async fn load(&self, mut lapp: RwLockWriteGuard<'_, Lapp>) -> ServerResult<()> {
        let http_client = self.http_client.clone();
        lapp.instantiate(http_client).await
    }

    pub async fn unload(&self, mut lapp: RwLockWriteGuard<'_, Lapp>) -> ServerResult<()> {
        lapp.take_instance();
        lapp.service_stop().await;

        Ok(())
    }

    pub async fn load_lapps(&self) {
        for (name, shared_lapp) in &self.lapps {
            let lapp = shared_lapp.read().await;
            if !lapp.is_main() && lapp.enabled() && !lapp.is_loaded() {
                log::info!("Load lapp '{name}'");

                drop(lapp);
                self.load(shared_lapp.write().await)
                    .await
                    .expect("Lapp should be loaded");
            }
        }
    }

    pub fn lapp_dir(&self, lapp_name: impl AsRef<str>) -> PathBuf {
        self.lapps_path.join(lapp_name.as_ref())
    }

    pub async fn is_loaded(&self, lapp_name: impl AsRef<str>) -> bool {
        if let Ok(lapp) = self.lapp(lapp_name.as_ref()) {
            lapp.read().await.is_loaded()
        } else {
            false
        }
    }

    pub fn lapp(&self, lapp_name: impl AsRef<str> + ToString) -> ServerResult<SharedLapp> {
        let lapp = self
            .lapps
            .get(lapp_name.as_ref())
            .ok_or_else(|| ServerError::LappNotFound(lapp_name.to_string()))?;
        Ok(lapp.clone())
    }

    pub fn lapps_iter(&self) -> impl Iterator<Item = &SharedLapp> {
        self.lapps.values()
    }
}
