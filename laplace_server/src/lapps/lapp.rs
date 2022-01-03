pub use laplace_common::{
    api::{UpdateQuery, UpdateRequest as LappUpdateRequest},
    lapp::access::*,
};

use std::{
    convert::TryFrom,
    fs,
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{Arc, Mutex, RwLockReadGuard},
};

use actix_files::Files;
use actix_web::web;
use arc_swap::ArcSwapOption;
use borsh::BorshDeserialize;
use log::error;
use rusqlite::Connection;
use serde::{Serialize, Serializer};
use wasmer::{Exports, Function, ImportObject, Instance, Module, Store};
use wasmer_wasi::WasiState;

use crate::{
    error::{ServerError, ServerResult},
    lapps::{
        handler,
        import::{
            database::{self, DatabaseEnv},
            http::{self, HttpEnv},
            sleep,
        },
        settings::{FileSettings, LappSettings, LappSettingsResult},
        ExpectedInstance,
    },
    service,
};

pub type CommonLapp = laplace_common::lapp::Lapp<PathBuf>;
pub type CommonLappResponse<'a> = laplace_common::api::Response<'a, PathBuf, CommonLappGuard<'a>>;

#[derive(Debug)]
pub struct CommonLappGuard<'a>(pub RwLockReadGuard<'a, Lapp>);

impl Deref for CommonLappGuard<'_> {
    type Target = CommonLapp;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl Serialize for CommonLappGuard<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.lapp.serialize(serializer)
    }
}

#[derive(Debug, Clone)]
pub struct Lapp {
    lapp: CommonLapp,
    instance: Option<Instance>,
    service_sender: Option<service::lapp::Sender>,
}

impl Lapp {
    pub fn new(name: impl Into<String>, root_dir: impl Into<PathBuf>) -> Self {
        let mut lapp = Self {
            lapp: CommonLapp::new(name.into(), root_dir.into(), Default::default()),
            instance: None,
            service_sender: None,
        };
        if !lapp.is_main() {
            if let Err(err) = lapp.reload_settings() {
                error!("Error when load settings for lapp '{}': {:?}", lapp.name(), err);
            }
        }
        lapp
    }

