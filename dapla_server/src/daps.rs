use std::{
    fs, io,
    path::{Path, PathBuf},
};

use actix_files::{Files, NamedFile};
use actix_web::web;
pub use dapla_common::dap::access::*;
use dapla_common::dap::Dap as CommonDap;
use derive_more::From;
use log::error;
use serde::{Deserialize, Serialize};
use wasmer::{imports, CompileError, Instance, InstantiationError, Module, Store};

pub use self::{manager::*, service::*, settings::*};

pub mod handler;
mod manager;
mod service;
mod settings;

#[derive(Debug, From)]
pub enum DapError {
    NotFound(String),

    #[from]
    Io(io::Error),

    #[from]
    Compile(CompileError),

    #[from]
    Instantiation(InstantiationError),
}

pub type DapResult<T> = Result<T, DapError>;

#[derive(Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Dap(CommonDap<PathBuf>);

impl Dap {
    pub const STATIC_DIR_NAME: &'static str = "static";
    pub const INDEX_FILE_NAME: &'static str = "index.html";
    pub const SETTINGS_FILE_NAME: &'static str = "settings.toml";

    pub fn new(name: impl Into<String>, root_dir: impl Into<PathBuf>) -> Self {
        let mut dap = Self(CommonDap::new(name.into(), root_dir.into(), Default::default()));
        if let Err(err) = dap.reload_settings() {
            error!("Error when load settings for dap '{}': {:?}", dap.name(), err);
        }
        dap
    }

    pub fn reload_settings(&mut self) -> DapSettingsResult<()> {
        self.0
            .set_settings(DapSettings::load(self.root_dir().join(Self::SETTINGS_FILE_NAME))?);
        Ok(())
    }

    pub fn enabled(&self) -> bool {
        self.0.enabled()
    }

    pub fn title(&self) -> &str {
        self.0.title()
    }

    pub fn name(&self) -> &str {
        self.0.name()
    }

    pub fn root_dir(&self) -> &Path {
        self.0.root_dir()
    }

    pub fn root_uri(&self) -> String {
        format!("/{}", self.name())
    }

    pub fn static_uri(&self) -> String {
        format!("{}/{}", self.root_uri(), Self::STATIC_DIR_NAME)
    }

    pub fn static_dir(&self) -> PathBuf {
        self.root_dir().join(Self::STATIC_DIR_NAME)
    }

    pub fn index_file(&self) -> PathBuf {
        self.static_dir().join(Self::INDEX_FILE_NAME)
    }

    pub fn server_module_file(&self) -> PathBuf {
        self.root_dir().join(&format!("{}_server.wasm", self.name()))
    }

    pub fn is_main_client(&self) -> bool {
        self.name() == DapsManager::MAIN_CLIENT_APP_NAME
    }

    pub fn http_configure(&self) -> impl FnOnce(&mut web::ServiceConfig) + '_ {
        let name = self.name().to_string();
        let index_file = self.index_file();
        let root_uri = self.root_uri();
        let static_uri = self.static_uri();
        let static_dir = self.static_dir();
        let is_main_client = self.is_main_client();

        move |config| {
            config
                .route(
                    &root_uri,
                    web::get().to(move || {
                        let index_file = index_file.clone();
                        async { NamedFile::open(index_file) }
                    }),
                )
                .service(Files::new(&static_uri, static_dir).index_file(Dap::INDEX_FILE_NAME));

            if !is_main_client {
                config.service(web::scope(&root_uri).route(
                    "/*",
                    web::get().to(move |daps_manager, request| handler::get(daps_manager, request, name.clone())),
                ));
            }
        }
    }

    pub fn instantiate(&self) -> DapResult<Instance> {
        let wasm = fs::read(self.server_module_file())?;

        let store = Store::default();
        let module = Module::new(&store, &wasm)?;
        let import_object = imports! {};
        Instance::new(&module, &import_object).map_err(Into::into)
    }
}
