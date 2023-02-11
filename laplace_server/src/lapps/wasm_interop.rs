use std::borrow::Cow;
use std::ptr::copy_nonoverlapping;
use std::slice;
use std::string::FromUtf8Error;

use laplace_wasm::WasmSlice;
use thiserror::Error;
use wasmer::{
    AsStoreMut, AsStoreRef, ExportError, Exports, Memory, MemoryError, MemoryView, Pages, RuntimeError, TypedFunction,
};

pub mod database;
pub mod http;
pub mod sleep;

#[derive(Debug, Error)]
pub enum MemoryManagementError {
    #[error("Wasm runtime error: {0}")]
    Runtime(#[from] RuntimeError),

    #[error("Wasm memory error: {0}")]
    Memory(#[from] MemoryError),

    #[error("Wasm memory has a wrong size")]
    WrongMemorySize,

    #[error("Wrong string data: {0}")]
    IntoStringError(#[from] FromUtf8Error),
}

pub type MemoryManagementResult<T> = Result<T, MemoryManagementError>;

#[derive(Clone)]
pub struct MemoryManagementHostData {
    memory: Memory,
    alloc_fn: TypedFunction<u32, u32>,
    dealloc_fn: TypedFunction<(u32, u32), ()>,
}

impl MemoryManagementHostData {
    pub fn new(memory: Memory, alloc_fn: TypedFunction<u32, u32>, dealloc_fn: TypedFunction<(u32, u32), ()>) -> Self {
        Self {
            memory,
            alloc_fn,
            dealloc_fn,
        }
    }

    pub fn from_exports(exports: &Exports, store: &impl AsStoreRef) -> Result<Self, ExportError> {
        let memory = exports.get_memory("memory")?.clone();
        let alloc_fn = exports.get_typed_function(store, "alloc")?;
        let dealloc_fn = exports.get_typed_function(store, "dealloc")?;

        Ok(Self::new(memory, alloc_fn, dealloc_fn))
    }

    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    pub fn to_manager<'a, S: AsStoreMut>(&'a self, store: &'a mut S) -> MemoryManager<'a, S> {
        MemoryManager {
            host_data: Cow::Borrowed(self),
            store,
        }
    }

    pub fn into_manager<S: AsStoreMut>(self, store: &mut S) -> MemoryManager<S> {
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

impl<'a, S: AsStoreMut> MemoryManager<'a, S> {
    pub fn memory_view(&self) -> MemoryView {
        self.host_data.memory.view(self.store)
    }

    pub fn memory_grow(&mut self, pages: u32) -> Result<Pages, MemoryError> {
        self.host_data.memory.grow(&mut self.store, pages)
    }

    pub fn is_memory_enough(&self, size: u64, offset: u64) -> bool {
        size <= self.memory_view().data_size() - offset
    }

    pub fn grow_memory_if_needed(&mut self, size: u64, offset: u64) -> Result<(), MemoryError> {
        while !self.is_memory_enough(size as _, offset as _) {
            log::trace!(
                "Destination offset = {} and buffer len = {}, but memory data size = {}",
                offset,
                size,
                self.memory_view().data_size()
            );
            self.memory_grow(1)?;
        }
        Ok(())
    }

    pub fn copy_to_memory(&mut self, src: *const u8, size: usize) -> MemoryManagementResult<u32> {
        let offset = self.alloc(size as _)?;
        self.grow_memory_if_needed(size as _, offset as _)?;

        // SAFETY: in this point memory has a required space
        unsafe {
            copy_nonoverlapping(src, self.memory_view().data_ptr().offset(offset as _), size);
        }

        Ok(offset)
    }

    pub unsafe fn move_from_memory(&mut self, offset: u32, size: usize) -> MemoryManagementResult<Vec<u8>> {
        let memory_view = self.memory_view();
        log::trace!(
            "Move from memory: data_ptr = {}, data_size = {}, offset = {}, size = {}",
            memory_view.data_ptr() as usize,
            memory_view.data_size(),
            offset,
            size
        );

        let data = slice::from_raw_parts(memory_view.data_ptr().offset(offset as _), size).into();
        self.dealloc(offset, size as _)?;

        Ok(data)
    }

    pub unsafe fn wasm_slice_to_vec(&mut self, slice: impl Into<WasmSlice>) -> MemoryManagementResult<Vec<u8>> {
        let slice = slice.into();
        if self.is_memory_enough(slice.len() as _, slice.ptr() as _) {
            let data = self.move_from_memory(slice.ptr(), slice.len() as _)?;

            Ok(data)
        } else {
            log::error!(
                "WASM slice ptr = {}, len = {}, but memory data size = {}",
                slice.ptr(),
                slice.len(),
                self.memory_view().data_size()
            );
            Err(MemoryManagementError::WrongMemorySize)
        }
    }

    pub unsafe fn wasm_slice_to_string(&mut self, slice: impl Into<WasmSlice>) -> MemoryManagementResult<String> {
        let data = self.wasm_slice_to_vec(slice)?;
        String::from_utf8(data).map_err(Into::into)
    }

    pub fn bytes_to_wasm_slice(&mut self, bytes: impl AsRef<[u8]>) -> MemoryManagementResult<WasmSlice> {
        let bytes = bytes.as_ref();
        let offset = self.copy_to_memory(bytes.as_ptr(), bytes.len())?;
        Ok(WasmSlice::from((offset, bytes.len() as _)))
    }

    fn alloc(&mut self, size: u32) -> Result<u32, RuntimeError> {
        self.host_data.alloc_fn.call(&mut self.store, size)
    }

    unsafe fn dealloc(&mut self, offset: u32, size: u32) -> Result<(), RuntimeError> {
        log::trace!("Dealloc: offset = {offset}, size = {size}");
        self.host_data.dealloc_fn.call(&mut self.store, offset, size)
    }
}
