use std::{
    future::Future,
    io,
    ops::Deref,
    path::PathBuf,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use actix_web::HttpResponse;
use futures::{future, FutureExt};
use laplace_common::lapp::Permission;

use crate::{
    error::{error_response, ServerError, ServerResult},
    lapps::{Instance, Lapp, LappsManager},
};

#[derive(Clone)]
pub struct LappsProvider(Arc<RwLock<LappsManager>>);

impl LappsProvider {
    pub fn new(lapps_path: impl Into<PathBuf>) -> io::Result<Self> {
        LappsManager::new(lapps_path).map(|manager| Self(Arc::new(RwLock::new(manager))))
    }

    pub fn read_manager(&self) -> ServerResult<RwLockReadGuard<LappsManager>> {
        self.0.read().map_err(|_| ServerError::LappsManagerNotLock)
    }

    pub fn write_manager(&self) -> ServerResult<RwLockWriteGuard<LappsManager>> {
        self.0.write().map_err(|_| ServerError::LappsManagerNotLock)
    }

    pub async fn handle<Fut>(self: Arc<Self>, handler: impl FnOnce(Self) -> Fut) -> HttpResponse
    where
        Fut: Future<Output = ServerResult<HttpResponse>>,
    {
        handler(Self::clone(&self)).await.unwrap_or_else(error_response)
    }

    pub async fn handle_allowed<Fut>(
        self: Arc<Self>,
        permissions: &[Permission],
        lapp_name: String,
        handler: impl FnOnce(Self, String) -> Fut,
    ) -> HttpResponse
    where
        Fut: Future<Output = ServerResult<HttpResponse>>,
    {
        self.handle(move |lapps_provider| {
            lapps_provider
                .read_manager()
                .and_then(|manager| {
                    manager
                        .loaded_lapp(&lapp_name)
                        .and_then(|(lapp, _)| lapp.check_enabled_and_allow_permissions(permissions))
                })
                .map(|_| handler(lapps_provider, lapp_name).left_future())
                .unwrap_or_else(|err| future::ready(Err(err)).right_future())
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
        self.handle(move |lapps_provider| {
            lapps_provider
                .read_manager()
                .and_then(|manager| {
                    manager.loaded_lapp(&lapp_name).and_then(|(lapp, instance)| {
                        lapp.check_enabled_and_allow_permissions(permissions)?;
                        Ok(handler(lapp_name, &*lapp, instance).left_future())
                    })
                })
                .unwrap_or_else(|err| future::ready(Err(err)).right_future())
        })
        .await
    }

    pub async fn handle_client_http<Fut>(
        self: Arc<Self>,
        lapp_name: String,
        handler: impl FnOnce(Self, String) -> Fut,
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
        handler: impl FnOnce(Self, String) -> Fut,
    ) -> HttpResponse
    where
        Fut: Future<Output = ServerResult<HttpResponse>>,
    {
        self.handle_allowed(&[Permission::ClientHttp, Permission::Websocket], lapp_name, handler)
            .await
    }
}

impl Deref for LappsProvider {
    type Target = RwLock<LappsManager>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
