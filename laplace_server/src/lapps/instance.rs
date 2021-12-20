use std::{convert::TryFrom, ops::Deref, ptr::copy_nonoverlapping, slice, string::FromUtf8Error, sync::Arc};

use laplace_wasm::WasmSlice;
use thiserror::Error;
use wasmer::{Instance, Memory};

#[derive(Debug, Error)]
pub enum LappInstanceError {
    #[error("Get instance export: {0}")]
    ExportError(#[from] wasmer::ExportError),

    #[error("Execution abort: {0}")]
    RuntimeError(#[from] wasmer::RuntimeError),

    #[error("Can't deserialize string: {0:?}")]
    DeserializeStringError(#[from] FromUtf8Error),

    #[error("Incorrect WASM slice length")]
    WrongBufferLength,
}

pub type LappInstanceResult<T> = Result<T, LappInstanceError>;

pub trait Allocation {
    fn alloc(&self, size: u32) -> LappInstanceResult<u32>;
    unsafe fn dealloc(&self, offset: u32, size: u32) -> LappInstanceResult<()>;
}

impl Allocation for Instance {
    fn alloc(&self, size: u32) -> LappInstanceResult<u32> {
        let alloc_fn = self.exports.get_function("alloc")?.native::<u32, u32>()?;
        alloc_fn.call(size).map_err(Into::into)
    }

    unsafe fn dealloc(&self, offset: u32, size: u32) -> LappInstanceResult<()> {
        log::trace!("Dealloc: offset = {}, size = {}", offset, size);
        let dealloc_fn = self.exports.get_function("dealloc")?.native::<(u32, u32), ()>()?;
        dealloc_fn.call(offset, size).map_err(Into::into)
    }
}

#[derive(Clone)]
pub struct ExpectedInstance {
    instance: Instance,
    memory: Memory,
}

impl ExpectedInstance {
    pub fn copy_to_memory(&self, src: *const u8, size: usize) -> LappInstanceResult<u32> {
        let offset = self.alloc(size as _)?;
        if size as u64 <= self.memory.data_size() - offset as u64 {
            unsafe {
                copy_nonoverlapping(src, self.memory.data_ptr().offset(offset as _), size);
            }
            Ok(offset)
        } else {
            log::error!(
                "Destination offset = {} and buffer len = {}, but memory data size = {}",
                offset,
                size,
                self.memory.data_size()
            );
            Err(LappInstanceError::WrongBufferLength)
        }
    }

    pub unsafe fn move_from_memory(&self, offset: u32, size: usize) -> LappInstanceResult<Vec<u8>> {
        log::trace!(
            "Move from memory: data_ptr = {}, data_size = {}, offset = {}, size = {}",
            self.memory.data_ptr() as usize,
            self.memory.data_size(),
            offset,
            size
        );
        let data = slice::from_raw_parts(self.memory.data_ptr().offset(offset as _), size).into();
        self.dealloc(offset, size as _)?;
        Ok(data)
    }

    pub unsafe fn wasm_slice_to_vec(&self, slice: impl Into<WasmSlice>) -> LappInstanceResult<Vec<u8>> {
        let slice = slice.into();
        if slice.len() as u64 <= self.memory.data_size() - slice.ptr() as u64 {
            Ok(self.move_from_memory(slice.ptr(), slice.len() as _)?)
        } else {
            log::error!(
                "WASM slice ptr = {}, len = {}, but memory data size = {}",
                slice.ptr(),
                slice.len(),
                self.memory.data_size()
            );
            Err(LappInstanceError::WrongBufferLength)
        }
    }

    pub unsafe fn wasm_slice_to_string(&self, slice: impl Into<WasmSlice>) -> LappInstanceResult<String> {
        let data = self.wasm_slice_to_vec(slice)?;
        String::from_utf8(data).map_err(Into::into)
    }

    pub fn bytes_to_wasm_slice(&self, bytes: impl AsRef<[u8]>) -> LappInstanceResult<WasmSlice> {
        let bytes = bytes.as_ref();
        let offset = self.copy_to_memory(bytes.as_ptr(), bytes.len())?;
        Ok(WasmSlice::from((offset, bytes.len() as _)))
    }
}

impl Deref for ExpectedInstance {
    type Target = Instance;

    fn deref(&self) -> &Self::Target {
        &self.instance
    }
}

impl TryFrom<Instance> for ExpectedInstance {
    type Error = wasmer::ExportError;

    fn try_from(instance: Instance) -> Result<Self, Self::Error> {
        let memory = instance.exports.get_memory("memory")?.clone();

        Ok(Self { instance, memory })
    }
}

impl TryFrom<&Instance> for ExpectedInstance {
    type Error = wasmer::ExportError;

    fn try_from(instance: &Instance) -> Result<Self, Self::Error> {
        Self::try_from(instance.clone())
    }
}

impl TryFrom<Arc<Instance>> for ExpectedInstance {
    type Error = wasmer::ExportError;

    fn try_from(instance: Arc<Instance>) -> Result<Self, Self::Error> {
        Self::try_from(&*instance)
    }
}
