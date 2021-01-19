use std::{
    io,
    ops::Deref,
    path::Path,
    sync::{Arc, Mutex},
};

use super::DapsManager;

#[derive(Clone)]
pub struct DapsService(Arc<Mutex<DapsManager>>);

impl DapsService {
    pub fn new(daps_path: impl AsRef<Path>) -> io::Result<Self> {
        DapsManager::new(daps_path).map(|manager| Self(Arc::new(Mutex::new(manager))))
    }
}

impl Deref for DapsService {
    type Target = Mutex<DapsManager>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
