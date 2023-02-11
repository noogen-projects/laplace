use std::io;
use std::ops::Deref;
use std::string::FromUtf8Error;

use borsh::{BorshDeserialize, BorshSerialize};
use laplace_wasm::route::{gossipsub, websocket, Route};
use laplace_wasm::{http, WasmSlice};
use thiserror::Error;
use wasmer::{Instance, Store};

use crate::lapps::wasm_interop::{MemoryManagementError, MemoryManagementHostData};

#[derive(Debug, Error)]
pub enum LappInstanceError {
    #[error("Get instance export: {0}")]
    ExportError(#[from] wasmer::ExportError),

    #[error("Execution abort: {0}")]
    RuntimeError(#[from] wasmer::RuntimeError),

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
    pub store: Store,
}

impl LappInstance {
    pub fn process_http(&mut self, request: http::Request) -> LappInstanceResult<http::Response> {
        let process_http_fn = self
            .instance
            .exports
            .get_typed_function::<u64, u64>(&self.store, "process_http")?;

        let bytes = request.try_to_vec()?;
        let arg = self.bytes_to_wasm_slice(&bytes)?;

        let slice = process_http_fn.call(&mut self.store, arg.into())?;
        let bytes = unsafe { self.wasm_slice_to_vec(slice)? };

        Ok(BorshDeserialize::deserialize(&mut bytes.as_slice())?)
    }

    pub fn route_ws(&mut self, msg: &websocket::Message) -> LappInstanceResult<Vec<Route>> {
        let route_ws_fn = self.exports.get_function("route_ws")?.typed::<u64, u64>(&self.store)?;
        let arg = self.bytes_to_wasm_slice(&msg.try_to_vec()?)?;

        let response_slice = route_ws_fn.call(&mut self.store, arg.into())?;
        let bytes = unsafe { self.wasm_slice_to_vec(response_slice)? };

        Ok(BorshDeserialize::try_from_slice(&bytes)?)
    }

    pub fn route_gossipsub(&mut self, msg: &gossipsub::Message) -> LappInstanceResult<Vec<Route>> {
        let route_gossipsub = self
            .exports
            .get_function("route_gossipsub")?
            .typed::<u64, u64>(&self.store)?;
        let arg = self.bytes_to_wasm_slice(&msg.try_to_vec()?)?;

        let response_slice = route_gossipsub.call(&mut self.store, arg.into())?;
        let bytes = unsafe { self.wasm_slice_to_vec(response_slice)? };

        Ok(BorshDeserialize::try_from_slice(&bytes)?)
    }

    pub fn copy_to_memory(&mut self, src: *const u8, size: usize) -> LappInstanceResult<u32> {
        Ok(self
            .memory_management
            .to_manager(&mut self.store)
            .copy_to_memory(src, size)?)
    }

    pub unsafe fn move_from_memory(&mut self, offset: u32, size: usize) -> LappInstanceResult<Vec<u8>> {
        Ok(self
            .memory_management
            .to_manager(&mut self.store)
            .move_from_memory(offset, size)?)
    }

    pub unsafe fn wasm_slice_to_vec(&mut self, slice: impl Into<WasmSlice>) -> LappInstanceResult<Vec<u8>> {
        Ok(self
            .memory_management
            .to_manager(&mut self.store)
            .wasm_slice_to_vec(slice)?)
    }

    pub unsafe fn wasm_slice_to_string(&mut self, slice: impl Into<WasmSlice>) -> LappInstanceResult<String> {
        Ok(self
            .memory_management
            .to_manager(&mut self.store)
            .wasm_slice_to_string(slice)?)
    }

    pub fn bytes_to_wasm_slice(&mut self, bytes: impl AsRef<[u8]>) -> LappInstanceResult<WasmSlice> {
        Ok(self
            .memory_management
            .to_manager(&mut self.store)
            .bytes_to_wasm_slice(bytes)?)
    }
}

impl Deref for LappInstance {
    type Target = Instance;

    fn deref(&self) -> &Self::Target {
        &self.instance
    }
}

// impl TryFrom<Instance> for LappInstance {
//     type Error = wasmer::ExportError;
//
//     fn try_from(instance: Instance) -> Result<Self, Self::Error> {
//         let memory = instance.exports.get_memory("memory")?.clone();
//
//         Ok(Self { instance, memory })
//     }
// }
//
// impl TryFrom<&Instance> for LappInstance {
//     type Error = wasmer::ExportError;
//
//     fn try_from(instance: &Instance) -> Result<Self, Self::Error> {
//         Self::try_from(instance.clone())
//     }
// }
//
// impl TryFrom<Arc<Instance>> for LappInstance {
//     type Error = wasmer::ExportError;
//
//     fn try_from(instance: Arc<Instance>) -> Result<Self, Self::Error> {
//         Self::try_from(&*instance)
//     }
// }
