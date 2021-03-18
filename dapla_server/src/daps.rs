use std::{
    fs,
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use actix_files::Files;
use actix_web::web;
pub use dapla_common::{
    api::{UpdateQuery, UpdateRequest as DapUpdateRequest},
    dap::access::*,
};
use log::error;
use serde::{Deserialize, Serialize};
use wasmer::{imports, Instance, Module, Store};
use wasmer_wasi::WasiState;

pub use self::{instance::*, manager::*, service::*, settings::*};
use crate::error::ServerResult;

pub mod handler;
mod instance;
mod manager;
mod service;
mod settings;

type CommonDap = dapla_common::dap::Dap<PathBuf>;

pub type DapResponse<'a> = dapla_common::api::Response<'a, PathBuf>;

#[derive(Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Dap(CommonDap);

impl Dap {
    pub fn new(name: impl Into<String>, root_dir: impl Into<PathBuf>) -> Self {
        let mut dap = Self(CommonDap::new(name.into(), root_dir.into(), Default::default()));
        if !dap.is_main() {
            if let Err(err) = dap.reload_settings() {
                error!("Error when load settings for dap '{}': {:?}", dap.name(), err);
            }
        }
        dap
    }

    pub const fn settings_file_name() -> &'static str {
        "settings.toml"
    }

    pub const fn static_dir_name() -> &'static str {
        CommonDap::static_dir_name()
    }

    pub const fn index_file_name() -> &'static str {
        CommonDap::index_file_name()
    }

    pub const fn main_name() -> &'static str {
        CommonDap::main_name()
    }

    pub fn main_static_uri() -> String {
        CommonDap::main_static_uri()
    }

    pub fn main_uri(tail: impl AsRef<str>) -> String {
        CommonDap::main_uri(tail)
    }

    pub fn reload_settings(&mut self) -> DapSettingsResult<()> {
        self.0
            .set_settings(DapSettings::load(self.root_dir().join(Self::settings_file_name()))?);
        Ok(())
    }

    pub fn save_settings(&mut self) -> DapSettingsResult<()> {
        let path = self.root_dir().join(Self::settings_file_name());
        self.0.settings().save(path)
    }

    pub fn static_dir(&self) -> PathBuf {
        self.root_dir().join(Self::static_dir_name())
    }

    pub fn index_file(&self) -> PathBuf {
        self.static_dir().join(Self::index_file_name())
    }

    pub fn server_module_file(&self) -> PathBuf {
        self.root_dir().join(&format!("{}_server.wasm", self.name()))
    }

    pub fn http_configure(&self) -> impl FnOnce(&mut web::ServiceConfig) + '_ {
        let name = self.name().to_string();
        let root_uri = self.root_uri();
        let static_uri = self.static_uri();
        let static_dir = self.static_dir();
        let is_main_client = self.is_main();

        move |config| {
            config
                .route(
                    &root_uri,
                    web::get().to({
                        let name = name.clone();
                        move |daps_service, request| handler::index_file(daps_service, request, name.clone())
                    }),
                )
                .service(Files::new(&static_uri, static_dir).index_file(Self::index_file_name()));

            if !is_main_client {
                config.service(
                    web::scope(&root_uri)
                        .route(
                            "/*",
                            web::get().to({
                                let name = name.clone();
                                move |daps_service, request| handler::get(daps_service, request, name.clone())
                            }),
                        )
                        .route(
                            "/*",
                            web::post().to(move |daps_service, request, body| {
                                handler::post(daps_service, request, body, name.clone())
                            }),
                        ),
                );
            }
        }
    }

    pub fn instantiate(&self) -> ServerResult<Instance> {
        let wasm = fs::read(self.server_module_file())?;

        let store = Store::default();
        let module = Module::new(&store, &wasm)?;

        let is_allow_read = self.is_allowed_permission(Permission::FileRead);
        let is_allow_write = self.is_allowed_permission(Permission::FileWrite);

        let dir_path = self.root_dir().join("data");
        if !dir_path.exists() && (is_allow_read || is_allow_write) {
            fs::create_dir(&dir_path)?;
        }

        let import_object = if self
            .required_permissions()
            .any(|permission| permission == Permission::FileRead || permission == Permission::FileWrite)
        {
            let mut wasi_env = WasiState::new(self.name())
                .preopen(|preopen| {
                    preopen
                        .directory(&dir_path)
                        .alias("/")
                        .read(is_allow_read)
                        .write(is_allow_write)
                        // todo: why this works always as true?
                        .create(is_allow_write)
                })?
                .finalize()?;

            wasi_env.import_object(&module)?
        } else {
            imports! {}
        };

        let instance = Instance::new(&module, &import_object)?;
        if let Ok(init) = instance.exports.get_function("_initialize") {
            init.call(&[])?;
        }

        Ok(instance)
    }

    pub fn update(&mut self, mut query: UpdateQuery) -> DapSettingsResult<UpdateQuery> {
        if let Some(enabled) = query.enabled {
            if self.enabled() != enabled {
                self.set_enabled(enabled);
            } else {
                query.enabled = None;
            }
        }

        if let Some(permission) = query.allow_permission {
            if !self.allow_permission(permission) {
                query.allow_permission = None;
            }
        }

        if let Some(permission) = query.deny_permission {
            if !self.deny_permission(permission) {
                query.deny_permission = None;
            }
        }

        self.save_settings()?;
        Ok(query)
    }
}

impl Deref for Dap {
    type Target = CommonDap;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Dap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
