use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use borsh::BorshDeserialize;
use cap_std::fs::Dir;
use derive_more::{Deref, DerefMut};
pub use laplace_common::api::{UpdateQuery, UpdateRequest as LappUpdateRequest};
pub use laplace_common::lapp::access::*;
use laplace_wasm::http::{Request, Response};
use reqwest::Client;
use rusqlite::Connection;
use serde::{Serialize, Serializer};
use wasmtime::component::ResourceTable;
use wasmtime::{Config, Engine, Linker, Module, Store};
use wasmtime_wasi::preview1::add_to_linker_async;
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

use crate::error::{ServerError, ServerResult};
use crate::lapps::settings::{FileSettings, LappSettings, LappSettingsResult};
use crate::lapps::wasm_interop::database::DatabaseCtx;
use crate::lapps::wasm_interop::http::HttpCtx;
use crate::lapps::wasm_interop::{database, http, sleep, MemoryManagementHostData};
use crate::lapps::{Ctx, LappInstance, LappInstanceError};

lazy_static::lazy_static! {
    static ref ENGINE: Engine = {
        let mut config = Config::new();
        config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);
        config.wasm_component_model(true);
        config.async_support(true);

        Engine::new(&config).expect("Failed create engine")
    };
}

pub type CommonLapp = laplace_common::lapp::Lapp<PathBuf>;
pub type CommonLappResponse<'a> = laplace_common::api::Response<'a, CommonLappGuard<'a>>;

pub struct CommonLappGuard<'a>(pub &'a LappSettings);

impl<'a> Deref for CommonLappGuard<'a> {
    type Target = LappSettings;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl Serialize for CommonLappGuard<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

pub struct LappDir(pub PathBuf);

impl LappDir {
    pub fn root_dir(&self) -> &Path {
        &self.0
    }

    pub fn static_dir(&self) -> PathBuf {
        self.root_dir().join(Lapp::static_dir_name())
    }

    pub fn index_file(&self) -> PathBuf {
        self.static_dir().join(Lapp::index_file_name())
    }
}

impl Deref for LappDir {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<Path> for LappDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl From<LappDir> for PathBuf {
    fn from(dir: LappDir) -> Self {
        dir.0
    }
}

#[derive(Deref, DerefMut)]
pub struct Lapp {
    #[deref]
    #[deref_mut]
    lapp: CommonLapp,
    instance: Option<LappInstance>,
}

impl Lapp {
    pub fn new(name: impl Into<String>, root_dir: impl Into<PathBuf>, settings: LappSettings) -> Self {
        Self {
            lapp: CommonLapp::new(name.into(), root_dir.into(), settings),
            instance: None,
        }
    }

    pub const fn config_file_name() -> &'static str {
        "config.toml"
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

    pub fn is_main(name: impl AsRef<str>) -> bool {
        CommonLapp::is_main(name)
    }

    pub fn main_static_uri() -> String {
        CommonLapp::main_static_uri()
    }

    pub fn main_uri(tail: impl AsRef<str>) -> String {
        CommonLapp::main_uri(tail)
    }

    pub fn settings_path(lapp_path: impl AsRef<Path>) -> PathBuf {
        lapp_path.as_ref().join(Self::config_file_name())
    }

    pub fn load_settings(lapp_name: impl AsRef<str>, lapp_path: impl AsRef<Path>) -> Option<LappSettings> {
        let lapp_name = lapp_name.as_ref();

        if !Lapp::is_main(lapp_name) {
            LappSettings::load(lapp_name, Self::settings_path(lapp_path))
                .map_err(|err| log::error!("Error when load config for lapp '{lapp_name}': {err:?}"))
                .ok()
        } else {
            None
        }
    }

    pub fn save_settings(&mut self) -> LappSettingsResult<()> {
        self.settings().save(Self::settings_path(self.root_dir()))
    }

    pub fn instance_mut(&mut self) -> Option<&mut LappInstance> {
        self.instance.as_mut()
    }

    pub fn take_instance(&mut self) -> Option<LappInstance> {
        self.instance.take()
    }

    pub async fn process_http(&mut self, request: Request) -> ServerResult<Response> {
        match self.instance_mut() {
            Some(instance) => Ok(instance.process_http(request).await?),
            None => Err(ServerError::LappNotLoaded(self.name().to_string())),
        }
    }

