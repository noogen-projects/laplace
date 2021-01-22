use dapla_wasm::WasmSlice;

#[no_mangle]
pub fn get(uri_ptr: *const u8, uri_len: usize) -> WasmSlice {
    static mut RESULT: String = String::new();

    let uri = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(uri_ptr, uri_len)) };
    unsafe {
        RESULT = String::from("Echo ");
        RESULT.push_str(uri);
        WasmSlice::from(RESULT.as_str())
    }
}
