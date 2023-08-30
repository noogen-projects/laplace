use std::future::Future;
use std::io;
use std::sync::Arc;

use axum::response::IntoResponse;
use derive_more::Deref;
use laplace_common::lapp::Permission;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use truba::Context;

use crate::error::ServerResult;
use crate::lapps::{LappsManager, SharedLapp};
use crate::service::Addr;
use crate::settings::LappsSettings;
use crate::web_api::{err_into_json_response, ResultResponse};

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

    pub async fn handle<Fut, Res>(self, handler: impl FnOnce(Self) -> Fut) -> ResultResponse<Res>
    where
        Fut: Future<Output = ServerResult<Res>>,
        Res: IntoResponse,
    {
        handler(self).await.map_err(err_into_json_response)
    }

    pub async fn handle_allowed<Fut, Res>(
        self,
        permissions: &[Permission],
        lapp_name: String,
        handler: impl FnOnce(Self, String) -> Fut,
    ) -> ResultResponse<Res>
    where
        Fut: Future<Output = ServerResult<Res>>,
        Res: IntoResponse,
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

    pub async fn handle_allowed_lapp<Fut, Res>(
        self,
        permissions: &[Permission],
        lapp_name: String,
        handler: impl FnOnce(String, SharedLapp) -> Fut,
    ) -> ResultResponse<Res>
    where
        Fut: Future<Output = ServerResult<Res>>,
        Res: IntoResponse,
    {
        self.handle(move |lapps_provider| async move {
            let manager = lapps_provider.read_manager().await;
            let lapp = manager.lapp(&lapp_name)?;
            lapp.read().await.check_enabled_and_allow_permissions(permissions)?;

            handler(lapp_name, lapp).await
        })
        .await
    }

    pub async fn handle_client_http<Fut, Res>(
        self,
        lapp_name: String,
        handler: impl FnOnce(Self, String) -> Fut,
    ) -> ResultResponse<Res>
    where
        Fut: Future<Output = ServerResult<Res>>,
        Res: IntoResponse,
    {
        self.handle_allowed(&[Permission::ClientHttp], lapp_name, handler).await
    }

    pub async fn handle_client_http_lapp<Fut, Res>(
        self,
        lapp_name: String,
        handler: impl FnOnce(String, SharedLapp) -> Fut,
    ) -> ResultResponse<Res>
    where
        Fut: Future<Output = ServerResult<Res>>,
        Res: IntoResponse,
    {
        self.handle_allowed_lapp(&[Permission::ClientHttp], lapp_name, handler)
            .await
    }

    pub async fn handle_ws<Fut, Res>(
        self,
        lapp_name: String,
        handler: impl FnOnce(Self, String) -> Fut,
    ) -> ResultResponse<Res>
    where
        Fut: Future<Output = ServerResult<Res>>,
        Res: IntoResponse,
    {
        self.handle_allowed(&[Permission::ClientHttp, Permission::Websocket], lapp_name, handler)
            .await
    }
}
