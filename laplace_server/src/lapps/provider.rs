use std::{
    future::Future,
    io,
    ops::Deref,
    path::Path,
    sync::{Arc, Mutex},
};

use actix_web::HttpResponse;
use futures::future;
use laplace_common::lapp::Permission;
use log::error;

use crate::{
    error::{error_response, ServerError, ServerResult},
    lapps::{Instance, Lapp, LappsManager},
};

#[derive(Clone)]
pub struct LappsProvider(Arc<Mutex<LappsManager>>);

impl LappsProvider {
    pub fn new(lapps_path: impl AsRef<Path>) -> io::Result<Self> {
        LappsManager::new(lapps_path).map(|manager| Self(Arc::new(Mutex::new(manager))))
    }

    pub async fn handle<Fut>(self: Arc<Self>, handler: impl FnOnce(&mut LappsManager) -> Fut) -> HttpResponse
    where
        Fut: Future<Output = ServerResult<HttpResponse>>,
    {
        match self.lock().map_err(|err| {
            error!("Lapps service lock should be asquired: {:?}", err);
            ServerError::LappsServiceNotLock
        }) {
            Ok(mut lapps_manager) => handler(&mut lapps_manager).await.unwrap_or_else(error_response),
            Err(err) => error_response(err),
        }
    }

    pub async fn handle_allowed<Fut>(
        self: Arc<Self>,
        permissions: &[Permission],
        lapp_name: String,
        handler: impl FnOnce(&mut LappsManager, String) -> Fut,
    ) -> HttpResponse
    where
        Fut: Future<Output = ServerResult<HttpResponse>>,
    {
        self.handle(move |lapps_manager| {
            lapps_manager
                .loaded_lapp(&lapp_name)
                .and_then(|(lapp, _)| lapp.check_enabled_and_allow_permissions(permissions))
                .map(|_| future::Either::Left(handler(lapps_manager, lapp_name)))
                .unwrap_or_else(|err| future::Either::Right(future::ready(Err(err))))
        })
        .await
    }

    pub async fn handle_allowed_lapp<Fut>(
        self: Arc<Self>,
        permissions: &[Permission],
        lapp_name: String,
        handler: impl FnOnce(String, &Lapp, Instance) -> Fut,
    ) -> HttpResponse
    where
        Fut: Future<Output = ServerResult<HttpResponse>>,
    {
        self.handle(move |lapps_manager| {
            lapps_manager
                .loaded_lapp(&lapp_name)
                .and_then(|(lapp, instance)| {
                    lapp.check_enabled_and_allow_permissions(permissions)?;
                    Ok(future::Either::Left(handler(lapp_name, lapp, instance)))
                })
                .unwrap_or_else(|err| future::Either::Right(future::ready(Err(err))))
        })
        .await
    }

    pub async fn handle_client_http<Fut>(
        self: Arc<Self>,
        lapp_name: String,
        handler: impl FnOnce(&mut LappsManager, String) -> Fut,
    ) -> HttpResponse
    where
        Fut: Future<Output = ServerResult<HttpResponse>>,
    {
        self.handle_allowed(&[Permission::ClientHttp], lapp_name, handler).await
    }

    pub async fn handle_client_http_lapp<Fut>(
        self: Arc<Self>,
        lapp_name: String,
        handler: impl FnOnce(String, &Lapp, Instance) -> Fut,
    ) -> HttpResponse
    where
        Fut: Future<Output = ServerResult<HttpResponse>>,
    {
        self.handle_allowed_lapp(&[Permission::ClientHttp], lapp_name, handler)
            .await
    }

    pub async fn handle_ws<Fut>(
        self: Arc<Self>,
        lapp_name: String,
        handler: impl FnOnce(&mut LappsManager, String) -> Fut,
    ) -> HttpResponse
    where
        Fut: Future<Output = ServerResult<HttpResponse>>,
    {
        self.handle_allowed(&[Permission::ClientHttp, Permission::Websocket], lapp_name, handler)
            .await
    }
}

impl Deref for LappsProvider {
    type Target = Mutex<LappsManager>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