    pub fn server_module_file(&self) -> PathBuf {
        self.root_dir().join(format!("{}_server.wasm", self.name()))
    }

    pub async fn instantiate(&mut self, http_client: Client) -> ServerResult<()> {
        let wasm_bytes = fs::read(self.server_module_file())?;
        let module = Module::new(&ENGINE, wasm_bytes)?;

        let mut linker = Linker::new(&ENGINE);
        add_to_linker_async(&mut linker, |ctx| ctx)?;

        let is_allow_read = self.is_allowed_permission(Permission::FileRead);
        let is_allow_write = self.is_allowed_permission(Permission::FileWrite);
        let is_allow_db_access = self.is_allowed_permission(Permission::Database);
        let is_allow_http = self.is_allowed_permission(Permission::Http);
        let is_allow_sleep = self.is_allowed_permission(Permission::Sleep);

        let data_dir_path = if self.data_dir().is_absolute() {
            self.data_dir().to_owned()
        } else {
            self.root_dir().join(self.data_dir())
        };
        if !data_dir_path.exists() && (is_allow_read || is_allow_write) {
            fs::create_dir(&data_dir_path)?;
        }

        let mut wasi = WasiCtxBuilder::new();
        wasi.inherit_stdout();

        if self
            .settings()
            .permissions
            .required()
            .any(|permission| permission == Permission::FileRead || permission == Permission::FileWrite)
        {
            let preopened_dir = Dir::open_ambient_dir(&data_dir_path, cap_std::ambient_authority())?;
            let mut perms = DirPerms::empty();
            let mut file_perms = FilePerms::empty();

            if is_allow_read {
                perms |= DirPerms::READ;
                file_perms |= FilePerms::READ;
            }

            if is_allow_write {
                perms |= DirPerms::MUTATE;
                file_perms |= FilePerms::WRITE;
            }

            wasi.preopened_dir(preopened_dir, perms, file_perms, "/");
        }

        let wasi = wasi.build();
        let table = ResourceTable::new();
        let ctx = Ctx::new(wasi, table);
        let mut store = Store::new(&ENGINE, ctx);

        if is_allow_db_access {
            let database_path = self.get_database_path();
            let connection = Connection::open(database_path)?;

            store.data_mut().database = Some(DatabaseCtx::new(connection));
            linker.func_wrap1_async("env", "db_execute", database::execute)?;
            linker.func_wrap1_async("env", "db_query", database::query)?;
            linker.func_wrap1_async("env", "db_query_row", database::query_row)?;
        }

        if is_allow_http {
            store.data_mut().http = Some(HttpCtx::new(http_client, self.lapp.settings().network().http().clone()));
            linker.func_wrap1_async("env", "invoke_http", http::invoke_http)?;
        }

        if is_allow_sleep {
            linker.func_wrap1_async("env", "invoke_sleep", sleep::invoke_sleep)?;
        }

        let instance = linker.instantiate_async(&mut store, &module).await?;
        let memory_management = MemoryManagementHostData::from_instance(&instance, &mut store)?;
        store.data_mut().memory_data = Some(memory_management.clone());

        if let Some(initialize) = instance.get_func(&mut store, "_initialize") {
            initialize.call_async(&mut store, &[], &mut Vec::new()).await?;
        }

        if let Some(start) = instance.get_func(&mut store, "_start") {
            start.call_async(&mut store, &[], &mut Vec::new()).await?;
        }

        if let Some(init) = instance.get_func(&mut store, "init") {
            let slice = init.typed::<(), u64>(&store)?.call_async(&mut store, ()).await?;
            let mut memory_manager = memory_management.to_manager(&mut store);
            let bytes = memory_manager
                .wasm_slice_to_vec(slice)
                .await
                .map_err(LappInstanceError::MemoryManagementError)?;
            Result::<(), String>::try_from_slice(&bytes)?.map_err(ServerError::LappInitError)?;
        }

        self.instance.replace(LappInstance {
            instance,
            memory_management,
            store,
        });
        Ok(())
    }

    fn get_database_path(&self) -> PathBuf {
        let database_path = self.settings().database().path();

        if database_path.is_relative() {
            self.root_dir().join(database_path)
        } else {
            database_path.into()
        }
    }
}
