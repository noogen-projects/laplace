use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::path::PathBuf;

use futures::future::{self, Either};
use futures::{FutureExt, TryFutureExt};
use laplace_common::api::UpdateQuery;
use laplace_common::lapp::{LappSettings, Permission};
use laplace_wasm::http;
use reqwest::Client;
use tokio::fs;
use truba::{Context, Sender};

use crate::error::{ServerError, ServerResult};
use crate::lapps::settings::FileSettings;
use crate::lapps::LappDir;
use crate::service::lapp::LappServiceMessage;
use crate::service::{Addr, LappService};
use crate::settings::LappsSettings;
use crate::Lapp;

pub struct LappsManager {
    lapp_settings: HashMap<String, LappSettings>,
    lapps_path: PathBuf,
    http_client: Client,
    ctx: Context<Addr>,
}

impl LappsManager {
    pub async fn new(settings: &LappsSettings, ctx: Context<Addr>) -> io::Result<Self> {
        let mut lapp_settings = HashMap::new();
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

            if let Some(settings) = Lapp::load_settings(&name, dir.path()) {
                lapp_settings.insert(name, settings);
            }
        }

        Ok(Self {
            lapp_settings,
            lapps_path: settings.path.clone(),
            http_client: Client::new(),
            ctx,
        })
    }

    pub fn ctx(&self) -> &Context<Addr> {
        &self.ctx
    }

    pub fn insert_lapp_settings(&mut self, lapp_name: impl Into<String>) {
        let lapp_name = lapp_name.into();
        let lapp_dir = self.lapp_dir(&lapp_name);

        if let Some(settings) = Lapp::load_settings(&lapp_name, lapp_dir) {
            self.lapp_settings.insert(lapp_name, settings);
        }
    }

    pub fn load_lapp_service(
        &self,
        lapp_name: impl Into<String>,
        lapp_settings: impl Into<LappSettings>,
    ) -> impl Future<Output = ServerResult<()>> {
        let lapp_name = lapp_name.into();
        let lapp_dir = self.lapp_dir(&lapp_name);
        let lapp_service_addr = Addr::Lapp(lapp_name);

        LappService::stop(self.ctx(), &lapp_service_addr);

        let lapp = Lapp::new(lapp_service_addr.into_lapp_name(), lapp_dir, lapp_settings.into());
        LappService::new(lapp).run(self.ctx().clone(), self.http_client.clone())
    }

    pub async fn autoload_lapps(&self) {
        for (name, settings) in &self.lapp_settings {
            if settings.is_lapp_startup_active() {
                log::info!("Autoload lapp '{name}'");

                self.load_lapp_service(name, settings.clone())
                    .await
                    .expect("Lapp should be loaded");
            }
        }
    }

    pub fn run_lapp_service_if_needed(
        &self,
        lapp_name: impl Into<String>,
    ) -> impl Future<Output = ServerResult<Sender<LappServiceMessage>>> {
        let lapp_name = lapp_name.into();
        let lapp_settings = match self.lapp_settings(&lapp_name) {
            Ok(lapp_settings) => lapp_settings,
            Err(err) => return Either::Left(future::err(err)),
        };
        let lapp_service_addr = Addr::Lapp(lapp_name);

        match self.ctx().get_actor_sender::<LappServiceMessage>(&lapp_service_addr) {
            Some(sender) => Either::Left(future::ok(sender)),
            None => {
                let lapp_name = lapp_service_addr.as_lapp_name();
                let lapp_dir = self.lapp_dir(lapp_name);
                let lapp = Lapp::new(lapp_name, lapp_dir, lapp_settings.clone());
                let ctx = self.ctx().clone();

                let run_fut = LappService::new(lapp).run(ctx.clone(), self.http_client.clone());
                Either::Right(run_fut.map_ok(move |()| ctx.actor_sender::<LappServiceMessage>(lapp_service_addr)))
            },
        }
    }

    pub fn process_http(
        &self,
        lapp_name: impl Into<String>,
        request: http::Request,
    ) -> impl Future<Output = ServerResult<http::Response>> {
        let lapp_name = lapp_name.into();
        let (message, response_in) = LappServiceMessage::new_http(request);

        self.run_lapp_service_if_needed(lapp_name.clone())
            .and_then(move |lapp_service_sender| {
                let send_result = lapp_service_sender.send(message).map_err(|err| {
                    log::error!("Error occurs when send to lapp service: {err:?}");
                    ServerError::LappServiceSendError(lapp_name.clone())
                });

                if let Err(err) = send_result {
                    return Either::Left(future::err(err));
                }

                Either::Right(response_in.map(move |receive_result| match receive_result {
                    Ok(response_result) => response_result,
                    Err(_) => Err(ServerError::LappNotLoaded(lapp_name)),
                }))
            })
    }

    pub fn lapp_dir(&self, lapp_name: impl AsRef<str>) -> LappDir {
        LappDir(self.lapps_path.join(lapp_name.as_ref()))
    }

    pub fn lapp_settings(&self, lapp_name: impl AsRef<str> + ToString) -> ServerResult<&LappSettings> {
        let lapp_settings = self
            .lapp_settings
            .get(lapp_name.as_ref())
            .ok_or_else(|| ServerError::LappNotFound(lapp_name.to_string()))?;
        Ok(lapp_settings)
    }

    pub fn lapp_settings_mut(&mut self, lapp_name: impl AsRef<str> + ToString) -> ServerResult<&mut LappSettings> {
        let lapp_settings = self
            .lapp_settings
            .get_mut(lapp_name.as_ref())
            .ok_or_else(|| ServerError::LappNotFound(lapp_name.to_string()))?;
        Ok(lapp_settings)
    }

    pub fn lapp_settings_iter(&self) -> impl Iterator<Item = (&String, &LappSettings)> {
        self.lapp_settings.iter()
    }

    pub fn check_enabled_and_allow_permissions(
        &self,
        lapp_name: impl AsRef<str>,
        permissions: &[Permission],
    ) -> ServerResult<()> {
        let lapp_name = lapp_name.as_ref();
        let lapp_settings = self.lapp_settings(lapp_name)?;

        if !lapp_settings.enabled() {
            return Err(ServerError::LappNotEnabled(lapp_name.into()));
        };

        for &permission in permissions {
            if !lapp_settings.permissions.is_allowed(permission) {
                return Err(ServerError::LappPermissionDenied(lapp_name.into(), permission));
            }
        }

        Ok(())
    }

    pub async fn update_lapp_settings(&mut self, query: UpdateQuery) -> ServerResult<UpdateQuery> {
        let ctx = self.ctx().clone();
        let lapp_name = query.lapp_name.clone();
        let lapp_dir = self.lapp_dir(&lapp_name);
        let lapp_settings = self.lapp_settings_mut(&lapp_name)?;

        let updated = lapp_settings.update(query, Lapp::settings_path(lapp_dir))?;

        if updated.is_applied() {
            let lapp_service_actor_id = Addr::Lapp(lapp_name);
            if LappService::is_run(&ctx, &lapp_service_actor_id) && lapp_settings.enabled() {
                LappService::stop(&ctx, &lapp_service_actor_id);
                let lapp_settings = lapp_settings.clone();
                self.load_lapp_service(lapp_service_actor_id.into_lapp_name(), lapp_settings)
                    .await?;
            }
        }

        Ok(updated)
    }
}
