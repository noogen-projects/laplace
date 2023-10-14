use std::fs;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use borsh::BorshDeserialize;
use cap_std::fs::Dir;
use derive_more::{Deref, DerefMut};
pub use laplace_common::api::{UpdateQuery, UpdateRequest as LappUpdateRequest};
pub use laplace_common::lapp::access::*;
use laplace_wasm::http::{Request, Response};
use reqwest::Client;
use rusqlite::Connection;
use serde::{Serialize, Serializer};
use tokio::sync::{RwLock, RwLockReadGuard};
use truba::{Context, Sender};
use wasmtime::{Config, Engine, Linker, Module, Store};
use wasmtime_wasi::preview2::preview1::add_to_linker_async;
use wasmtime_wasi::preview2::{DirPerms, FilePerms, Table, WasiCtxBuilder};

use crate::error::{ServerError, ServerResult};
use crate::lapps::settings::{FileSettings, LappSettings, LappSettingsResult};
use crate::lapps::wasm_interop::database::DatabaseCtx;
use crate::lapps::wasm_interop::http::HttpCtx;
use crate::lapps::wasm_interop::{database, http, sleep, MemoryManagementHostData};
use crate::lapps::{Ctx, LappInstance, LappInstanceError};
use crate::service;
use crate::service::lapp::LappServiceMessage;
use crate::service::Addr;

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
pub type CommonLappResponse<'a> = laplace_common::api::Response<'a, PathBuf, CommonLappGuard<'a>>;

pub struct CommonLappGuard<'a>(pub RwLockReadGuard<'a, CommonLapp>);

impl<'a> Deref for CommonLappGuard<'a> {
    type Target = CommonLapp;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> From<RwLockReadGuard<'a, Lapp>> for CommonLappGuard<'a> {
    fn from(lapp: RwLockReadGuard<'a, Lapp>) -> Self {
        Self(RwLockReadGuard::map(lapp, |inner| &inner.lapp))
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

#[derive(Deref, DerefMut)]
pub struct Lapp {
    #[deref]
    #[deref_mut]
    lapp: CommonLapp,
    instance: Option<LappInstance>,
}

impl Lapp {
    pub fn new(name: impl Into<String>, root_dir: impl Into<PathBuf>) -> Self {
        let mut lapp = Self {
            lapp: CommonLapp::new(name.into(), root_dir.into(), Default::default()),
            instance: None,
        };
        if !lapp.is_main() {
            if let Err(err) = lapp.reload_settings() {
                log::error!("Error when load config for lapp '{}': {err:?}", lapp.name());
            }
        }
        lapp
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

    pub fn main_static_uri() -> String {
        CommonLapp::main_static_uri()
    }

    pub fn main_uri(tail: impl AsRef<str>) -> String {
        CommonLapp::main_uri(tail)
    }

    pub fn reload_settings(&mut self) -> LappSettingsResult<()> {
        self.lapp
            .set_settings(LappSettings::load(self.root_dir().join(Self::config_file_name()))?);
        Ok(())
    }

    pub fn save_settings(&mut self) -> LappSettingsResult<()> {
        let path = self.root_dir().join(Self::config_file_name());
        self.settings().save(path)
    }

    pub fn static_dir(&self) -> PathBuf {
        self.root_dir().join(Self::static_dir_name())
    }

    pub fn index_file(&self) -> PathBuf {
        self.static_dir().join(Self::index_file_name())
    }

    pub fn server_module_file(&self) -> PathBuf {
        self.root_dir().join(format!("{}_server.wasm", self.name()))
    }

    pub fn is_loaded(&self) -> bool {
        self.instance.is_some()
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

    pub fn service_stop(&mut self, ctx: &Context<Addr>) -> bool {
        if let Some(sender) = ctx.get_actor_sender::<LappServiceMessage>(&Addr::Lapp(self.lapp.name().to_string())) {
            sender.send(LappServiceMessage::Stop).is_ok()
        } else {
            false
        }
    }

    pub async fn instantiate(&mut self, http_client: Client) -> ServerResult<()> {
        let wasm_bytes = fs::read(self.server_module_file())?;
        let module = Module::new(&ENGINE, wasm_bytes)?;

        let mut linker = Linker::new(&ENGINE);
        add_to_linker_async(&mut linker)?;

        let is_allow_read = self.is_allowed_permission(Permission::FileRead);
        let is_allow_write = self.is_allowed_permission(Permission::FileWrite);
        let is_allow_db_access = self.is_allowed_permission(Permission::Database);
        let is_allow_http = self.is_allowed_permission(Permission::Http);
        let is_allow_sleep = self.is_allowed_permission(Permission::Sleep);

        let dir_path = self.root_dir().join("data");
        if !dir_path.exists() && (is_allow_read || is_allow_write) {
            fs::create_dir(&dir_path)?;
        }

        let mut wasi = WasiCtxBuilder::new();
        wasi.inherit_stdout();

        if self
            .required_permissions()
            .any(|permission| permission == Permission::FileRead || permission == Permission::FileWrite)
        {
            let preopened_dir = Dir::open_ambient_dir(&dir_path, cap_std::ambient_authority())?;
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
        let table = Table::new();
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

    fn get_database_path(&self) -> PathBuf {
        let database_path = self.settings().database().path();

        if database_path.is_relative() {
            self.root_dir().join(database_path)
        } else {
            database_path.into()
        }
    }
}

#[derive(Clone, Deref)]
pub struct SharedLapp {
    name: String,
    #[deref]
    inner: Arc<RwLock<Lapp>>,
}

impl SharedLapp {
    pub fn new(lapp: Lapp) -> Self {
        Self {
            name: lapp.name().to_owned(),
            inner: Arc::new(RwLock::new(lapp)),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn run_service_if_needed(&self, ctx: &Context<Addr>) -> Sender<LappServiceMessage> {
        let service_actor_id = Addr::Lapp(self.name.clone());

        ctx.get_actor_sender::<LappServiceMessage>(&service_actor_id)
            .unwrap_or_else(|| {
                let sender = ctx.actor_sender::<LappServiceMessage>(service_actor_id);
                service::LappService::new(self.clone()).run(ctx.clone());
                sender
            })
    }
}
