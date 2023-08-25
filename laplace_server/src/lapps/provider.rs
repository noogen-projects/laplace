use std::future::Future;
use std::io;
use std::sync::Arc;

use actix_web::HttpResponse;
use derive_more::Deref;
use laplace_common::lapp::Permission;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use truba::Context;

use crate::error::{error_response, ServerResult};
use crate::lapps::{LappsManager, SharedLapp};
use crate::service::Addr;
use crate::settings::LappsSettings;

#[derive(Clone, Deref)]
#[deref(forward)]
pub struct LappsProvider(Arc<RwLock<LappsManager>>);

impl LappsProvider {
    pub async fn new(settings: &LappsSettings, ctx: Context<Addr>) -> io::Result<Self> {
        let manager = LappsManager::new(settings, ctx).await?;

        Ok(Self(Arc::new(RwLock::new(manager))))
    }

    pub async fn read_manager(&self) -> RwLockReadGuard<LappsManager> {
        self.0.read().await
    }

    pub async fn write_manager(&self) -> RwLockWriteGuard<LappsManager> {
        self.0.write().await
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
        self.handle(move |lapps_provider| async move {
            lapps_provider
                .read_manager()
                .await
                .lapp(&lapp_name)?
                .read()
                .await
                .check_enabled_and_allow_permissions(permissions)?;

            handler(lapps_provider, lapp_name).await
        })
        .await
    }

    pub async fn handle_allowed_lapp<Fut>(
        self: Arc<Self>,
        permissions: &[Permission],
        lapp_name: String,
        handler: impl FnOnce(String, SharedLapp) -> Fut,
    ) -> HttpResponse
    where
        Fut: Future<Output = ServerResult<HttpResponse>>,
    {
        self.handle(move |lapps_provider| async move {
            let manager = lapps_provider.read_manager().await;
            let lapp = manager.lapp(&lapp_name)?;
            lapp.read().await.check_enabled_and_allow_permissions(permissions)?;

            handler(lapp_name, lapp).await
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
        handler: impl FnOnce(String, SharedLapp) -> Fut,
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
