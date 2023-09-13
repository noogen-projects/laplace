use std::borrow::Cow;
use std::future::Future;
use std::ptr::copy_nonoverlapping;
use std::string::FromUtf8Error;

use anyhow::anyhow;
use laplace_wasm::WasmSlice;
use thiserror::Error;
use wasmtime::{AsContextMut, Instance, Memory, TypedFunc};

pub mod database;
pub mod http;
pub mod sleep;

pub type BoxedSendFuture<'a, T> = Box<dyn Future<Output = T> + Send + 'a>;

#[derive(Debug, Error)]
pub enum MemoryManagementError {
    #[error("Wasm error: {0}")]
    Wasmtime(#[from] anyhow::Error),

    #[error("Wasm memory has a wrong size")]
    WrongMemorySize,

    #[error("Wrong string data: {0}")]
    IntoStringError(#[from] FromUtf8Error),
}

pub type MemoryManagementResult<T> = Result<T, MemoryManagementError>;

#[derive(Clone)]
pub struct MemoryManagementHostData {
    memory: Memory,
    alloc_fn: TypedFunc<u32, u32>,
    dealloc_fn: TypedFunc<(u32, u32), ()>,
}

impl MemoryManagementHostData {
    pub fn new(memory: Memory, alloc_fn: TypedFunc<u32, u32>, dealloc_fn: TypedFunc<(u32, u32), ()>) -> Self {
        Self {
            memory,
            alloc_fn,
            dealloc_fn,
        }
    }

    pub fn from_instance(instance: &Instance, mut store: impl AsContextMut) -> anyhow::Result<Self> {
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow!("Memory is empty"))?;
        let alloc_fn = instance.get_typed_func(&mut store, "alloc")?;
        let dealloc_fn = instance.get_typed_func(store, "dealloc")?;

        Ok(Self::new(memory, alloc_fn, dealloc_fn))
    }

    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    pub fn to_manager<'a, S: AsContextMut>(&'a self, store: &'a mut S) -> MemoryManager<'a, S> {
        MemoryManager {
            host_data: Cow::Borrowed(self),
            store,
        }
    }

    pub fn into_manager<S: AsContextMut>(self, store: &mut S) -> MemoryManager<S> {
        MemoryManager {
            host_data: Cow::Owned(self),
            store,
        }
    }
}

pub struct MemoryManager<'a, S> {
    host_data: Cow<'a, MemoryManagementHostData>,
    store: &'a mut S,
}

impl<'a, S> MemoryManager<'a, S>
where
    S: AsContextMut,
    S::Data: Send,
{
    pub fn memory(&self) -> &Memory {
        &self.host_data.memory
    }

    pub async fn memory_grow(&mut self, pages: u64) -> anyhow::Result<u64> {
        self.host_data.memory.grow_async(&mut self.store, pages).await
    }

    pub fn is_memory_enough(&self, offset: usize, size: usize) -> bool {
        size <= self.memory().data_size(&self.store) - offset
    }

    pub async fn grow_memory_if_needed(&mut self, offset: usize, size: usize) -> anyhow::Result<()> {
        while !self.is_memory_enough(offset, size) {
            log::trace!(
                "Destination offset = {} and buffer len = {}, but memory data size = {}",
                offset,
                size,
                self.memory().data_size(&self.store)
            );
            self.memory_grow(1).await?;
        }
        Ok(())
    }

    pub async fn copy_to_memory(&mut self, src_bytes: &[u8]) -> MemoryManagementResult<u32> {
        let size = src_bytes.len();
        let offset = self.alloc(size as _).await?;
        self.grow_memory_if_needed(offset as _, size).await?;

        // SAFETY: in this point memory has a required space
        unsafe {
            copy_nonoverlapping(
                src_bytes.as_ptr(),
                self.memory().data_ptr(&self.store).offset(offset as _),
                size,
            );
        }

        Ok(offset)
    }

    pub async fn move_from_memory(&mut self, offset: usize, size: usize) -> MemoryManagementResult<Vec<u8>> {
        let memory = self.memory();
        log::trace!(
            "Move from memory: data_ptr = {}, data_size = {}, offset = {}, size = {}",
            memory.data_ptr(&self.store) as usize,
            memory.data_size(&self.store),
            offset,
            size
        );

        let data = memory.data(&self.store)[offset..(offset + size)].to_vec();
        unsafe { self.dealloc(offset as _, size as _).await? };

        Ok(data)
    }

    pub async fn wasm_slice_to_vec(&mut self, slice: impl Into<WasmSlice>) -> MemoryManagementResult<Vec<u8>> {
        let slice = slice.into();
        let ptr = slice.ptr() as _;
        let len = slice.len() as _;

        if self.is_memory_enough(ptr, len) {
            self.move_from_memory(ptr, len).await
        } else {
            log::error!(
                "WASM slice ptr = {ptr}, len = {len}, but memory data size = {}",
                self.memory().data_size(&self.store)
            );
            Err(MemoryManagementError::WrongMemorySize)
        }
    }

    pub async fn wasm_slice_to_string(&mut self, slice: impl Into<WasmSlice>) -> MemoryManagementResult<String> {
        let data = self.wasm_slice_to_vec(slice).await?;
        String::from_utf8(data).map_err(Into::into)
    }

    pub async fn bytes_to_wasm_slice(&mut self, bytes: impl AsRef<[u8]>) -> MemoryManagementResult<WasmSlice> {
        let bytes = bytes.as_ref();
        let offset = self.copy_to_memory(bytes).await?;
        Ok(WasmSlice::from((offset, bytes.len() as _)))
    }

    async fn alloc(&mut self, size: u32) -> anyhow::Result<u32> {
        self.host_data.alloc_fn.call_async(&mut self.store, size).await
    }

    async unsafe fn dealloc(&mut self, offset: u32, size: u32) -> anyhow::Result<()> {
        log::trace!("Dealloc: offset = {offset}, size = {size}");
        self.host_data
            .dealloc_fn
            .call_async(&mut self.store, (offset, size))
            .await
    }
}