    pub const fn settings_file_name() -> &'static str {
        "settings.toml"
    }

    pub const fn static_dir_name() -> &'static str {
        CommonLapp::static_dir_name()
    }

    pub const fn index_file_name() -> &'static str {
        CommonLapp::index_file_name()
    }

    pub const fn main_name() -> &'static str {
        CommonLapp::main_name()
    }

    pub fn main_static_uri() -> String {
        CommonLapp::main_static_uri()
    }

    pub fn main_uri(tail: impl AsRef<str>) -> String {
        CommonLapp::main_uri(tail)
    }

    pub fn reload_settings(&mut self) -> LappSettingsResult<()> {
        self.lapp
            .set_settings(LappSettings::load(self.root_dir().join(Self::settings_file_name()))?);
        Ok(())
    }

    pub fn save_settings(&mut self) -> LappSettingsResult<()> {
        let path = self.root_dir().join(Self::settings_file_name());
        self.settings().save(path)
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

    pub fn is_loaded(&self) -> bool {
        self.instance.is_some()
    }

    pub fn instance(&self) -> Option<Instance> {
        self.instance.clone()
    }

    pub fn take_instance(&mut self) -> Option<Instance> {
        self.instance.take()
    }

    pub fn service_sender(&self) -> Option<service::lapp::Sender> {
        self.service_sender.clone()
    }

    pub fn run_service_if_needed(&mut self) -> ServerResult<service::lapp::Sender> {
        if let Some(sender) = self.service_sender() {
            Ok(sender)
        } else {
            let instance = self
                .instance()
                .ok_or_else(|| ServerError::LappNotLoaded(self.name().to_string()))?;

            let (service, sender) = service::LappService::new(ExpectedInstance::try_from(instance)?);
            actix::spawn(service.run());

            self.service_sender = Some(sender.clone());
            Ok(sender)
        }
    }

    pub async fn service_stop(&mut self) -> bool {
        if let Some(sender) = self.service_sender.take() {
            service::LappService::stop(sender).await
        } else {
            false
        }
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
                        move |lapps_service, request| handler::index_file(lapps_service, request, name.clone())
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
                                move |lapps_service, request, stream| {
                                    handler::ws_start(lapps_service, request, stream, name.clone())
                                }
                            }),
                        )
                        .route(
                            "/p2p",
                            web::post().to({
                                let name = name.clone();
                                move |lapps_service, request| {
                                    handler::gossipsub_start(lapps_service, request, name.clone())
                                }
                            }),
                        )
                        .route(
                            "/{tail}*",
                            web::route().to(move |lapps_service, request, body| {
                                handler::http(lapps_service, request, body, name.clone())
                            }),
                        ),
                );
            }
        }
    }

    pub fn instantiate(&mut self, http_client: reqwest::blocking::Client) -> ServerResult<()> {
        let wasm = fs::read(self.server_module_file())?;

        let store = Store::default();
        let module = Module::new(&store, &wasm)?;

        let is_allow_read = self.is_allowed_permission(Permission::FileRead);
        let is_allow_write = self.is_allowed_permission(Permission::FileWrite);
        let is_allow_db_access = self.is_allowed_permission(Permission::Database);
        let is_allow_http = self.is_allowed_permission(Permission::Http);
        let is_allow_sleep = self.is_allowed_permission(Permission::Sleep);

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
        let mut exports = Exports::new();

        if is_allow_db_access {
            let database_path = self.settings().database().path();
            let connection = Arc::new(Mutex::new(Connection::open(database_path)?));

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

            exports.insert("db_execute", execute_native);
            exports.insert("db_query", query_native);
            exports.insert("db_query_row", query_row_native);
        }

        if is_allow_http {
            let invoke_http_native = Function::new_native_with_env(
                &store,
                HttpEnv {
                    instance: shared_instance.clone(),
                    client: http_client,
                    settings: self.lapp.settings().network().http().clone(),
                },
                http::invoke_http,
            );

            exports.insert("invoke_http", invoke_http_native);
        }

        if is_allow_sleep {
            let invoke_sleep_native = Function::new_native(&store, sleep::invoke_sleep);

            exports.insert("invoke_sleep", invoke_sleep_native);
        }

        import_object.register("env", exports);

        let instance = Instance::new(&module, &import_object)?;
        shared_instance.store(Some(Arc::new(instance.clone())));

        if let Ok(initialize) = instance.exports.get_function("_initialize") {
            initialize.call(&[])?;
        }

        if let Ok(init) = instance.exports.get_function("init") {
            let slice = init.native::<(), u64>()?.call()?;
            let instance = ExpectedInstance::try_from(&instance)?;
            let bytes = unsafe { instance.wasm_slice_to_vec(slice)? };
            Result::<(), String>::try_from_slice(&bytes)?.map_err(ServerError::LappInitError)?;
        }

        self.instance.replace(instance);
        Ok(())
    }

    pub fn update(&mut self, mut query: UpdateQuery) -> LappSettingsResult<UpdateQuery> {
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

    pub fn check_enabled_and_allow_permissions(&self, permissions: &[Permission]) -> ServerResult<()> {
        if !self.enabled() {
            return Err(ServerError::LappNotEnabled(self.name().into()));
        };
        for &permission in permissions {
            if !self.is_allowed_permission(permission) {
                return Err(ServerError::LappPermissionDenied(self.name().into(), permission));
            }
        }
        Ok(())
    }
}

impl Deref for Lapp {
    type Target = CommonLapp;

    fn deref(&self) -> &Self::Target {
        &self.lapp
    }
}

impl DerefMut for Lapp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.lapp
    }
}
