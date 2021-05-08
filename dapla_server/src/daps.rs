use std::{
    convert::TryFrom,
    fs,
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use actix_files::Files;
use actix_web::web;
use arc_swap::ArcSwapOption;
use borsh::BorshDeserialize;
pub use dapla_common::{
    api::{UpdateQuery, UpdateRequest as DapUpdateRequest},
    dap::access::*,
};
use log::error;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use wasmer::{import_namespace, Function, ImportObject, Instance, Module, Store};
use wasmer_wasi::WasiState;

pub use self::{instance::*, manager::*, service::*, settings::*};
use crate::{
    daps::import::database::{self, DatabaseEnv},
    error::{ServerError, ServerResult},
};

pub mod handler;
mod import;
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
                            "/ws",
                            web::get().to({
                                let name = name.clone();
                                move |daps_service, request, stream| {
                                    handler::ws_start(daps_service, request, stream, name.clone())
                                }
                            }),
                        )
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
        let is_allow_db_access = self.is_allowed_permission(Permission::Database);

        let dir_path = self.root_dir().join("data");
        if !dir_path.exists() && (is_allow_read || is_allow_write) {
            fs::create_dir(&dir_path)?;
        }

        let mut import_object = if self
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
            ImportObject::new()
        };

        let shared_instance = Arc::new(ArcSwapOption::from(None));
        if is_allow_db_access {
            let connection = Arc::new(Mutex::new(Connection::open(&self.settings().database.path)?));

            let execute_native = Function::new_native_with_env(
                &store,
                DatabaseEnv {
                    instance: shared_instance.clone(),
                    connection: connection.clone(),
                },
                database::execute,
            );
            let query_native = Function::new_native_with_env(
                &store,
                DatabaseEnv {
                    instance: shared_instance.clone(),
                    connection: connection.clone(),
                },
                database::query,
            );
            let query_row_native = Function::new_native_with_env(
                &store,
                DatabaseEnv {
                    instance: shared_instance.clone(),
                    connection,
                },
                database::query_row,
            );
            import_object.register(
                "env",
                import_namespace!({
                    "db_execute" => execute_native,
                    "db_query" => query_native,
                    "db_query_row" => query_row_native,
                }),
            );
        }

        let instance = Instance::new(&module, &import_object)?;
        shared_instance.store(Some(Arc::new(instance.clone())));

        if let Ok(initialize) = instance.exports.get_function("_initialize") {
            initialize.call(&[])?;
        }

        if let Ok(init) = instance.exports.get_function("init") {
            let slice = init.native::<(), u64>()?.call()?;
            let instance = ExpectedInstance::try_from(&instance)?;
            let bytes = unsafe { instance.wasm_slice_to_vec(slice)? };
            Result::<(), String>::try_from_slice(&bytes)?.map_err(ServerError::DapInitError)?;
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
