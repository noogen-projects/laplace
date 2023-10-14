use std::io;
use std::ops::Deref;
use std::string::FromUtf8Error;

use borsh::BorshDeserialize;
use laplace_wasm::route::{gossipsub, websocket, Route};
use laplace_wasm::{http, WasmSlice};
use thiserror::Error;
use wasmtime::{Instance, Store};
use wasmtime_wasi::preview2::preview1::{WasiPreview1Adapter, WasiPreview1View};
use wasmtime_wasi::preview2::{Table, WasiCtx, WasiView};

use crate::lapps::wasm_interop::database::DatabaseCtx;
use crate::lapps::wasm_interop::http::HttpCtx;
use crate::lapps::wasm_interop::{MemoryManagementError, MemoryManagementHostData};

#[derive(Debug, Error)]
pub enum LappInstanceError {
    #[error("Wasm function does not found: {0}")]
    WasmFunctionNotFound(String),

    #[error("Wasm error: {0}")]
    WasmError(#[from] anyhow::Error),

    #[error("Can't deserialize string: {0:?}")]
    DeserializeStringError(#[from] FromUtf8Error),

    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("Wrong memory operation: {0}")]
    MemoryManagementError(#[from] MemoryManagementError),
}

pub type LappInstanceResult<T> = Result<T, LappInstanceError>;

pub struct LappInstance {
    pub instance: Instance,
    pub memory_management: MemoryManagementHostData,
    pub store: Store<Ctx>,
}

impl LappInstance {
    pub async fn process_http(&mut self, request: http::Request) -> LappInstanceResult<http::Response> {
        let process_http_fn = self
            .instance
            .get_typed_func::<u64, u64>(&mut self.store, "process_http")?;

        let bytes = borsh::to_vec(&request)?;
        let arg = self.bytes_to_wasm_slice(&bytes).await?;

        let slice = process_http_fn.call_async(&mut self.store, arg.into()).await?;
        let bytes = self.wasm_slice_to_vec(slice).await?;

        Ok(BorshDeserialize::deserialize(&mut bytes.as_slice())?)
    }

    pub async fn route_ws(&mut self, msg: &websocket::Message) -> LappInstanceResult<Vec<Route>> {
        let route_ws_fn = self.instance.get_typed_func::<u64, u64>(&mut self.store, "route_ws")?;
        let arg = self.bytes_to_wasm_slice(&borsh::to_vec(&msg)?).await?;

        let response_slice = route_ws_fn.call_async(&mut self.store, arg.into()).await?;
        let bytes = self.wasm_slice_to_vec(response_slice).await?;

        Ok(BorshDeserialize::try_from_slice(&bytes)?)
    }

    pub async fn route_gossipsub(&mut self, msg: &gossipsub::Message) -> LappInstanceResult<Vec<Route>> {
        let route_gossipsub = self
            .instance
            .get_typed_func::<u64, u64>(&mut self.store, "route_gossipsub")?;
        let arg = self.bytes_to_wasm_slice(&borsh::to_vec(&msg)?).await?;

        let response_slice = route_gossipsub.call_async(&mut self.store, arg.into()).await?;
        let bytes = self.wasm_slice_to_vec(response_slice).await?;

        Ok(BorshDeserialize::try_from_slice(&bytes)?)
    }

    pub async fn copy_to_memory(&mut self, src_bytes: &[u8]) -> LappInstanceResult<u32> {
        Ok(self
            .memory_management
            .to_manager(&mut self.store)
            .copy_to_memory(src_bytes)
            .await?)
    }

    pub async fn move_from_memory(&mut self, offset: usize, size: usize) -> LappInstanceResult<Vec<u8>> {
        Ok(self
            .memory_management
            .to_manager(&mut self.store)
            .move_from_memory(offset, size)
            .await?)
    }

    pub async fn wasm_slice_to_vec(&mut self, slice: impl Into<WasmSlice>) -> LappInstanceResult<Vec<u8>> {
        Ok(self
            .memory_management
            .to_manager(&mut self.store)
            .wasm_slice_to_vec(slice)
            .await?)
    }

    pub async fn wasm_slice_to_string(&mut self, slice: impl Into<WasmSlice>) -> LappInstanceResult<String> {
        Ok(self
            .memory_management
            .to_manager(&mut self.store)
            .wasm_slice_to_string(slice)
            .await?)
    }

    pub async fn bytes_to_wasm_slice(&mut self, bytes: impl AsRef<[u8]>) -> LappInstanceResult<WasmSlice> {
        Ok(self
            .memory_management
            .to_manager(&mut self.store)
            .bytes_to_wasm_slice(bytes)
            .await?)
    }
}

impl Deref for LappInstance {
    type Target = Instance;

    fn deref(&self) -> &Self::Target {
        &self.instance
    }
}

pub struct Ctx {
    pub wasi: WasiCtx,
    pub table: Table,
    pub adapter: WasiPreview1Adapter,
    pub memory_data: Option<MemoryManagementHostData>,
    pub database: Option<DatabaseCtx>,
    pub http: Option<HttpCtx>,
}

impl Ctx {
    pub fn new(wasi: WasiCtx, table: Table) -> Self {
        Self {
            wasi,
            table,
            adapter: WasiPreview1Adapter::new(),
            memory_data: None,
            database: None,
            http: None,
        }
    }

    pub fn memory_data(&self) -> &MemoryManagementHostData {
        self.memory_data.as_ref().expect("Memory data is empty")
    }
}

impl WasiView for Ctx {
    fn table(&self) -> &Table {
        &self.table
    }

    fn table_mut(&mut self) -> &mut Table {
        &mut self.table
    }

    fn ctx(&self) -> &WasiCtx {
        &self.wasi
    }

    fn ctx_mut(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
}

impl WasiPreview1View for Ctx {
    fn adapter(&self) -> &WasiPreview1Adapter {
        &self.adapter
    }

    fn adapter_mut(&mut self) -> &mut WasiPreview1Adapter {
        &mut self.adapter
    }
}
