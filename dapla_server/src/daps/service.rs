use std::{
    io,
    ops::Deref,
    path::Path,
    sync::{Arc, Mutex},
};

use actix_web::HttpResponse;
use log::error;

use crate::{daps::DapsManager, ServerError};

#[derive(Clone)]
pub struct DapsService(Arc<Mutex<DapsManager>>);

impl DapsService {
    pub fn new(daps_path: impl AsRef<Path>) -> io::Result<Self> {
        DapsManager::new(daps_path).map(|manager| Self(Arc::new(Mutex::new(manager))))
    }

    pub fn handle_http(&self, handler: impl FnOnce(&mut DapsManager) -> HttpResponse) -> HttpResponse {
        self.lock()
            .map(|mut daps_manager| handler(&mut daps_manager))
            .unwrap_or_else(|err| {
                error!("Daps service lock should be asquired: {:?}", err);
                ServerError::DapsServiceNotLock.into_http_response()
            })
    }
}

impl Deref for DapsService {
    type Target = Mutex<DapsManager>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
