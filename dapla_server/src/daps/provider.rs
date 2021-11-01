use std::{
    future::Future,
    io,
    ops::Deref,
    path::Path,
    sync::{Arc, Mutex},
};

use actix_web::HttpResponse;
use dapla_common::dap::Permission;
use futures::future;
use log::error;

use crate::{
    daps::{Dap, DapsManager, Instance},
    error::{error_response, ServerError, ServerResult},
};

#[derive(Clone)]
pub struct DapsProvider(Arc<Mutex<DapsManager>>);

impl DapsProvider {
    pub fn new(daps_path: impl AsRef<Path>) -> io::Result<Self> {
        DapsManager::new(daps_path).map(|manager| Self(Arc::new(Mutex::new(manager))))
    }

    pub async fn handle<Fut>(self: Arc<Self>, handler: impl FnOnce(&mut DapsManager) -> Fut) -> HttpResponse
    where
        Fut: Future<Output = ServerResult<HttpResponse>>,
    {
        match self.lock().map_err(|err| {
            error!("Daps service lock should be asquired: {:?}", err);
            ServerError::DapsServiceNotLock
        }) {
            Ok(mut daps_manager) => handler(&mut daps_manager).await.unwrap_or_else(error_response),
            Err(err) => error_response(err),
        }
    }

    pub async fn handle_allowed<Fut>(
        self: Arc<Self>,
        permissions: &[Permission],
        dap_name: String,
        handler: impl FnOnce(&mut DapsManager, String) -> Fut,
    ) -> HttpResponse
    where
        Fut: Future<Output = ServerResult<HttpResponse>>,
    {
        self.handle(move |daps_manager| {
            daps_manager
                .loaded_dap(&dap_name)
                .and_then(|(dap, _)| dap.check_enabled_and_allow_permissions(permissions))
                .map(|_| future::Either::Left(handler(daps_manager, dap_name)))
                .unwrap_or_else(|err| future::Either::Right(future::ready(Err(err))))
        })
        .await
    }

    pub async fn handle_allowed_dap<Fut>(
        self: Arc<Self>,
        permissions: &[Permission],
        dap_name: String,
        handler: impl FnOnce(String, &Dap, Instance) -> Fut,
    ) -> HttpResponse
    where
        Fut: Future<Output = ServerResult<HttpResponse>>,
    {
        self.handle(move |daps_manager| {
            daps_manager
                .loaded_dap(&dap_name)
                .and_then(|(dap, instance)| {
                    dap.check_enabled_and_allow_permissions(permissions)?;
                    Ok(future::Either::Left(handler(dap_name, dap, instance)))
                })
                .unwrap_or_else(|err| future::Either::Right(future::ready(Err(err))))
        })
        .await
    }

    pub async fn handle_client_http<Fut>(
        self: Arc<Self>,
        dap_name: String,
        handler: impl FnOnce(&mut DapsManager, String) -> Fut,
    ) -> HttpResponse
    where
        Fut: Future<Output = ServerResult<HttpResponse>>,
    {
        self.handle_allowed(&[Permission::ClientHttp], dap_name, handler).await
    }

    pub async fn handle_client_http_dap<Fut>(
        self: Arc<Self>,
        dap_name: String,
        handler: impl FnOnce(String, &Dap, Instance) -> Fut,
    ) -> HttpResponse
    where
        Fut: Future<Output = ServerResult<HttpResponse>>,
    {
        self.handle_allowed_dap(&[Permission::ClientHttp], dap_name, handler)
            .await
    }

    pub async fn handle_ws<Fut>(
        self: Arc<Self>,
        dap_name: String,
        handler: impl FnOnce(&mut DapsManager, String) -> Fut,
    ) -> HttpResponse
    where
        Fut: Future<Output = ServerResult<HttpResponse>>,
    {
        self.handle_allowed(&[Permission::ClientHttp, Permission::Websocket], dap_name, handler)
            .await
    }
}

impl Deref for DapsProvider {
    type Target = Mutex<DapsManager>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
