use std::{
    io,
    ops::Deref,
    path::Path,
    sync::{Arc, Mutex},
};

use actix_web::HttpResponse;
use log::error;

use crate::{
    daps::DapsManager,
    error::{ServerError, ServerResult},
};

#[derive(Clone)]
pub struct DapsService(Arc<Mutex<DapsManager>>);

impl DapsService {
    pub fn new(daps_path: impl AsRef<Path>) -> io::Result<Self> {
        DapsManager::new(daps_path).map(|manager| Self(Arc::new(Mutex::new(manager))))
    }

    pub async fn handle_http(
        self: Arc<Self>,
        handler: impl FnOnce(&mut DapsManager) -> ServerResult<HttpResponse>,
    ) -> HttpResponse {
        self.lock()
            .map_err(|err| {
                error!("Daps service lock should be asquired: {:?}", err);
                ServerError::DapsServiceNotLock
            })
            .and_then(|mut daps_manager| handler(&mut daps_manager))
            .into()
    }

    pub async fn handle_http_dap(
        self: Arc<Self>,
        dap_name: String,
        handler: impl FnOnce(&mut DapsManager, String) -> ServerResult<HttpResponse>,
    ) -> HttpResponse {
        self.handle_http(move |daps_manager| {
            let dap = daps_manager.dap(&dap_name)?;
            if !dap.enabled() {
                Err(ServerError::DapNotEnabled(dap_name))
            } else if !daps_manager.is_loaded(&dap_name) {
                Err(ServerError::DapNotLoaded(dap_name))
            } else {
                handler(daps_manager, dap_name)
            }
        })
        .await
    }
}

impl Deref for DapsService {
    type Target = Mutex<DapsManager>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
