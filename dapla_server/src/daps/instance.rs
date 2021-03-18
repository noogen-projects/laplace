use std::{ptr::copy_nonoverlapping, slice};

use thiserror::Error;
use wasmer::{Instance, Memory};

#[derive(Debug, Error)]
pub enum DapInstanceError {
    #[error("Get instance export: {0}")]
    ExportError(#[from] wasmer::ExportError),

    #[error("Execution abort: {0}")]
    RuntimeError(#[from] wasmer::RuntimeError),
}

pub type DapInstanceResult<T> = Result<T, DapInstanceError>;

pub trait DapInstance {
    unsafe fn alloc(&self, size: u32) -> DapInstanceResult<u32>;
    unsafe fn dealloc(&self, offset: u32, size: u32) -> DapInstanceResult<()>;
    unsafe fn copy_to(&self, memory: &Memory, src: *const u8, size: usize) -> DapInstanceResult<u32>;
    unsafe fn move_from(&self, memory: &Memory, offset: u32, size: usize) -> DapInstanceResult<Vec<u8>>;
}

impl DapInstance for Instance {
    unsafe fn alloc(&self, size: u32) -> DapInstanceResult<u32> {
        let alloc_fn = self.exports.get_function("alloc")?.native::<u32, u32>()?;
        alloc_fn.call(size).map_err(Into::into)
    }

    unsafe fn dealloc(&self, offset: u32, size: u32) -> DapInstanceResult<()> {
        log::trace!("Dealloc: offset = {}, size = {}", offset, size);
        let dealloc_fn = self.exports.get_function("dealloc")?.native::<(u32, u32), ()>()?;
        dealloc_fn.call(offset, size).map_err(Into::into)
    }

    unsafe fn copy_to(&self, memory: &Memory, src: *const u8, size: usize) -> DapInstanceResult<u32> {
        let offset = self.alloc(size as _)?;
        copy_nonoverlapping(src, memory.data_ptr().offset(offset as _), size);
        Ok(offset)
    }

    unsafe fn move_from(&self, memory: &Memory, offset: u32, size: usize) -> DapInstanceResult<Vec<u8>> {
        log::trace!(
            "Move from memory: data_ptr = {}, data_size = {}, offset = {}, size = {}",
            memory.data_ptr() as usize,
            memory.data_size(),
            offset,
            size
        );
        let data = slice::from_raw_parts(memory.data_ptr().offset(offset as _), size).into();
        self.dealloc(offset, size as _)?;
        Ok(data)
    }
}
