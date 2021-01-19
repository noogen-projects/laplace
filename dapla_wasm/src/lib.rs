#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct WasmSlice(u64);

impl WasmSlice {
    pub fn ptr(&self) -> u32 {
        (self.0 >> 32) as _
    }

    pub fn len(&self) -> u32 {
        (self.0 & 0x00000000ffffffff) as _
    }
}

impl From<u64> for WasmSlice {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<(u32, u32)> for WasmSlice {
    fn from((ptr, len): (u32, u32)) -> Self {
        Self(((ptr as u64) << 32) | len as u64)
    }
}

impl<T> From<&[T]> for WasmSlice {
    fn from(slice: &[T]) -> Self {
        Self::from((slice.as_ptr() as u32, slice.len() as u32))
    }
}

impl From<&str> for WasmSlice {
    fn from(string: &str) -> Self {
        let ptr = string.as_ptr() as u32;
        let len = string.len() as u32;
        Self::from((ptr, len))
    }
}
