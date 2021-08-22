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

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub unsafe fn into_string_in_wasm(self) -> String {
        String::from_raw_parts(self.ptr() as *mut _, self.len() as usize, self.len() as usize)
    }

    pub unsafe fn into_vec_in_wasm(self) -> Vec<u8> {
        Vec::from_raw_parts(self.ptr() as *mut _, self.len() as usize, self.len() as usize)
    }
}

impl From<WasmSlice> for u64 {
    fn from(slice: WasmSlice) -> Self {
        slice.0
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

impl From<String> for WasmSlice {
    fn from(string: String) -> Self {
        let len = string.len() as u32;
        let ptr = Box::into_raw(string.into_boxed_str()) as *const u8 as u32;
        Self::from((ptr, len))
    }
}

impl From<Vec<u8>> for WasmSlice {
    fn from(bytes: Vec<u8>) -> Self {
        let len = bytes.len() as u32;
        let ptr = Box::into_raw(bytes.into_boxed_slice()) as *const u8 as u32;
        Self::from((ptr, len))
    }
}
